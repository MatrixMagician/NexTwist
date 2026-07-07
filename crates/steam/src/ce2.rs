//! Creation Engine 2 (Starfield) config-path resolution + version-drift compare.
//!
//! Starfield splits its user data across TWO prefix locations: `plugins.txt` lives in
//! `AppData/Local/Starfield/` (that arm is Phase 7's job), while `StarfieldCustom.ini`
//! and the first-launch marker live in `Documents/My Games/Starfield/`. THIS module
//! resolves the latter — the `My Games` path — mirroring libloadorder's own derivation
//! but computing it ourselves (libloot keeps its copy private).
//!
//! Everything here is READ-ONLY: Phase 6 writes nothing to a real prefix. The resolver
//! is case-folded (Wine prefixes are case-sensitive on Linux; the on-disk casing may not
//! match the canonical Bethesda casing) and defensively redirection-aware (a `user.reg`
//! `"Personal"` shell-folder repoint is honored, with the default steamuser/Documents
//! path as the load-bearing fallback). First launch is a typed SUCCESS state, never an
//! error — the game must stay addable even before its config dir exists (SFDET-02).

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::resolve::entry_ci;

/// The `My Games` subfolder name for Starfield (also the Steam AppData folder name).
const STARFIELD_FOLDER: &str = "Starfield";

/// The typed first-launch state of the CE2 `Documents/My Games/Starfield` config dir.
///
/// A SUCCESS enum, NOT a `thiserror` arm: an unresolved/first-launch config dir must not
/// abort the add-game flow (SFDET-02). Diverges deliberately from
/// `loadorder::LoadOrderError::NoLocalAppData` (which is an `Err`) — both carry the path,
/// but this one keeps the game addable while load-order / INI ops stay blocked upstream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Ce2ConfigState {
    /// The config dir exists and is non-empty — the real on-disk (case-folded) path.
    Ready(PathBuf),
    /// The config dir is absent or empty (game not launched yet) — the canonical
    /// expected path, for UI first-launch guidance.
    FirstLaunchPending(PathBuf),
}

/// The build NexTwist's Starfield support is validated against.
///
/// `0` = baseline UNSET → the drift notice is SUPPRESSED (`drift_notice` always returns
/// `None`) until Phase 9 seeds the real installed build. Keeping the compare here (in
/// `steam`, below `loadorder`) co-locates the constant with the compare that uses it.
pub const VALIDATED_BUILD: u64 = 0;

/// An advisory, non-blocking version-drift signal (SFDET-03).
///
/// Produced only when the installed build is strictly newer than a non-zero validated
/// build. It never gates management — reversibility is build-independent (Phase 6 writes
/// nothing).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DriftNotice {
    /// The build read from `appmanifest_<appid>.acf`.
    pub installed: u64,
    /// The build NexTwist validated against ([`VALIDATED_BUILD`]).
    pub validated: u64,
    /// Whether `installed > validated` (always `true` when a notice is present).
    pub is_newer: bool,
}

/// The aggregate Starfield detection status the Tauri adapter (Plan 03) forwards verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StarfieldStatus {
    /// CE2 `My Games` config-dir state (ready / first-launch-pending).
    pub ce2_state: Ce2ConfigState,
    /// Installed Steam build, or `None` if unreadable (fail-safe).
    pub installed_build: Option<u64>,
    /// The build NexTwist validated against.
    pub validated_build: u64,
    /// Advisory drift notice, or `None` when suppressed / no drift.
    pub drift: Option<DriftNotice>,
}

/// Resolve the `Documents/My Games/Starfield` path inside a Proton `prefix`.
///
/// Redirection-aware (`user.reg` `"Personal"`, default fallback) and case-folded: each
/// EXISTING path component is matched through [`entry_ci`] to recover the real on-disk
/// casing; missing components are appended in canonical casing (yielding the expected
/// first-launch path). Never follows a symlink out of the prefix (`entry_ci` reads dir
/// entries only) and never escapes `<prefix>/drive_c`.
pub fn my_games_path(prefix: &Path) -> PathBuf {
    let mut components = documents_components(prefix);
    components.push("My Games".to_string());
    components.push(STARFIELD_FOLDER.to_string());
    let resolved = resolve_cased(prefix, &components);
    // Load-bearing last-line invariant (T-06-01): whatever the redirect
    // produced, the final My Games path MUST stay under <prefix>/drive_c. This
    // is a lexical, canonicalize-free containment check — safe because the
    // component guard already rejects `..`, so no `..` can appear in the
    // resolved path to defeat `starts_with`. Any escape falls back to the
    // default steamuser Documents tree.
    if resolved.starts_with(prefix.join("drive_c")) {
        resolved
    } else {
        let mut fallback = default_documents_components();
        fallback.push("My Games".to_string());
        fallback.push(STARFIELD_FOLDER.to_string());
        resolve_cased(prefix, &fallback)
    }
}

/// The path components (relative to `prefix`) of the Documents folder: either the
/// `user.reg` `"Personal"` redirect (validated in-prefix) or the default steamuser tree.
fn documents_components(prefix: &Path) -> Vec<String> {
    read_personal_redirect(prefix).unwrap_or_else(default_documents_components)
}

/// The load-bearing default: `<prefix>/drive_c/users/steamuser/Documents`.
fn default_documents_components() -> Vec<String> {
    ["drive_c", "users", "steamuser", "Documents"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Read `<prefix>/user.reg`, extract the `"Personal"` shell-folder value, and map it to
/// in-prefix components. `None` (→ default fallback) on any absence, parse failure, or a
/// value that would escape `<prefix>/drive_c` (T-06-01 traversal guard).
fn read_personal_redirect(prefix: &Path) -> Option<Vec<String>> {
    let raw = std::fs::read_to_string(prefix.join("user.reg")).ok()?;
    let value = extract_personal(&raw)?;
    windows_path_to_components(&value)
}

/// Pull the `"Personal"="..."` value from the `User Shell Folders` section of a Wine
/// `user.reg`. Lenient line scan; un-escapes Wine's doubled backslashes.
fn extract_personal(reg: &str) -> Option<String> {
    let mut in_section = false;
    for line in reg.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t.to_ascii_lowercase().contains("user shell folders");
            continue;
        }
        if in_section && let Some(rest) = t.strip_prefix("\"Personal\"") {
            let rest = rest.trim_start().strip_prefix('=')?.trim_start();
            // Optional Wine type tag before the quote: `str(2):` (REG_EXPAND_SZ,
            // Wine's default for shell-folder redirects) or `str:`.
            let rest = rest
                .strip_prefix("str(2):")
                .or_else(|| rest.strip_prefix("str:"))
                .unwrap_or(rest)
                .trim_start();
            let inner = rest.strip_prefix('"')?;
            let end = inner.find('"')?;
            let value = inner[..end].replace("\\\\", "\\");
            // Expand a leading %USERPROFILE% to the steamuser home so a genuine
            // REG_EXPAND_SZ redirect resolves (still passes through the
            // windows_path_to_components traversal guard downstream).
            return Some(value.replace("%USERPROFILE%", "C:\\users\\steamuser"));
        }
    }
    None
}

/// Map a Windows `C:\users\...` path to in-prefix components rooted at `drive_c`.
/// Rejects a non-`C:` drive and any segment that is not a single plain path
/// component — `.`/`..`/empty, an embedded `/`, or an absolute `/…` segment —
/// all of which could let `resolve_cased`'s `Path::join` escape drive_c
/// (T-06-01 traversal guard). `my_games_path` re-checks lexical containment.
fn windows_path_to_components(value: &str) -> Option<Vec<String>> {
    let mut parts = value.split('\\');
    if !parts.next()?.eq_ignore_ascii_case("C:") {
        return None;
    }
    let mut out = vec!["drive_c".to_string()];
    for p in parts {
        // Reject empty / dot-dirs AND any segment that is not a single plain
        // path component. A '/' inside a segment, a leading '/' (absolute), or
        // any non-Normal component would let `Path::join` in resolve_cased
        // escape drive_c / the prefix root (verified: `C:\/etc` → `/etc`,
        // `C:\foo/../../etc\bar` → out of drive_c). Backslash `..` is caught
        // here; the `/`-embedded and absolute forms are caught by the '/' and
        // component-count checks.
        if p.is_empty()
            || p == "."
            || p == ".."
            || p.contains('/')
            || Path::new(p).components().count() != 1
        {
            return None;
        }
        out.push(p.to_string());
    }
    // A bare `C:` (no folder) is not a usable Documents redirect.
    (out.len() > 1).then_some(out)
}

/// Walk `components` under `prefix`, matching each EXISTING component through [`entry_ci`]
/// to recover its real on-disk casing; once a component is missing, the rest are appended
/// in canonical casing (the expected first-launch path).
fn resolve_cased(prefix: &Path, components: &[String]) -> PathBuf {
    let mut current = prefix.to_path_buf();
    let mut still_exists = true;
    for comp in components {
        current = if still_exists {
            match entry_ci(&current, comp) {
                Some(real) => real,
                None => {
                    still_exists = false;
                    current.join(comp)
                }
            }
        } else {
            current.join(comp)
        };
    }
    current
}

/// Classify the CE2 config dir: [`Ce2ConfigState::Ready`] if it exists and is non-empty,
/// else [`Ce2ConfigState::FirstLaunchPending`] with the expected canonical path.
pub fn resolve_ce2_config(prefix: &Path) -> Ce2ConfigState {
    let path = my_games_path(prefix);
    let non_empty = std::fs::read_dir(&path)
        .map(|mut rd| rd.next().is_some())
        .unwrap_or(false);
    if non_empty {
        Ce2ConfigState::Ready(path)
    } else {
        Ce2ConfigState::FirstLaunchPending(path)
    }
}

/// Advisory drift compare (SFDET-03). `Some` only when `installed > validated > 0`.
pub fn drift_notice(installed: Option<u64>, validated: u64) -> Option<DriftNotice> {
    let installed = installed?;
    (validated > 0 && installed > validated).then_some(DriftNotice {
        installed,
        validated,
        is_newer: true,
    })
}

/// Aggregate CE2 state + installed build + drift into the single typed status value.
pub fn starfield_status(prefix: &Path, library_root: &Path, appid: u32) -> StarfieldStatus {
    let installed_build = crate::resolve::installed_build(library_root, appid);
    StarfieldStatus {
        ce2_state: resolve_ce2_config(prefix),
        installed_build,
        validated_build: VALIDATED_BUILD,
        drift: drift_notice(installed_build, VALIDATED_BUILD),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testkit::{MyGamesOpts, fake_my_games_prefix};

    const STARFIELD_APPID: u32 = crate::resolve::STARFIELD;

    #[test]
    fn my_games_path_default_prefix_resolves_canonical() {
        let dir = tempfile::TempDir::new().unwrap();
        let root =
            fake_my_games_prefix(dir.path(), STARFIELD_FOLDER, MyGamesOpts::default()).unwrap();
        let resolved = my_games_path(&root);
        assert_eq!(
            resolved,
            root.join("drive_c/users/steamuser/Documents/My Games/Starfield")
        );
    }

    #[test]
    fn my_games_case_mismatch_resolves_real_on_disk_casing() {
        // MANDATORY (SFDET-02 crit 2): prefix built mis-cased must resolve to the real path.
        let dir = tempfile::TempDir::new().unwrap();
        let root = fake_my_games_prefix(
            dir.path(),
            STARFIELD_FOLDER,
            MyGamesOpts { case_variant: true, ..Default::default() },
        )
        .unwrap();
        let resolved = my_games_path(&root);
        assert_eq!(
            resolved,
            root.join("drive_c/users/steamuser/documents/my games/starfield"),
            "must return the mis-cased on-disk path, not canonical casing"
        );
        assert!(resolved.is_dir());
    }

    #[test]
    fn documents_redirect_is_honored_when_in_prefix() {
        // A user.reg repointing Personal to another in-prefix dir is followed.
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        // Seed the redirected Documents tree directly.
        std::fs::create_dir_all(
            root.join("drive_c/users/steamuser/Redirected/My Games/Starfield"),
        )
        .unwrap();
        std::fs::write(
            root.join("user.reg"),
            "[Software\\\\Microsoft\\\\Windows\\\\CurrentVersion\\\\Explorer\\\\User Shell Folders]\n\"Personal\"=\"C:\\\\users\\\\steamuser\\\\Redirected\"\n",
        )
        .unwrap();
        let resolved = my_games_path(&root);
        assert_eq!(
            resolved,
            root.join("drive_c/users/steamuser/Redirected/My Games/Starfield")
        );
    }

    #[test]
    fn documents_redirect_traversal_is_rejected() {
        // A Personal value escaping the prefix (`..`) must fall back to the default.
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::write(
            root.join("user.reg"),
            "[Software\\\\Microsoft\\\\Windows\\\\CurrentVersion\\\\Explorer\\\\User Shell Folders]\n\"Personal\"=\"C:\\\\users\\\\..\\\\..\\\\etc\"\n",
        )
        .unwrap();
        let resolved = my_games_path(&root);
        assert_eq!(
            resolved,
            root.join("drive_c/users/steamuser/Documents/My Games/Starfield"),
            "traversal Personal must fall back to the default path"
        );
    }

    #[test]
    fn documents_redirect_forward_slash_and_absolute_are_rejected() {
        // HI-01 regression (LO-02): the `/`-embedded, absolute-`/`, and
        // mixed-separator vectors that the old backslash-only guard let escape
        // must ALL fall back to the default and stay under drive_c. These MUST
        // fail against the pre-fix guard.
        for value in [
            "C:\\users/../../../etc", // forward-slash traversal inside one backslash segment
            "C:\\/etc",               // absolute unix segment (would become /etc)
            "C:\\foo/../../etc\\bar",  // mixed-separator escape out of drive_c
        ] {
            let dir = tempfile::TempDir::new().unwrap();
            let root = dir.path().to_path_buf();
            std::fs::write(
                root.join("user.reg"),
                format!(
                    "[Software\\\\Microsoft\\\\Windows\\\\CurrentVersion\\\\Explorer\\\\User Shell Folders]\n\"Personal\"=\"{}\"\n",
                    value.replace('\\', "\\\\")
                ),
            )
            .unwrap();
            let resolved = my_games_path(&root);
            assert_eq!(
                resolved,
                root.join("drive_c/users/steamuser/Documents/My Games/Starfield"),
                "traversal vector {value:?} must fall back to the default path"
            );
            assert!(
                resolved.starts_with(root.join("drive_c")),
                "resolved path for {value:?} must stay under <prefix>/drive_c"
            );
        }
    }

    #[test]
    fn documents_redirect_str2_userprofile_form_is_expanded() {
        // ME-01: Wine's REG_EXPAND_SZ form `str(2):"%USERPROFILE%\Documents"`
        // is the default for a genuine Documents redirect and must be honored.
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(
            root.join("drive_c/users/steamuser/Documents/My Games/Starfield"),
        )
        .unwrap();
        std::fs::write(
            root.join("user.reg"),
            "[Software\\\\Microsoft\\\\Windows\\\\CurrentVersion\\\\Explorer\\\\User Shell Folders]\n\"Personal\"=str(2):\"%USERPROFILE%\\\\Documents\"\n",
        )
        .unwrap();
        let resolved = my_games_path(&root);
        assert_eq!(
            resolved,
            root.join("drive_c/users/steamuser/Documents/My Games/Starfield"),
            "str(2)/%USERPROFILE% redirect must resolve to the expanded in-prefix path"
        );
    }

    #[test]
    fn malformed_user_reg_falls_back_to_default() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::write(root.join("user.reg"), "this is not a valid reg file\n").unwrap();
        let resolved = my_games_path(&root);
        assert_eq!(
            resolved,
            root.join("drive_c/users/steamuser/Documents/My Games/Starfield")
        );
    }

    #[test]
    fn first_launch_pending_when_absent_or_empty() {
        // Absent dir → FirstLaunchPending (expected canonical path).
        let empty = tempfile::TempDir::new().unwrap();
        match resolve_ce2_config(empty.path()) {
            Ce2ConfigState::FirstLaunchPending(p) => assert_eq!(
                p,
                empty
                    .path()
                    .join("drive_c/users/steamuser/Documents/My Games/Starfield")
            ),
            other => panic!("expected FirstLaunchPending, got {other:?}"),
        }

        // Present but empty dir → still FirstLaunchPending.
        let dir = tempfile::TempDir::new().unwrap();
        let root =
            fake_my_games_prefix(dir.path(), STARFIELD_FOLDER, MyGamesOpts::default()).unwrap();
        assert!(matches!(
            resolve_ce2_config(&root),
            Ce2ConfigState::FirstLaunchPending(_)
        ));
    }

    #[test]
    fn ready_when_config_dir_has_a_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = fake_my_games_prefix(
            dir.path(),
            STARFIELD_FOLDER,
            MyGamesOpts { marker: Some("StarfieldCustom.ini"), ..Default::default() },
        )
        .unwrap();
        match resolve_ce2_config(&root) {
            Ce2ConfigState::Ready(p) => {
                assert_eq!(p, root.join("drive_c/users/steamuser/Documents/My Games/Starfield"))
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn drift_notice_only_fires_when_installed_newer_than_nonzero_validated() {
        // installed > validated > 0 → Some{is_newer:true}
        let n = drift_notice(Some(200), 100).unwrap();
        assert!(n.is_newer);
        assert_eq!((n.installed, n.validated), (200, 100));
        // equal → None
        assert!(drift_notice(Some(100), 100).is_none());
        // older → None
        assert!(drift_notice(Some(50), 100).is_none());
        // installed None → None
        assert!(drift_notice(None, 100).is_none());
        // validated == 0 (baseline unset) → None, even with a huge installed build
        assert!(drift_notice(Some(999_999), VALIDATED_BUILD).is_none());
    }

    #[test]
    fn starfield_status_aggregates_and_is_serializable() {
        let dir = tempfile::TempDir::new().unwrap();
        let root =
            fake_my_games_prefix(dir.path(), STARFIELD_FOLDER, MyGamesOpts::default()).unwrap();
        let status = starfield_status(&root, root.as_path(), STARFIELD_APPID);
        assert_eq!(status.validated_build, VALIDATED_BUILD);
        assert!(matches!(status.ce2_state, Ce2ConfigState::FirstLaunchPending(_)));
        // Drift is dormant in Phase 6 (VALIDATED_BUILD == 0).
        assert!(status.drift.is_none());
        // The whole aggregate serializes (the Tauri adapter forwards it).
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("FirstLaunchPending"));
    }
}
