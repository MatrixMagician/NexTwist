//! `nextwist-deploy` — the reversible deployment engine (CROWN JEWEL).
//!
//! This crate is the differentiating, irreplaceable value of NexTwist: it deploys a
//! staged mod into a game's `Data/` tree **without ever modifying an original game
//! file in place**, records every deployed file in a per-game manifest, backs up any
//! pre-existing vanilla file before overwriting it, and purges back to a
//! **byte-for-byte pristine** game folder — surviving a crash mid-deploy.
//!
//! ## Why this can't be SQLite-WAL alone
//!
//! A `link()`/`reflink()`/`copy()` syscall and the DB row recording it are two
//! operations that cannot be made atomic together. So crash-safety here is an
//! explicit **operation journal** (intent recorded `pending` *before* the syscall,
//! flipped to `done` *after*) combined with **idempotent file ops** — replaying a
//! half-finished op after a crash is always safe. See [`journal`].
//!
//! ## Module map
//!
//! * [`probe`]  — per-target fs-capability probe (st_dev, reflink, throwaway
//!   hardlink, casefold) — `FsCaps`.
//! * [`method`] — the `DeploymentMethod` trait + reflink → hardlink → symlink → copy
//!   ladder, chosen per-target with EXDEV/`CrossesDevices` fallback.
//! * [`journal`] — intent-before-act protocol + idempotent replay/recovery on launch.
//! * [`backup`] — backup-before-overwrite into a content-addressed vanilla store.
//! * [`engine`] — `deploy()` / `purge()` / `recover_on_launch()` orchestration.

pub mod backup;
pub mod casefold;
pub mod conflict;
pub mod engine;
pub mod journal;
pub mod method;
pub mod probe;
pub mod profile;
pub mod verify;

mod error;
mod path_guard;

pub use casefold::normalize_to_canonical;
pub use conflict::{resolve, ModInput, WinnerFile};
pub use error::DeployError;
pub use profile::{switch_profile, SwitchReport};
pub use method::{apply_idempotent, choose_method, DeploymentMethod};
pub use probe::{probe, Casefold, FsCaps};
pub use verify::{repair, verify, RepairReport, VerifyReport};

// Engine orchestration (deploy/purge/recover/deploy_winners) plus the deploy-path fs warnings.
pub use engine::*;

use std::path::{Path, PathBuf};

/// The deploy root for a game: mods are deployed under `<install_dir>/Data`.
///
/// Bethesda mods are `Data/`-rooted, and the staged tree (from `extract`) is also
/// `Data/`-rooted, so the target for a staged relpath `Data/foo/bar.esp` is
/// `<install_dir>/Data/foo/bar.esp`. We resolve the real on-disk `Data` directory
/// case-insensitively (Wine/Proton case-sensitivity), defaulting to `Data`.
pub fn deploy_root(install_dir: &Path) -> PathBuf {
    // Find an existing top-level entry matching "data" case-insensitively, else "Data".
    if let Ok(rd) = std::fs::read_dir(install_dir) {
        for entry in rd.flatten() {
            if entry.file_name().to_string_lossy().eq_ignore_ascii_case("data") {
                return entry.path();
            }
        }
    }
    install_dir.join("Data")
}

/// Resolve the absolute on-disk target for a staged relpath.
///
/// The staged relpath is `Data/`-rooted (e.g. `Data/textures/x.dds`). We strip the
/// leading `Data` segment and re-root under the resolved [`deploy_root`] so casing of
/// the top-level `Data` directory is honored. A relpath without a leading `Data`
/// segment is treated as already deploy-root-relative.
#[allow(dead_code)] // wired into the engine in Task 2
pub(crate) fn resolve_target(install_dir: &Path, staged_rel: &Path) -> PathBuf {
    let root = deploy_root(install_dir);
    let comps = staged_rel.components();
    if let Some(std::path::Component::Normal(first)) = comps.clone().next()
        && first.to_string_lossy().eq_ignore_ascii_case("data")
    {
        // Drop the leading Data/ segment; the rest is relative to the deploy root.
        let rest: PathBuf = comps.skip(1).collect();
        return root.join(rest);
    }
    root.join(staged_rel)
}
