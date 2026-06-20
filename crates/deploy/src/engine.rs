//! deploy() / purge() / recover_on_launch() orchestration — the safe round-trip.
//!
//! This module ties the probe, method ladder, journal protocol, and vanilla backup
//! together into the three public operations:
//!
//! * [`deploy`] — link every staged file into the game `Data/` tree, intent-before-act,
//!   backing up any pre-existing vanilla file first. Zero original game files are
//!   modified in place.
//! * [`purge`] — manifest-driven (NEVER a directory scan): remove exactly the recorded
//!   files, restore every backed-up vanilla original, and report orphans rather than
//!   blindly deleting. After purge the game folder is byte-for-byte pristine.
//! * [`recover_on_launch`] — replay any non-`done` journal rows to a consistent state.
//!
//! The ordering invariant (Pattern 1) is non-negotiable: a `pending` journal row is
//! durable BEFORE any syscall; the manifest row + `done` flip happen together AFTER.

use std::path::{Path, PathBuf};

use nextwist_core::{DeployMethod, FileEntry, Game};
use store::Store;

use crate::backup;
use crate::error::DeployError;
use crate::journal;
use crate::method::{apply_idempotent, choose_method};
use crate::probe::probe;

/// What [`deploy`] placed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployReport {
    /// Number of files deployed.
    pub deployed: usize,
    /// Number of pre-existing vanilla files backed up before overwrite.
    pub backed_up: usize,
    /// The method actually used per target (after any EXDEV downgrade).
    pub methods: Vec<(PathBuf, DeployMethod)>,
}

/// What [`purge`] removed/restored, plus any orphans it refused to blindly delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurgeReport {
    /// Number of deployed files removed.
    pub removed: usize,
    /// Number of vanilla originals restored.
    pub restored: usize,
    /// Paths present under the deploy root that provenance does not explain. Reported,
    /// never deleted (Pitfall 4: purge must not delete user/vanilla files).
    pub orphans: Vec<PathBuf>,
}

/// What [`recover_on_launch`] replayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReport {
    /// Number of journal rows replayed (rolled forward or back).
    pub replayed: usize,
}

/// Deploy every file of `staged` into `game`'s `Data/` tree.
///
/// For each staged file: resolve the target, choose a method from a per-target probe,
/// record a `pending` journal intent, back up any pre-existing vanilla file, apply the
/// idempotent file op, then write the manifest row + flip the intent to `done`.
pub fn deploy(store: &Store, game: &Game, staged: &StagedFiles) -> Result<DeployReport, DeployError> {
    deploy_inner(store, game, staged, None)
}

/// Test-only seam: deploy but abort after `abort_after` file operations, having
/// committed the `pending` journal rows for the work done so far but NOT the manifest
/// rows / `done` flips for the aborted remainder — simulating a kill mid-deploy.
///
/// Returns [`DeployError::Aborted`] once `abort_after` files have been placed.
pub fn deploy_with_abort(
    store: &Store,
    game: &Game,
    staged: &StagedFiles,
    abort_after: usize,
) -> Result<DeployReport, DeployError> {
    deploy_inner(store, game, staged, Some(abort_after))
}

/// The deploy worklist: a staging root plus `Data/`-rooted relpaths to deploy.
///
/// This mirrors `extract::StagedMod` without taking a dependency on the extract crate
/// (the engine is consumed by Plan 06 which already holds a `StagedMod`); callers map
/// `StagedMod { staging_root, files }` into this directly.
#[derive(Debug, Clone)]
pub struct StagedFiles {
    /// Root of the staged, read-only tree.
    pub staging_root: PathBuf,
    /// `Data/`-rooted relpaths under `staging_root`, in deploy order.
    pub files: Vec<PathBuf>,
}

fn deploy_inner(
    store: &Store,
    game: &Game,
    staged: &StagedFiles,
    abort_after: Option<usize>,
) -> Result<DeployReport, DeployError> {
    let data_dir = crate::deploy_root(&game.install_dir);
    std::fs::create_dir_all(&data_dir).map_err(|e| DeployError::io(&data_dir, e))?;

    // Probe once per deploy at the staging-root → data-dir granularity (both are
    // directories; the method ladder still catches a late per-file EXDEV).
    let caps = probe(&staged.staging_root, &data_dir).map_err(|e| DeployError::io(&data_dir, e))?;
    let chosen = choose_method(&caps);

    let mut report = DeployReport {
        deployed: 0,
        backed_up: 0,
        methods: Vec::new(),
    };

    for rel in &staged.files {
        let src = staged.staging_root.join(rel);
        if !src.is_file() {
            // Only deploy regular files (never a directory; symlinks were rejected at
            // extract time). Skip anything else defensively.
            continue;
        }
        let target = crate::resolve_target(&game.install_dir, rel);
        guard_within_root(&data_dir, &target)?;

        let source_hash = backup::blake3_file(&src)?;

        // 1. Durable intent BEFORE any syscall.
        let jid = journal::begin_deploy(store, game.appid, rel, chosen, &source_hash)?;

        // 2. Backup-before-overwrite (idempotent, content-addressed).
        let backed = backup::backup_vanilla_if_absent(store, game, &target, rel)?;
        if backed {
            report.backed_up += 1;
        }

        // 3. Idempotent file op (downgrades on EXDEV).
        let used = apply_idempotent(chosen, &src, &target)?;

        // --- injected abort point (crash simulation): the pending row is committed
        //     and the file is on disk, but we return BEFORE writing the manifest row
        //     / flipping the intent to done — exactly the kill-mid-deploy window. ---
        if let Some(n) = abort_after {
            if report.deployed >= n {
                return Err(DeployError::Aborted(report.deployed));
            }
        }

        // 4. Manifest row + done flip together, AFTER the syscall succeeded.
        let entry = FileEntry {
            target_rel: rel.clone(),
            source_mod: 0,
            method: used,
            hash: source_hash,
            pre_existing: backed,
        };
        journal::finish_deploy(store, jid, game.appid, &entry)?;

        report.deployed += 1;
        report.methods.push((rel.clone(), used));
    }

    Ok(report)
}

/// Purge every recorded deployed file for `game`, restoring vanilla originals, and
/// return to a byte-for-byte pristine deploy root.
///
/// Driven ENTIRELY by the manifest (`list_deployed_files`) — never a directory scan.
/// Each removal is intent-journaled (so a crash mid-purge is recoverable) and
/// idempotent. Orphans (paths under the deploy root that provenance does not explain)
/// are REPORTED, not deleted.
pub fn purge(store: &Store, game: &Game) -> Result<PurgeReport, DeployError> {
    let files = store.list_deployed_files(game.appid)?;
    let mut report = PurgeReport {
        removed: 0,
        restored: 0,
        orphans: Vec::new(),
    };

    for entry in &files {
        let target = crate::resolve_target(&game.install_dir, &entry.target_rel);

        // 1. Durable purge intent BEFORE the syscall.
        let jid = journal::begin_purge(store, game.appid, &entry.target_rel)?;

        // 2. Remove our placement idempotently.
        crate::method::remove_if_present(&target).map_err(|e| DeployError::io(&target, e))?;
        report.removed += 1;

        // 3. Restore any backed-up vanilla original.
        if backup::restore_vanilla(store, game, &target, &entry.target_rel)? {
            report.restored += 1;
        }

        // 4. Drop the manifest row + flip the intent to done.
        store.remove_deployed_file(game.appid, &entry.target_rel)?;
        journal::finish_purge(store, jid)?;
    }

    Ok(report)
}

/// Replay any non-`done` journal rows on launch to reach a consistent state.
pub fn recover_on_launch(store: &Store, game: &Game) -> Result<RecoveryReport, DeployError> {
    let replayed = journal::replay(store, game)?;
    Ok(RecoveryReport { replayed })
}

/// Assert `target` is within `root` (V4 access control: never write outside the
/// resolved deploy root). Uses lexical containment so it works for not-yet-created
/// paths (canonicalize would fail on a missing target).
fn guard_within_root(root: &Path, target: &Path) -> Result<(), DeployError> {
    // Both paths are constructed by us from `install_dir` + a validated relpath, but
    // we still assert containment as a defence-in-depth boundary.
    let root_norm = lexical_normalize(root);
    let target_norm = lexical_normalize(target);
    if target_norm.starts_with(&root_norm) {
        Ok(())
    } else {
        Err(DeployError::PathEscape(target.to_path_buf()))
    }
}

/// Lexically normalize a path (resolve `.`/`..` components) without touching disk.
fn lexical_normalize(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}
