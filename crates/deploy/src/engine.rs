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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use nextwist_core::{DeployMethod, FileEntry, Game};
use serde::{Deserialize, Serialize};
use store::Store;

use steam::canonical_data_casing;

use crate::backup;
use crate::casefold::normalize_to_canonical;
use crate::error::DeployError;
use crate::journal;
use crate::method::{apply_idempotent, choose_method};
use crate::probe::{probe, Casefold, FsCaps};

/// An unsafe-filesystem warning surfaced through [`DeployReport`] so the UI (Plan 06)
/// can warn the user before/at deploy (ENV-04 "warn about unsafe configurations").
///
/// These are WARNINGS, not gates: deploy still proceeds (the method ladder safely
/// downgrades cross-device links, and casing normalization always runs), but the user
/// is informed their filesystem configuration is sub-optimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FsWarning {
    /// The staging and game-data dirs are on different devices (cross-device / EXDEV):
    /// hardlink/reflink are impossible, so deploy falls back to symlink/copy. Same-FS
    /// staging is recommended.
    CrossDevice,
    /// The game-data filesystem does not have the case-folding flag set (or it could
    /// not be determined). Mixed-case mod paths rely on path-casing normalization
    /// instead; surfaced so the user knows case-sensitivity is being handled by us.
    NotCasefolded,
}

/// Derive the [`FsWarning`]s implied by a probe result (ENV-04 warning half).
///
/// `CrossDevice` when staging and game data are not on the same device; `NotCasefolded`
/// when the casefold flag is `Off` or `Unknown` (best-effort — A6: absence of a
/// confirmed `On` means we cannot rely on the kernel for case-insensitivity, so we warn
/// and lean on normalization).
pub fn fs_warnings_from_caps(caps: &FsCaps) -> Vec<FsWarning> {
    let mut warnings = Vec::new();
    if !caps.same_device {
        warnings.push(FsWarning::CrossDevice);
    }
    if !matches!(caps.casefold, Casefold::On) {
        warnings.push(FsWarning::NotCasefolded);
    }
    warnings
}

/// What [`deploy`] placed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployReport {
    /// Number of files deployed.
    pub deployed: usize,
    /// Number of pre-existing vanilla files backed up before overwrite.
    pub backed_up: usize,
    /// The method actually used per target (after any EXDEV downgrade).
    pub methods: Vec<(PathBuf, DeployMethod)>,
    /// Unsafe-filesystem warnings (cross-device / non-casefolded) surfaced for the UI
    /// to show the user before relying on this deployment (ENV-04 warning half).
    pub fs_warnings: Vec<FsWarning>,
}

/// What [`purge`] removed/restored, plus any orphans it refused to blindly delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeReport {
    /// Number of deployed files removed.
    pub removed: usize,
    /// Number of vanilla originals restored.
    pub restored: usize,
    /// Paths present under the deploy root that provenance does not explain. Reported,
    /// never deleted (Pitfall 4: purge must not delete user/vanilla files).
    pub orphans: Vec<PathBuf>,
}

/// What [`recover_on_launch`] replayed, plus the post-replay drift report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryReport {
    /// Number of journal rows replayed (rolled forward or back).
    pub replayed: usize,
    /// Drift report from the automatic verify run after journal replay. An abnormal
    /// exit thus always yields a drift status (DEPLOY-07: full-pristine-or-report).
    pub drift: crate::verify::VerifyReport,
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

    let mut report = DeployReport {
        deployed: 0,
        backed_up: 0,
        methods: Vec::new(),
        fs_warnings: Vec::new(),
    };

    // An empty mod is a valid no-op deploy. Returning early also avoids probing a
    // staging root that a no-file extract may not have materialized.
    if staged.files.is_empty() {
        return Ok(report);
    }

    // Probe once per deploy at the staging-root → data-dir granularity (both are
    // directories; the method ladder still catches a late per-file EXDEV).
    let caps = probe(&staged.staging_root, &data_dir)
        .map_err(|e| DeployError::io(&staged.staging_root, e))?;
    let chosen = choose_method(&caps);

    // ENV-04 (warning half): surface unsafe-fs warnings (cross-device / non-casefolded)
    // for the UI before the user relies on this deployment.
    report.fs_warnings = fs_warnings_from_caps(&caps);

    // DEPLOY-08: the per-game canonical Data/ casing map. Mixed-case mod directory
    // components are rewritten to the game's REAL on-disk casing so Wine's
    // case-sensitive open() resolves them. The map is produced by the steam crate (Plan
    // 02); deploy only consumes it. It is best-effort: a game whose Data/ tree cannot be
    // walked yields an empty map and normalization is a no-op (no worse than today).
    let casing = canonical_data_casing(&game.install_dir).unwrap_or_default();

    for rel in &staged.files {
        let src = staged.staging_root.join(rel);
        if !src.is_file() {
            // Only deploy regular files (never a directory; symlinks were rejected at
            // extract time). Skip anything else defensively.
            continue;
        }
        // Normalize the mod's path casing to the game's canonical Data/ casing BEFORE
        // resolving the on-disk target, so the deployed path matches what Wine opens
        // (DEPLOY-08). The manifest then records the normalized relpath, preserving the
        // round-trip-pristine guarantee (purge keys off the same normalized path).
        let rel = normalize_to_canonical(rel, &casing);
        let rel = &rel;
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
        if let Some(n) = abort_after
            && report.deployed >= n
        {
            return Err(DeployError::Aborted(report.deployed));
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

    // Once every recorded file is gone, remove the now-empty directories that deploy()
    // created. The candidate set is derived ONLY from the manifest rows we just removed
    // (never a blind scan of the vanilla tree), so a vanilla directory is never a
    // candidate; bottom-up `remove_dir` additionally refuses any dir still holding files
    // (a vanilla dir, or a dir an unmanaged orphan lives in), so the game Data/ tree's
    // pre-existing shape is never disturbed (GAP-01 / T-01-19).
    let removed_rels: Vec<PathBuf> = files.iter().map(|e| e.target_rel.clone()).collect();
    remove_emptied_dirs(&game.install_dir, &removed_rels)?;

    Ok(report)
}

/// Remove the directories that `deploy()` created for `removed_rels` and that are now
/// EMPTY, bottom-up, bounded strictly below the deploy root.
///
/// ## Why bottom-up `remove_dir` is safe (GAP-01 safety argument)
///
/// 1. **Manifest-derived candidates only.** The candidate directories are computed from
///    OUR manifest `target_rel`s (the rows purge/recovery just removed) — never from a
///    directory scan of the vanilla game tree. A directory that only ever held vanilla
///    content is therefore never even a candidate.
/// 2. **`remove_dir` refuses non-empty dirs.** We use `std::fs::remove_dir` (NOT
///    `remove_dir_all`): it errors with `DirectoryNotEmpty` on any dir that still holds a
///    file. That is the safety net — a vanilla dir that still holds vanilla files, or a
///    dir containing an unmanaged orphan, is left intact. That error is treated as a
///    benign "stop / leave it", not propagated. `NotFound` is likewise benign (idempotent
///    re-purge). Any other IO error IS propagated.
/// 3. **Strictly below the deploy root.** Candidates are constructed as the ancestor
///    chain strictly BETWEEN the deploy root (exclusive) and each file (exclusive), so
///    the game `Data/` boundary is never crossed. A defence-in-depth guard additionally
///    drops any candidate that equals or is an ancestor of the deploy root.
///
/// Sorting deepest-first ensures a child empties before its parent is attempted.
fn remove_emptied_dirs(install_dir: &Path, removed_rels: &[PathBuf]) -> Result<(), DeployError> {
    let root = crate::deploy_root(install_dir);
    let root_norm = lexical_normalize(&root);

    // Collect the unique set of ancestor directories strictly below the deploy root.
    let mut candidates: BTreeSet<PathBuf> = BTreeSet::new();
    for rel in removed_rels {
        let target = crate::resolve_target(install_dir, rel);
        // Walk UP from the file's parent toward the root, collecting each dir strictly
        // below the root (we stop the moment we reach the root itself).
        let mut dir = target.parent();
        while let Some(d) = dir {
            let d_norm = lexical_normalize(d);
            // Stop at (and never include) the deploy root or anything at/above it.
            if d_norm == root_norm || !d_norm.starts_with(&root_norm) {
                break;
            }
            candidates.insert(d_norm.clone());
            dir = d.parent();
        }
    }

    // Deepest-first: longest component count first, so children empty before parents.
    let mut ordered: Vec<PathBuf> = candidates.into_iter().collect();
    ordered.sort_by_key(|d| std::cmp::Reverse(d.components().count()));

    for dir in &ordered {
        // Defence-in-depth: never remove the deploy root or any ancestor of it.
        if *dir == root_norm || root_norm.starts_with(dir) {
            continue;
        }
        match std::fs::remove_dir(dir) {
            Ok(()) => {}
            Err(e)
                if e.kind() == std::io::ErrorKind::DirectoryNotEmpty
                    || e.kind() == std::io::ErrorKind::NotFound =>
            {
                // Benign: a vanilla dir still holding files / a dir with an unmanaged
                // orphan (DirectoryNotEmpty), or an already-gone dir (NotFound, idempotent
                // re-purge). Leave it — this is the safety net, not a failure.
            }
            Err(e) => return Err(DeployError::io(dir, e)),
        }
    }
    Ok(())
}

/// Replay any non-`done` journal rows on launch to reach a consistent state, then
/// auto-run a verify pass so an abnormal exit always yields a drift report (DEPLOY-07).
pub fn recover_on_launch(store: &Store, game: &Game) -> Result<RecoveryReport, DeployError> {
    let outcome = journal::replay(store, game)?;
    // If recovery rolled any PURGE rows forward (a crash-mid-purge), the directories the
    // original deploy created may now be empty — clean them up exactly as purge() does,
    // from the journal-derived relpath set (never a disk scan), so a crash-then-recover
    // converges to a directory-pristine tree (GAP-01 / T-01-21).
    if !outcome.purged_rels.is_empty() {
        remove_emptied_dirs(&game.install_dir, &outcome.purged_rels)?;
    }
    // After journal replay reaches a consistent DB+disk state, hash-diff the manifest
    // against disk so any external drift (orphans / missing / changed) is surfaced
    // automatically — never blindly repaired or deleted here (the UI decides).
    let drift = crate::verify::verify(store, game)?;
    Ok(RecoveryReport {
        replayed: outcome.replayed,
        drift,
    })
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
