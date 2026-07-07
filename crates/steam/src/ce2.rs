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
    unimplemented!()
}

/// Classify the CE2 config dir: [`Ce2ConfigState::Ready`] if it exists and is non-empty,
/// else [`Ce2ConfigState::FirstLaunchPending`] with the expected canonical path.
pub fn resolve_ce2_config(prefix: &Path) -> Ce2ConfigState {
    unimplemented!()
}

/// Advisory drift compare (SFDET-03). `Some` only when `installed > validated > 0`.
pub fn drift_notice(installed: Option<u64>, validated: u64) -> Option<DriftNotice> {
    unimplemented!()
}

/// Aggregate CE2 state + installed build + drift into the single typed status value.
pub fn starfield_status(prefix: &Path, library_root: &Path, appid: u32) -> StarfieldStatus {
    unimplemented!()
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
