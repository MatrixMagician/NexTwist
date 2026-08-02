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

use std::fs;
use std::path::{Path, PathBuf};

use nextwist_core::Game;
use serde::{Deserialize, Serialize};
use store::Store;

use crate::backup;
use crate::error::DeployError;
use crate::journal;

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
/// collide with a `Data/`-rooted deploy-manifest relpath.
pub const INI_FILENAME: &str = "StarfieldCustom.ini";

/// Reserved `vanilla_backup.hash` value meaning "NexTwist CREATED this INI — it did not
/// pre-exist". Not a 64-char blake3 hex, so provenance is three-valued and never inferred
/// from disk:
///   * NO row              → NexTwist never activated → restore is a safe no-op.
///   * this ABSENCE_MARKER → CreatedByNexTwist        → restore deletes + prunes.
///   * a real blake3 hash  → PreExisting               → restore copies original bytes back.
const ABSENCE_MARKER: &str = "nextwist:created-absent";

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

/// A read-only preview of what activation WOULD do, without writing.
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
    /// The on-disk INI carries a UTF-16/UTF-32 BOM the byte-oriented editor refuses to edit
    ///. Nothing was written — the user's file is left byte-for-byte
    /// intact rather than corrupted with a mixed-encoding UTF-8 block appended.
    UnsupportedEncoding,
}

/// How the on-disk StarfieldCustom.ini has drifted from its recorded-active state while
/// loose files are deployed — the INI analogue of `verify`'s Data/ drift
/// buckets. `Missing` = the INI should be active but is absent; `Changed` = present but no
/// longer carrying NexTwist's exact activation. A user-blocked conflict is NOT drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IniDrift {
    /// The INI should be active (loose files deployed) but is absent on disk.
    Missing,
    /// The INI is present but no longer carries NexTwist's exact activation.
    Changed,
}

// ---------------------------------------------------------------------------
// The surgical std-only INI byte editor (pure — NO file I/O lives here).
// ---------------------------------------------------------------------------

// The pure editor is consumed by the op wrappers below (and its unit tests).
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
        /// A UTF-16/UTF-32 BOM was detected: this byte-oriented editor cannot safely parse
        /// or edit a multi-byte-encoded INI, so it refuses to touch it.
        Unsupported,
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

    /// A multi-byte Unicode BOM (UTF-16 LE/BE or UTF-32 LE/BE) this byte-oriented editor
    /// cannot safely parse or edit — every ASCII char is interleaved with NUL, so
    /// `[Archive]` / `sResourceDataDirsFinal=` never match and a UTF-8 block would corrupt
    /// the file. The only encodings we own are UTF-8 and BOM-less ANSI/UTF-8; refuse the
    /// rest. (UTF-32 LE `FF FE 00 00` shares the UTF-16 LE `FF FE` prefix.)
    pub(crate) fn is_unsupported_bom(bytes: &[u8]) -> bool {
        bytes.starts_with(&[0xFF, 0xFE]) // UTF-16 LE / UTF-32 LE
            || bytes.starts_with(&[0xFE, 0xFF]) // UTF-16 BE
            || bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) // UTF-32 BE
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
        let h = raw.iter().position(|ln| is_archive_header(ln))?;
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
    pub(crate) fn plan_merge(
        current: Option<&[u8]>,
        resolution: IniConflictResolution,
    ) -> MergePlan {
        let Some(bytes) = current else {
            // New file: CRLF + no BOM (CE2/Windows convention, Assumption A2).
            let s = format!(
                "[{INI_SECTION}]\r\n{KEY_INVALIDATE}={VAL_INVALIDATE}\r\n{KEY_RESOURCE_DIRS}={VAL_RESOURCE_DIRS}\r\n"
            );
            return MergePlan::Write(s.into_bytes());
        };
        // Refuse a UTF-16/UTF-32-encoded INI: the byte-oriented merge below would fail to
        // match `[Archive]` / `sResourceDataDirsFinal=` (they are NUL-interleaved) and would
        // append a UTF-8 block, corrupting the file and silently overriding the user's real
        // value. Never edit what we cannot safely parse.
        if is_unsupported_bom(bytes) {
            return MergePlan::Unsupported;
        }
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

// ---------------------------------------------------------------------------
// Journaled op wrappers — reuse backup + journal VERBATIM, changing only WHERE the
// target resolves (the Proton-prefix `My Games/Starfield`, never `Data/`).
// ---------------------------------------------------------------------------

/// Ensure loose-file loading is active for `game`, journaled + provenance-captured.
///
/// Blocks (writes nothing, returns [`IniOutcome::Blocked`]) on a pre-existing non-empty
/// user `sResourceDataDirsFinal` under [`IniConflictResolution::Block`]; overwrites it only
/// under `UseNexTwist` (after backing up the whole original file). Idempotent — a second
/// call converges to [`IniOutcome::AlreadyActive`] with byte-identical output.
pub fn ensure_ini_active(
    store: &Store,
    game: &Game,
    resolution: IniConflictResolution,
) -> Result<IniOutcome, DeployError> {
    let target = resolve_ini_target(game)?; // T-08-01: re-verify drive_c containment.
    refuse_symlink(&target)?; // T-08-02: never write through a symlink we do not own.

    let current: Option<Vec<u8>> = if path_exists(&target) {
        Some(fs::read(&target).map_err(|e| DeployError::io(&target, e))?)
    } else {
        None
    };

    let bytes = match editor::plan_merge(current.as_deref(), resolution) {
        editor::MergePlan::Conflict(current_value) => {
            return Ok(IniOutcome::Blocked { current_value });
        }
        // A UTF-16/UTF-32 INI we refuse to edit: write nothing, take no provenance row, and
        // leave the user's file byte-for-byte intact.
        editor::MergePlan::Unsupported => {
            return Ok(IniOutcome::UnsupportedEncoding);
        }
        editor::MergePlan::Write(bytes) => bytes,
    };

    // Already exactly active → nothing to write (and DO NOT re-capture provenance, which
    // would mis-record our own file as a pre-existing vanilla original).
    if current.as_deref() == Some(bytes.as_slice()) {
        return Ok(IniOutcome::AlreadyActive);
    }

    let sentinel = Path::new(INI_FILENAME);
    // 1. Durable intent BEFORE any write (crash → replayed to provenance).
    let jid = journal::begin_ini(store, game.appid, sentinel)?;
    // 2. Capture provenance: original bytes for a pre-existing file, else an absence marker.
    //    Stamp CreatedByNexTwist ONLY when no provenance exists yet. An existing real-hash
    //    (PreExisting) row is authoritative and must never be downgraded to ABSENCE_MARKER —
    //    e.g. a PreExisting INI deleted on disk then re-activated (repair/deploy) still has
    //    its row; downgrading it would make a later purge DELETE a user file whose original
    //    bytes we still hold, instead of restoring them.
    let pre_existing = backup::backup_vanilla_if_absent(store, game, &target, sentinel)?;
    if !pre_existing && store.vanilla_for(game.appid, sentinel)?.is_none() {
        store.record_vanilla(game.appid, sentinel, ABSENCE_MARKER)?;
    }
    // 3. Atomic write (temp + rename) — never a half-written INI.
    atomic_write(&target, &bytes)?;
    // 4. Flip the intent to done.
    store.mark_done(jid)?;
    Ok(IniOutcome::Activated)
}

/// Restore the INI to its recorded provenance: original bytes for a
/// PreExisting file, or delete + prune NexTwist-created empty dirs for a CreatedByNexTwist
/// file. A safe no-op when NexTwist never activated the INI (never touches a user file).
pub fn restore_ini(store: &Store, game: &Game) -> Result<IniOutcome, DeployError> {
    let target = resolve_ini_target(game)?;
    // Gate: no provenance row ⇒ we never activated ⇒ leave any user file untouched.
    if store
        .vanilla_for(game.appid, Path::new(INI_FILENAME))?
        .is_none()
    {
        return Ok(IniOutcome::NotActive);
    }
    let jid = journal::begin_ini(store, game.appid, Path::new(INI_FILENAME))?;
    restore_ini_at(store, game, &target)?;
    store.mark_done(jid)?;
    Ok(IniOutcome::Restored)
}

/// Read-only preview of what activation would do — writes nothing, touches no
/// store. Reports will-create vs will-edit, the exact two lines, and any conflict.
pub fn preview_ini_activation(game: &Game) -> Result<IniActivationPreview, DeployError> {
    let target = resolve_ini_target(game)?;
    let exists = path_exists(&target);
    let current = if exists {
        Some(fs::read(&target).map_err(|e| DeployError::io(&target, e))?)
    } else {
        None
    };
    Ok(IniActivationPreview {
        will_create: !exists,
        will_edit: exists,
        lines: editor::owned_lines(),
        conflict: editor::detect_conflict(current.as_deref()),
    })
}

/// Detect whether the StarfieldCustom.ini has drifted from its recorded-active state, for
/// `verify`/`repair` participation. The caller gates on Starfield; ALL INI/path
/// logic stays here so `verify.rs` never touches `my_games_path` /
/// `resolve_target` / `guard_within_root`.
///
/// The INI should be active exactly when loose files are deployed (`list_deployed_files`
/// non-empty — the SAME signal `verify` keys off). Given that: a pre-existing non-empty
/// user value is a recorded/blocked state (NOT drift → clear), an absent target is
/// [`IniDrift::Missing`], a present-but-not-exactly-active target is [`IniDrift::Changed`],
/// and an already-active file is clear. When no loose files are deployed there is no drift
/// (a prior purge already restored the INI to provenance).
pub fn ini_drift(store: &Store, game: &Game) -> Result<Option<IniDrift>, DeployError> {
    if store.list_deployed_files(game.appid)?.is_empty() {
        return Ok(None);
    }
    let target = resolve_ini_target(game)?;
    if !path_exists(&target) {
        return Ok(Some(IniDrift::Missing));
    }
    let current = fs::read(&target).map_err(|e| DeployError::io(&target, e))?;
    match editor::plan_merge(Some(&current), IniConflictResolution::Block) {
        // A pre-existing non-empty user value is a recorded/blocked state, not repairable drift.
        editor::MergePlan::Conflict(_) => Ok(None),
        // A UTF-16/UTF-32 INI we refuse to edit is likewise not repairable drift — surface as
        // clear so repair never appends a UTF-8 block into it.
        editor::MergePlan::Unsupported => Ok(None),
        // Planning is a no-op against the current bytes → already exactly active.
        editor::MergePlan::Write(bytes) if bytes == current => Ok(None),
        editor::MergePlan::Write(_) => Ok(Some(IniDrift::Changed)),
    }
}

/// The shared restore body used by BOTH [`restore_ini`] and the `KIND_INI` journal replay.
///
/// Drives the bytes-vs-absence decision ONLY from the recorded `vanilla_backup` row (never
/// inferred from disk): a real hash → copy original bytes back; the [`ABSENCE_MARKER`] →
/// remove our file + prune the dirs we created; no row → safe no-op. Drops the provenance
/// row at the end so a future user file is never mistaken for ours. Idempotent.
pub(crate) fn restore_ini_at(store: &Store, game: &Game, target: &Path) -> Result<(), DeployError> {
    let sentinel = Path::new(INI_FILENAME);
    let Some(hash) = store.vanilla_for(game.appid, sentinel)? else {
        return Ok(()); // Never activated (or already restored) → never touch a user file.
    };
    crate::method::remove_if_present(target).map_err(|e| DeployError::io(target, e))?;
    if hash == ABSENCE_MARKER {
        // CreatedByNexTwist: file removed above; prune the dirs we may have created.
        prune_created_dirs(target);
    } else {
        // PreExisting: copy the exact original bytes back.
        backup::restore_vanilla(store, game, target, sentinel)?;
    }
    // Provenance consumed → reset to "never activated" (closes a future-user-file window).
    store.remove_vanilla(game.appid, sentinel)?;
    Ok(())
}

/// Resolve the INI target via the Phase-6 hardened resolver and RE-VERIFY `drive_c`
/// containment at the write site — never trust a cached path. NEVER touches the
/// `Data/`-root guard (`resolve_target`/`guard_within_root`).
pub(crate) fn resolve_ini_target(game: &Game) -> Result<PathBuf, DeployError> {
    let target = steam::my_games_path(&game.prefix).join(INI_FILENAME);
    verify_contained(&target, &game.prefix)?;
    Ok(target)
}

/// Lexical `<prefix>/drive_c` containment check (canonicalize-free — the resolver already
/// rejects `..`). Extracted so it is unit-testable with a crafted escaping target.
fn verify_contained(target: &Path, prefix: &Path) -> Result<(), DeployError> {
    if target.starts_with(prefix.join("drive_c")) {
        Ok(())
    } else {
        Err(DeployError::PathEscape(target.to_path_buf()))
    }
}

/// Refuse to write through a symlink at the INI target that we do not own,
/// mirroring `backup.rs`'s `symlink_metadata` discipline. We only ever place a regular
/// file (via temp + rename), so any symlink here is foreign.
fn refuse_symlink(target: &Path) -> Result<(), DeployError> {
    match fs::symlink_metadata(target) {
        Ok(meta) if meta.file_type().is_symlink() => Err(DeployError::NotPristine(format!(
            "refusing to write {INI_FILENAME} through a symlink at {}",
            target.display()
        ))),
        _ => Ok(()),
    }
}

/// Write `bytes` to `target` atomically (sibling temp + `rename`) so a crash mid-write
/// never leaves a half-written INI. Creates the parent dir chain if absent
/// (the first-launch CreatedByNexTwist case restore later prunes).
fn atomic_write(target: &Path, bytes: &[u8]) -> Result<(), DeployError> {
    let parent = target
        .parent()
        .ok_or_else(|| DeployError::PathEscape(target.to_path_buf()))?;
    fs::create_dir_all(parent).map_err(|e| DeployError::io(parent, e))?;
    let tmp = parent.join(format!(".{INI_FILENAME}.nxtmp"));
    fs::write(&tmp, bytes).map_err(|e| DeployError::io(&tmp, e))?;
    fs::rename(&tmp, target).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        DeployError::io(target, e)
    })
}

/// Bottom-up prune the `My Games/<game>` dir chain NexTwist may have created for a
/// first-launch prefix, bounded STRICTLY below the resolved Documents dir (never removes
/// Documents or above). `remove_dir` refuses a non-empty (game-populated) dir, so a dir the
/// game created is never removed. Prune failures are benign — never fail a
/// restore because a dir could not be cleaned up.
fn prune_created_dirs(target: &Path) {
    // target = <Documents>/My Games/<game>/StarfieldCustom.ini
    // documents = target.parent(<game>).parent(My Games).parent(Documents)
    let Some(documents) = target
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
    else {
        return;
    };
    let mut dir = target.parent().map(Path::to_path_buf);
    while let Some(d) = dir {
        if d == documents || !d.starts_with(documents) {
            break; // Reached (or above) the Documents boundary — stop.
        }
        if fs::remove_dir(&d).is_err() {
            break; // Non-empty (game-populated) or already gone — leave it intact.
        }
        dir = d.parent().map(Path::to_path_buf);
    }
}

/// `symlink_metadata`-based existence (a dangling symlink still "exists" here), matching
/// `backup.rs`'s discipline.
fn path_exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

#[cfg(test)]
mod editor_tests {
    use super::IniConflictResolution::{Block, UseNexTwist};
    use super::editor::*;

    fn planned(current: Option<&[u8]>, r: super::IniConflictResolution) -> Vec<u8> {
        match plan_merge(current, r) {
            MergePlan::Write(b) => b,
            MergePlan::Conflict(v) => panic!("unexpected conflict: {v}"),
            MergePlan::Unsupported => panic!("unexpected unsupported encoding"),
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
        assert!(
            text.starts_with("[General]\r\nsLang=en\r\n"),
            "prior bytes untouched"
        );
        assert!(
            text.contains("[Archive]\r\nbInvalidateOlderFiles=1\r\nsResourceDataDirsFinal=\r\n")
        );
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
        assert_eq!(
            out, input,
            "no-op re-merge of a CRLF+BOM file must be identical"
        );
        assert_eq!(detect_bom(&out), 3, "BOM preserved");
    }

    #[test]
    fn existing_value_is_updated_in_place_preserving_its_crlf() {
        // A stale bInvalidateOlderFiles=0 is corrected to 1 without touching CRLF/order.
        let input = b"[Archive]\r\nbInvalidateOlderFiles=0\r\nsResourceDataDirsFinal=\r\n";
        let out = planned(Some(input), Block);
        assert_eq!(
            out,
            b"[Archive]\r\nbInvalidateOlderFiles=1\r\nsResourceDataDirsFinal=\r\n"
        );
    }

    #[test]
    fn nonempty_user_value_is_a_conflict_under_block() {
        let input = b"[Archive]\nsResourceDataDirsFinal=Textures\\\n";
        match plan_merge(Some(input), Block) {
            MergePlan::Conflict(v) => assert_eq!(v, "Textures\\"),
            _ => panic!("must block a non-empty user value"),
        }
        // detect_conflict agrees.
        assert_eq!(detect_conflict(Some(input)).as_deref(), Some("Textures\\"));
    }

    #[test]
    fn utf16_and_utf32_bom_are_refused_utf8_is_editable() {
        // A UTF-16 LE INI with a REAL user value (properly NUL-interleaved) must be refused,
        // not parsed as ASCII and appended-to.
        let mut le = vec![0xFF, 0xFE];
        for u in "[Archive]\r\nsResourceDataDirsFinal=Mods\r\n".encode_utf16() {
            le.extend_from_slice(&u.to_le_bytes());
        }
        assert!(matches!(
            plan_merge(Some(&le), Block),
            MergePlan::Unsupported
        ));
        // UTF-16 BE and UTF-32 BE too.
        assert!(matches!(
            plan_merge(Some(&[0xFE, 0xFF, 0x00, b'x']), Block),
            MergePlan::Unsupported
        ));
        assert!(matches!(
            plan_merge(Some(&[0x00, 0x00, 0xFE, 0xFF, 0x00]), Block),
            MergePlan::Unsupported
        ));
        // A UTF-8 BOM file is still editable (NOT refused).
        let mut utf8 = vec![0xEF, 0xBB, 0xBF];
        utf8.extend_from_slice(b"[Archive]\r\nsResourceDataDirsFinal=\r\n");
        assert!(matches!(
            plan_merge(Some(&utf8), Block),
            MergePlan::Write(_)
        ));
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
        assert!(
            text.contains("sResourceDataDirsFinal=\r\n"),
            "value cleared to empty"
        );
        assert!(
            !text.contains("Textures"),
            "user value overwritten under UseNexTwist"
        );
    }

    #[test]
    fn owned_lines_are_the_two_recipe_keys() {
        assert_eq!(
            owned_lines(),
            vec![
                "bInvalidateOlderFiles=1".to_string(),
                "sResourceDataDirsFinal=".to_string()
            ]
        );
    }
}

#[cfg(test)]
mod wrapper_tests {
    use super::*;
    use nextwist_core::Game;
    use std::path::Path;
    use store::Store;
    use tempfile::TempDir;
    use testkit::{MyGamesOpts, fake_my_games_prefix};

    const STARFIELD: u32 = 1716740;

    /// A store + game whose prefix is a fresh fake Proton `My Games/Starfield` tree.
    /// `opts` shapes the seeded INI (marker = a pre-existing StarfieldCustom.ini).
    fn fixture(dir: &TempDir, opts: MyGamesOpts<'_>) -> (Store, Game) {
        let root = dir.path();
        let prefix = fake_my_games_prefix(&root.join("prefix"), "Starfield", opts).unwrap();
        let store = Store::open(&root.join("d.db")).unwrap();
        let game = Game {
            appid: STARFIELD,
            name: "Starfield".into(),
            install_dir: root.join("install"),
            prefix,
            staging_dir: root.join("staging"),
        };
        (store, game)
    }

    fn ini_path(game: &Game) -> PathBuf {
        steam::my_games_path(&game.prefix).join(INI_FILENAME)
    }

    #[test]
    fn preview_absent_reports_will_create_and_writes_nothing() {
        let dir = TempDir::new().unwrap();
        let (_store, game) = fixture(&dir, MyGamesOpts::default());
        let p = preview_ini_activation(&game).unwrap();
        assert!(p.will_create && !p.will_edit);
        assert_eq!(p.lines, editor::owned_lines());
        assert!(p.conflict.is_none());
        assert!(!ini_path(&game).exists(), "preview must not write");
    }

    #[test]
    fn preview_existing_empty_reports_will_edit_no_conflict() {
        let dir = TempDir::new().unwrap();
        let (_store, game) = fixture(
            &dir,
            MyGamesOpts {
                marker: Some(INI_FILENAME),
                ..Default::default()
            },
        );
        std::fs::write(ini_path(&game), b"[Archive]\r\nsResourceDataDirsFinal=\r\n").unwrap();
        let p = preview_ini_activation(&game).unwrap();
        assert!(p.will_edit && !p.will_create);
        assert!(p.conflict.is_none());
    }

    #[test]
    fn preview_conflict_reports_value_no_write() {
        let dir = TempDir::new().unwrap();
        let (_store, game) = fixture(
            &dir,
            MyGamesOpts {
                marker: Some(INI_FILENAME),
                ..Default::default()
            },
        );
        let original = b"[Archive]\r\nsResourceDataDirsFinal=Mods\\\r\n";
        std::fs::write(ini_path(&game), original).unwrap();
        let p = preview_ini_activation(&game).unwrap();
        assert_eq!(p.conflict.as_deref(), Some("Mods\\"));
        assert_eq!(
            std::fs::read(ini_path(&game)).unwrap(),
            original,
            "preview writes nothing"
        );
    }

    #[test]
    fn ensure_creates_new_ini_and_is_idempotent_no_pending() {
        let dir = TempDir::new().unwrap();
        let (store, game) = fixture(&dir, MyGamesOpts::default());
        assert_eq!(
            ensure_ini_active(&store, &game, IniConflictResolution::Block).unwrap(),
            IniOutcome::Activated
        );
        let written = std::fs::read(ini_path(&game)).unwrap();
        assert_eq!(
            written,
            b"[Archive]\r\nbInvalidateOlderFiles=1\r\nsResourceDataDirsFinal=\r\n"
        );
        // Intent-before-act fully resolved.
        assert!(store.pending_ops().unwrap().is_empty());
        // Second call converges without a rewrite.
        assert_eq!(
            ensure_ini_active(&store, &game, IniConflictResolution::Block).unwrap(),
            IniOutcome::AlreadyActive
        );
        assert_eq!(std::fs::read(ini_path(&game)).unwrap(), written);
        assert!(store.pending_ops().unwrap().is_empty());
    }

    #[test]
    fn ensure_blocks_nonempty_user_value_and_use_nextwist_overwrites() {
        let dir = TempDir::new().unwrap();
        let (store, game) = fixture(
            &dir,
            MyGamesOpts {
                marker: Some(INI_FILENAME),
                ..Default::default()
            },
        );
        let original = b"[Archive]\r\nsResourceDataDirsFinal=Mods\\\r\n";
        std::fs::write(ini_path(&game), original).unwrap();
        // Block → surfaced, NOTHING written, Ok (not Err).
        assert_eq!(
            ensure_ini_active(&store, &game, IniConflictResolution::Block).unwrap(),
            IniOutcome::Blocked {
                current_value: "Mods\\".into()
            }
        );
        assert_eq!(std::fs::read(ini_path(&game)).unwrap(), original);
        assert!(store.pending_ops().unwrap().is_empty());
        // UseNexTwist → overwrite (after backing up the whole original).
        assert_eq!(
            ensure_ini_active(&store, &game, IniConflictResolution::UseNexTwist).unwrap(),
            IniOutcome::Activated
        );
        let after = String::from_utf8(std::fs::read(ini_path(&game)).unwrap()).unwrap();
        assert!(after.contains("sResourceDataDirsFinal=\r\n") && !after.contains("Mods"));
    }

    #[test]
    fn restore_preexisting_restores_original_bytes() {
        let dir = TempDir::new().unwrap();
        let (store, game) = fixture(
            &dir,
            MyGamesOpts {
                marker: Some(INI_FILENAME),
                ..Default::default()
            },
        );
        let original =
            b"; user config\r\n[Display]\r\niSize=1080\r\n[Archive]\r\nsResourceDataDirsFinal=\r\n";
        std::fs::write(ini_path(&game), original).unwrap();
        ensure_ini_active(&store, &game, IniConflictResolution::Block).unwrap();
        assert_ne!(
            std::fs::read(ini_path(&game)).unwrap(),
            original,
            "activation edited it"
        );
        assert_eq!(restore_ini(&store, &game).unwrap(), IniOutcome::Restored);
        assert_eq!(
            std::fs::read(ini_path(&game)).unwrap(),
            original,
            "PreExisting INI restored byte-for-byte"
        );
        assert!(store.pending_ops().unwrap().is_empty());
    }

    #[test]
    fn restore_created_deletes_file_and_prunes_created_dir() {
        // A truly-first-launch prefix: no My Games/Starfield dir exists yet.
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let store = Store::open(&root.join("d.db")).unwrap();
        let game = Game {
            appid: STARFIELD,
            name: "Starfield".into(),
            install_dir: root.join("install"),
            prefix: root.join("prefix"),
            staging_dir: root.join("staging"),
        };
        ensure_ini_active(&store, &game, IniConflictResolution::Block).unwrap();
        let ini = ini_path(&game);
        assert!(ini.exists(), "activation created the INI (and its dirs)");
        assert_eq!(restore_ini(&store, &game).unwrap(), IniOutcome::Restored);
        assert!(!ini.exists(), "CreatedByNexTwist INI deleted on restore");
        assert!(
            !ini.parent().unwrap().exists(),
            "NexTwist-created Starfield dir pruned"
        );
        // Documents (the boundary) is never removed.
        let documents = ini.parent().unwrap().parent().unwrap().parent().unwrap();
        assert!(documents.exists(), "Documents boundary dir preserved");
    }

    #[test]
    fn restore_is_a_safe_noop_when_never_activated() {
        // A user's own INI that NexTwist never touched must survive a restore untouched.
        let dir = TempDir::new().unwrap();
        let (store, game) = fixture(
            &dir,
            MyGamesOpts {
                marker: Some(INI_FILENAME),
                ..Default::default()
            },
        );
        let user_bytes = b"[Archive]\r\nsResourceDataDirsFinal=MyMods\\\r\n";
        std::fs::write(ini_path(&game), user_bytes).unwrap();
        assert_eq!(restore_ini(&store, &game).unwrap(), IniOutcome::NotActive);
        assert_eq!(
            std::fs::read(ini_path(&game)).unwrap(),
            user_bytes,
            "a never-activated user INI must NEVER be deleted or altered"
        );
    }

    #[test]
    fn ensure_refuses_to_write_through_a_symlink() {
        let dir = TempDir::new().unwrap();
        let (store, game) = fixture(
            &dir,
            MyGamesOpts {
                marker: Some("x"),
                ..Default::default()
            },
        );
        let ini = ini_path(&game);
        let elsewhere = dir.path().join("outside.ini");
        std::fs::write(&elsewhere, b"x").unwrap();
        std::os::unix::fs::symlink(&elsewhere, &ini).unwrap();
        assert!(
            ensure_ini_active(&store, &game, IniConflictResolution::Block).is_err(),
            "must refuse to write through a foreign symlink (T-08-02)"
        );
        // The symlink target is untouched.
        assert_eq!(std::fs::read(&elsewhere).unwrap(), b"x");
    }

    #[test]
    fn verify_contained_refuses_an_escaping_target() {
        let prefix = Path::new("/tmp/prefix");
        let ok = prefix.join("drive_c/users/steamuser/Documents/x.ini");
        assert!(verify_contained(&ok, prefix).is_ok());
        let escape = Path::new("/etc/passwd");
        assert!(
            matches!(
                verify_contained(escape, prefix),
                Err(DeployError::PathEscape(_))
            ),
            "a target outside <prefix>/drive_c must be refused (T-08-01)"
        );
    }
}
