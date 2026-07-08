//! Reversible StarfieldCustom.ini loose-file activation (SFINI-01..05).
//!
//! This is the FIRST sanctioned engine write OUTSIDE the `Data/` deploy root. It does
//! NOT relax the `Data/`-root guard: it resolves its target independently via the
//! Phase-6 hardened `steam::my_games_path` resolver and rides its own journal `kind`
//! token (`journal::KIND_INI`) on a bare `StarfieldCustom.ini` sentinel that can never
//! collide with a `Data/`-rooted manifest relpath.
//!
//! The reversibility substrate is REUSED verbatim: the content-addressed vanilla backup
//! ledger (`backup::{backup_vanilla_if_absent, restore_vanilla}`), the intent-before-act
//! operation journal, and the bottom-up empty-dir prune discipline. Only *where the
//! target resolves* changes.
//!
//! The one genuinely new piece is a std-only surgical INI byte editor ([`editor`]) that
//! merges NexTwist's two owned `[Archive]` keys into an existing file while preserving
//! its BOM, EOL style, comments, key order, and every untouched section byte-for-byte —
//! something no INI *parser* crate round-trips (RESEARCH rejected `rust-ini` on exactly
//! these grounds).

use serde::{Deserialize, Serialize};

// ============================================================================
// CE2 loose-file activation recipe — ASSUMPTION A1 (MEDIUM confidence).
//
// These four constants ARE the recipe: the `[Archive]` section plus the two owned keys
// `bInvalidateOlderFiles=1` and an empty `sResourceDataDirsFinal=`. This is validated
// AGAINST-A-BUILD, not a permanent constant — Phase 9's on-hardware gate (SFVER-01)
// corrects the recipe by editing THIS block ONLY, without touching the reversibility
// machinery around it. Keep the recipe here, in one labelled place.
// ============================================================================

/// The section that owns the loose-file loading keys.
const INI_SECTION: &str = "Archive";
/// NexTwist-owned key: invalidate the archive cache so loose files win.
const KEY_INVALIDATE: &str = "bInvalidateOlderFiles";
/// The value NexTwist writes for [`KEY_INVALIDATE`].
const VAL_INVALIDATE: &str = "1";
/// NexTwist-owned key: the loose-file resource dirs list (empty = engine default order).
const KEY_RESOURCE_DIRS: &str = "sResourceDataDirsFinal";
/// The value NexTwist writes for [`KEY_RESOURCE_DIRS`] (empty — we only own an empty value).
const VAL_RESOURCE_DIRS: &str = "";

/// The bare relative path used as BOTH the on-disk INI filename and the journal /
/// vanilla-ledger sentinel key. Deliberately has NO `Data/` prefix so it can never
/// collide with a `Data/`-rooted deploy-manifest relpath (Pitfall 2).
pub const INI_FILENAME: &str = "StarfieldCustom.ini";

// ---------------------------------------------------------------------------
// Public serde types the op wrappers + Tauri boundary use.
// ---------------------------------------------------------------------------

/// How to resolve a pre-existing NON-EMPTY user `sResourceDataDirsFinal` on activation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IniConflictResolution {
    /// Refuse to clobber a non-empty user value — surface it as a conflict (the default).
    Block,
    /// The user explicitly chose to overwrite their value with NexTwist's (empty) value.
    UseNexTwist,
}

/// A read-only preview of what activation WOULD do, without writing (SFINI-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IniActivationPreview {
    /// The INI does not exist yet — activation would create it (CRLF, no BOM).
    pub will_create: bool,
    /// The INI exists — activation would surgically merge into it.
    pub will_edit: bool,
    /// The exact two `key=value` lines NexTwist would ensure.
    pub lines: Vec<String>,
    /// `Some(current_value)` if a pre-existing NON-EMPTY user `sResourceDataDirsFinal`
    /// would block auto-activation; `None` otherwise.
    pub conflict: Option<String>,
}

/// The outcome of an [`ensure_ini_active`] / [`restore_ini`] op.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum IniOutcome {
    /// The INI was created/merged so loose-file loading is now active.
    Activated,
    /// The INI already carried NexTwist's exact activation — nothing was written.
    AlreadyActive,
    /// A non-empty user `sResourceDataDirsFinal` blocked auto-activation (nothing written).
    Blocked {
        /// The user's current value the caller must resolve.
        current_value: String,
    },
    /// Activation was reverted to the recorded provenance (bytes restored or file removed).
    Restored,
    /// The INI was never activated by NexTwist — restore was a safe no-op.
    NotActive,
}

// ---------------------------------------------------------------------------
// The surgical std-only INI byte editor (pure — NO file I/O lives here).
// ---------------------------------------------------------------------------

// ponytail: the op wrappers (Task 2) are the only non-test callers of this module; keep
// the pure editor self-contained. The module-level allow is removed once they land.
#[allow(dead_code)]
pub(crate) mod editor {
    use super::{
        INI_SECTION, IniConflictResolution, KEY_INVALIDATE, KEY_RESOURCE_DIRS, VAL_INVALIDATE,
        VAL_RESOURCE_DIRS,
    };

    /// The result of planning a merge: either the exact bytes to write, or a detected
    /// non-empty user-value conflict (carrying that value).
    pub(crate) enum MergePlan {
        /// The full byte vector to write (BOM + merged body).
        Write(Vec<u8>),
        /// A non-empty user `sResourceDataDirsFinal` blocked the merge under `Block`.
        Conflict(String),
    }

    /// Length (in bytes) of a leading byte-order mark: UTF-8 (EF BB BF), UTF-16 LE
    /// (FF FE), or UTF-16 BE (FE FF). `0` when there is none. The BOM bytes are preserved
    /// verbatim as a saved prefix and re-prepended on output.
    pub(crate) fn detect_bom(bytes: &[u8]) -> usize {
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            3
        } else if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
            2
        } else {
            0
        }
    }

    /// Split `body` into raw lines, EACH INCLUDING its trailing terminator (`\r\n`, `\n`,
    /// or none for a final unterminated line). Concatenating the result reproduces `body`
    /// byte-for-byte — the basis of the byte-fidelity guarantee.
    pub(crate) fn raw_lines(body: &[u8]) -> Vec<&[u8]> {
        let mut out = Vec::new();
        let mut start = 0;
        for (i, &b) in body.iter().enumerate() {
            if b == b'\n' {
                out.push(&body[start..=i]);
                start = i + 1;
            }
        }
        if start < body.len() {
            out.push(&body[start..]);
        }
        out
    }

    /// The line's content with its trailing EOL removed (bytes only, no trim).
    fn strip_eol(line: &[u8]) -> &[u8] {
        let mut e = line.len();
        if e > 0 && line[e - 1] == b'\n' {
            e -= 1;
            if e > 0 && line[e - 1] == b'\r' {
                e -= 1;
            }
        }
        &line[..e]
    }

    /// The line's exact trailing EOL bytes (`\r\n`, `\n`, or empty).
    fn eol_of(line: &[u8]) -> &[u8] {
        if line.ends_with(b"\r\n") {
            b"\r\n"
        } else if line.ends_with(b"\n") {
            b"\n"
        } else {
            b""
        }
    }

    /// ASCII-whitespace-trim a byte slice.
    fn trim(b: &[u8]) -> &[u8] {
        let mut s = 0;
        let mut e = b.len();
        while s < e && b[s].is_ascii_whitespace() {
            s += 1;
        }
        while e > s && b[e - 1].is_ascii_whitespace() {
            e -= 1;
        }
        &b[s..e]
    }

    /// Is this trimmed content a `[Section]` header?
    fn is_section(trimmed: &[u8]) -> bool {
        trimmed.first() == Some(&b'[') && trimmed.last() == Some(&b']') && trimmed.len() >= 2
    }

    /// The name inside a `[Section]` header (assumes [`is_section`]).
    fn section_name(trimmed: &[u8]) -> &[u8] {
        &trimmed[1..trimmed.len() - 1]
    }

    /// Split a trimmed `key = value` line into (trimmed key, trimmed value).
    fn split_key(trimmed: &[u8]) -> Option<(&[u8], &[u8])> {
        let eq = trimmed.iter().position(|&c| c == b'=')?;
        Some((trim(&trimmed[..eq]), trim(&trimmed[eq + 1..])))
    }

    /// The dominant EOL of a body: CRLF if any `\r\n` is present, else LF.
    fn dominant_eol(body: &[u8]) -> &'static [u8] {
        if body.windows(2).any(|w| w == b"\r\n") {
            b"\r\n"
        } else {
            b"\n"
        }
    }

    /// Read the current trimmed `sResourceDataDirsFinal` value inside `[Archive]`, if the
    /// key is present. `Some("")` for an explicit empty value, `None` if the key/section
    /// is absent. BOM-aware.
    pub(crate) fn current_resource_value(bytes: &[u8]) -> Option<String> {
        let body = &bytes[detect_bom(bytes)..];
        let raw = raw_lines(body);
        let h = raw
            .iter()
            .position(|ln| is_archive_header(ln))?;
        for ln in raw.iter().skip(h + 1) {
            let t = trim(strip_eol(ln));
            if is_section(t) {
                break;
            }
            if let Some((k, v)) = split_key(t)
                && k.eq_ignore_ascii_case(KEY_RESOURCE_DIRS.as_bytes())
            {
                return Some(String::from_utf8_lossy(v).into_owned());
            }
        }
        None
    }

    /// `Some(value)` if there is a pre-existing NON-EMPTY user `sResourceDataDirsFinal`
    /// (a conflict); `None` for an empty/absent value.
    pub(crate) fn detect_conflict(current: Option<&[u8]>) -> Option<String> {
        let v = current.and_then(current_resource_value)?;
        (!v.is_empty()).then_some(v)
    }

    /// The exact two `key=value` lines NexTwist ensures (for previews).
    pub(crate) fn owned_lines() -> Vec<String> {
        vec![
            format!("{KEY_INVALIDATE}={VAL_INVALIDATE}"),
            format!("{KEY_RESOURCE_DIRS}={VAL_RESOURCE_DIRS}"),
        ]
    }

    fn is_archive_header(line: &[u8]) -> bool {
        let t = trim(strip_eol(line));
        is_section(t) && section_name(t).eq_ignore_ascii_case(INI_SECTION.as_bytes())
    }

    /// Plan the merge of NexTwist's two owned keys into `current` (or a fresh file when
    /// `None`), honoring `resolution` for a conflicting non-empty user value.
    ///
    /// * `None` → a brand-new file: `[Archive]` + the two keys, CRLF + NO BOM.
    /// * `Some(bytes)` → preserve the BOM + dominant EOL; merge in place into an existing
    ///   `[Archive]` (case-insensitive) touching ONLY the two owned keys, or append exactly
    ///   one `[Archive]` block when the section is absent; every other byte is untouched.
    ///
    /// Idempotent: planning against this function's own output yields byte-identical bytes.
    pub(crate) fn plan_merge(current: Option<&[u8]>, resolution: IniConflictResolution) -> MergePlan {
        let Some(bytes) = current else {
            // New file: CRLF + no BOM (CE2/Windows convention, Assumption A2).
            let s = format!(
                "[{INI_SECTION}]\r\n{KEY_INVALIDATE}={VAL_INVALIDATE}\r\n{KEY_RESOURCE_DIRS}={VAL_RESOURCE_DIRS}\r\n"
            );
            return MergePlan::Write(s.into_bytes());
        };
        let bom_len = detect_bom(bytes);
        let bom = &bytes[..bom_len];
        let body = &bytes[bom_len..];
        let eol = dominant_eol(body);
        match merge_body(body, eol, resolution) {
            Err(conflict) => MergePlan::Conflict(conflict),
            Ok(new_body) => {
                let mut out = Vec::with_capacity(bom.len() + new_body.len());
                out.extend_from_slice(bom);
                out.extend_from_slice(&new_body);
                MergePlan::Write(out)
            }
        }
    }

    /// Merge the two owned keys into `body` (BOM already stripped). `Err(value)` iff a
    /// non-empty user `sResourceDataDirsFinal` is present under `Block`.
    fn merge_body(
        body: &[u8],
        eol: &[u8],
        resolution: IniConflictResolution,
    ) -> Result<Vec<u8>, String> {
        let raw = raw_lines(body);
        let inv_line = format!("{KEY_INVALIDATE}={VAL_INVALIDATE}");
        let res_line = format!("{KEY_RESOURCE_DIRS}={VAL_RESOURCE_DIRS}");

        let Some(h) = raw.iter().position(|ln| is_archive_header(ln)) else {
            // No [Archive] section: append exactly one block using the file's EOL, leaving
            // every prior byte untouched (only add a separating EOL if the file did not end
            // with a newline).
            let mut out = body.to_vec();
            if !out.is_empty() && !out.ends_with(b"\n") {
                out.extend_from_slice(eol);
            }
            out.extend_from_slice(format!("[{INI_SECTION}]").as_bytes());
            out.extend_from_slice(eol);
            out.extend_from_slice(inv_line.as_bytes());
            out.extend_from_slice(eol);
            out.extend_from_slice(res_line.as_bytes());
            out.extend_from_slice(eol);
            return Ok(out);
        };

        // Section extent: from just after the header up to the next section header or EOF.
        let end = raw
            .iter()
            .enumerate()
            .skip(h + 1)
            .find(|(_, ln)| is_section(trim(strip_eol(ln))))
            .map(|(j, _)| j)
            .unwrap_or(raw.len());

        // Locate the two owned keys within [h+1, end); detect a conflicting user value.
        let mut inv_at = None;
        let mut res_at = None;
        for (j, ln) in raw.iter().enumerate().take(end).skip(h + 1) {
            let t = trim(strip_eol(ln));
            let Some((k, v)) = split_key(t) else { continue };
            if inv_at.is_none() && k.eq_ignore_ascii_case(KEY_INVALIDATE.as_bytes()) {
                inv_at = Some(j);
            } else if res_at.is_none() && k.eq_ignore_ascii_case(KEY_RESOURCE_DIRS.as_bytes()) {
                res_at = Some(j);
                if matches!(resolution, IniConflictResolution::Block) && !v.is_empty() {
                    return Err(String::from_utf8_lossy(v).into_owned());
                }
            }
        }

        // Owned mutable copy; update existing keys in place (preserving their own EOL) and
        // insert any missing key just after the header.
        let mut out: Vec<Vec<u8>> = raw.iter().map(|s| s.to_vec()).collect();
        if let Some(j) = inv_at {
            let term = eol_of(&out[j]).to_vec();
            out[j] = [inv_line.as_bytes(), term.as_slice()].concat();
        }
        if let Some(j) = res_at {
            let term = eol_of(&out[j]).to_vec();
            out[j] = [res_line.as_bytes(), term.as_slice()].concat();
        }

        let mut inserts: Vec<Vec<u8>> = Vec::new();
        if inv_at.is_none() {
            inserts.push([inv_line.as_bytes(), eol].concat());
        }
        if res_at.is_none() {
            inserts.push([res_line.as_bytes(), eol].concat());
        }
        if !inserts.is_empty() {
            // The header line must carry a terminator before we insert lines after it.
            if eol_of(&out[h]).is_empty() {
                out[h].extend_from_slice(eol);
            }
            for (k, ins) in inserts.into_iter().enumerate() {
                out.insert(h + 1 + k, ins);
            }
        }
        Ok(out.concat())
    }
}

#[cfg(test)]
mod editor_tests {
    use super::IniConflictResolution::{Block, UseNexTwist};
    use super::editor::*;

    fn planned(current: Option<&[u8]>, r: super::IniConflictResolution) -> Vec<u8> {
        match plan_merge(current, r) {
            MergePlan::Write(b) => b,
            MergePlan::Conflict(v) => panic!("unexpected conflict: {v}"),
        }
    }

    #[test]
    fn detect_bom_utf8_utf16_and_none() {
        assert_eq!(detect_bom(&[0xEF, 0xBB, 0xBF, b'x']), 3, "utf-8 BOM");
        assert_eq!(detect_bom(&[0xFF, 0xFE, b'x']), 2, "utf-16 LE BOM");
        assert_eq!(detect_bom(&[0xFE, 0xFF, b'x']), 2, "utf-16 BE BOM");
        assert_eq!(detect_bom(b"[Archive]\r\n"), 0, "no BOM stays BOM-less");
    }

    #[test]
    fn new_file_is_crlf_no_bom_exact() {
        let out = planned(None, Block);
        assert_eq!(
            out,
            b"[Archive]\r\nbInvalidateOlderFiles=1\r\nsResourceDataDirsFinal=\r\n"
        );
        assert_eq!(detect_bom(&out), 0, "a new file carries no BOM");
    }

    #[test]
    fn merge_into_existing_archive_preserves_everything_else() {
        // Mixed-case section header, unrelated keys/comments/blank lines, LF EOL.
        let input = b"; my config\n[Display]\niSize W=2560\n\n[archive]\nbUseArchives=1\n";
        let out = planned(Some(input), Block);
        let text = String::from_utf8(out).unwrap();
        // Exactly one [Archive]/[archive] header, in place (no second appended block).
        assert_eq!(
            text.to_ascii_lowercase().matches("[archive]").count(),
            1,
            "must not append a second [archive]"
        );
        // Every unrelated line preserved verbatim, in order.
        assert!(text.starts_with("; my config\n[Display]\niSize W=2560\n\n[archive]\n"));
        assert!(text.contains("bUseArchives=1\n"), "unrelated key preserved");
        // The two owned keys inserted just after the header, LF EOL honored.
        assert!(text.contains("bInvalidateOlderFiles=1\n"));
        assert!(text.contains("sResourceDataDirsFinal=\n"));
        assert!(!text.contains("\r\n"), "LF file must stay LF");
    }

    #[test]
    fn merge_without_archive_appends_one_block_with_detected_eol() {
        let input = b"[General]\r\nsLang=en\r\n"; // CRLF file, no [Archive]
        let out = planned(Some(input), Block);
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("[General]\r\nsLang=en\r\n"), "prior bytes untouched");
        assert!(text.contains("[Archive]\r\nbInvalidateOlderFiles=1\r\nsResourceDataDirsFinal=\r\n"));
        assert_eq!(text.matches("[Archive]").count(), 1);
    }

    #[test]
    fn idempotent_remerge_is_byte_identical_single_archive() {
        let first = planned(Some(b"[archive]\nkeep=1\n"), Block);
        let second = planned(Some(&first), Block);
        assert_eq!(first, second, "re-merge must be byte-identical");
        let text = String::from_utf8(second).unwrap();
        assert_eq!(text.to_ascii_lowercase().matches("[archive]").count(), 1);
    }

    #[test]
    fn crlf_bom_noop_roundtrip_is_byte_for_byte_identical() {
        // A CRLF + UTF-8 BOM file that ALREADY carries both owned keys must round-trip
        // byte-for-byte (SFINI-03b).
        let mut input = vec![0xEF, 0xBB, 0xBF];
        input.extend_from_slice(
            b"[Archive]\r\nbInvalidateOlderFiles=1\r\nsResourceDataDirsFinal=\r\n[Other]\r\nx=1\r\n",
        );
        let out = planned(Some(&input), Block);
        assert_eq!(out, input, "no-op re-merge of a CRLF+BOM file must be identical");
        assert_eq!(detect_bom(&out), 3, "BOM preserved");
    }

    #[test]
    fn existing_value_is_updated_in_place_preserving_its_crlf() {
        // A stale bInvalidateOlderFiles=0 is corrected to 1 without touching CRLF/order.
        let input = b"[Archive]\r\nbInvalidateOlderFiles=0\r\nsResourceDataDirsFinal=\r\n";
        let out = planned(Some(input), Block);
        assert_eq!(out, b"[Archive]\r\nbInvalidateOlderFiles=1\r\nsResourceDataDirsFinal=\r\n");
    }

    #[test]
    fn nonempty_user_value_is_a_conflict_under_block() {
        let input = b"[Archive]\nsResourceDataDirsFinal=Textures\\\n";
        match plan_merge(Some(input), Block) {
            MergePlan::Conflict(v) => assert_eq!(v, "Textures\\"),
            MergePlan::Write(_) => panic!("must block a non-empty user value"),
        }
        // detect_conflict agrees.
        assert_eq!(detect_conflict(Some(input)).as_deref(), Some("Textures\\"));
    }

    #[test]
    fn empty_user_value_is_not_a_conflict() {
        let input = b"[Archive]\nsResourceDataDirsFinal=  \n"; // whitespace-only = empty
        assert!(detect_conflict(Some(input)).is_none());
        // And it plans a clean write.
        let _ = planned(Some(input), Block);
    }

    #[test]
    fn use_nextwist_overwrites_a_conflicting_value() {
        let input = b"[Archive]\r\nsResourceDataDirsFinal=Textures\\\r\n";
        let out = planned(Some(input), UseNexTwist);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("sResourceDataDirsFinal=\r\n"), "value cleared to empty");
        assert!(!text.contains("Textures"), "user value overwritten under UseNexTwist");
    }

    #[test]
    fn owned_lines_are_the_two_recipe_keys() {
        assert_eq!(
            owned_lines(),
            vec!["bInvalidateOlderFiles=1".to_string(), "sResourceDataDirsFinal=".to_string()]
        );
    }
}
