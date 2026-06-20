//! verify/repair drift detection (DEPLOY-07): hash-diff the per-game manifest against
//! the on-disk game tree.
//!
//! After a deploy, the per-game manifest (`list_deployed_files`) is the source of truth
//! for what NexTwist placed and the blake3 hash it placed. The on-disk tree can drift
//! out from under us — a crash mid-op, a Steam re-verify, or the user editing a file.
//! `verify` classifies that drift into three buckets WITHOUT ever mutating disk:
//!
//! * `missing` — recorded in the manifest but absent on disk.
//! * `changed` — recorded, present, but the on-disk bytes hash differently than recorded.
//! * `orphan`  — present on disk under the deploy root but neither recorded by us nor a
//!   known vanilla original we backed up. These are UNMANAGED files (user-added, or the
//!   untouched vanilla tree) — they are REPORTED, never deleted (Pitfall 4 / T-01-16).
//!
//! `repair` re-deploys `missing` files from staging and restores `changed` files to the
//! recorded state, but NEVER deletes orphans — only manifest-recorded paths are ever
//! touched, so an unmanaged file mistaken for an orphan can never be destroyed.
//!
//! `verify` auto-runs after `recover_on_launch`'s journal replay so an abnormal exit
//! always yields an automatic drift report (RESEARCH.md Pattern 1: full-pristine-or-report).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use nextwist_core::{FileEntry, Game};
use store::Store;
use walkdir::WalkDir;

use crate::backup;
use crate::error::DeployError;
use crate::method::apply_idempotent;

/// The result of a [`verify`] pass: manifest-vs-disk drift, classified.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerifyReport {
    /// Recorded files absent on disk (absolute target paths).
    pub missing: Vec<PathBuf>,
    /// Recorded files whose on-disk bytes hash differently than recorded.
    pub changed: Vec<PathBuf>,
    /// On-disk files under the deploy root that provenance does not explain. Reported,
    /// never deleted.
    pub orphans: Vec<PathBuf>,
    /// True when no drift of any kind was found.
    pub pristine: bool,
}

/// The result of a [`repair`] pass.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepairReport {
    /// Number of `missing` files re-deployed from staging.
    pub restored_missing: usize,
    /// Number of `changed` files restored to the recorded state.
    pub restored_changed: usize,
    /// Orphans surfaced for the UI — NEVER deleted by repair.
    pub orphans: Vec<PathBuf>,
}

/// Hash-diff the manifest against the on-disk game tree, classifying drift.
///
/// Read-only: this NEVER mutates disk. For each recorded [`FileEntry`], the on-disk
/// target is checked for existence (else `missing`) and blake3-hashed (else `changed`
/// when the digest differs from `entry.hash`). The deploy root is then walked and any
/// regular file that is neither a recorded target nor a known vanilla-backed original is
/// reported as an `orphan`.
pub fn verify(store: &Store, game: &Game) -> Result<VerifyReport, DeployError> {
    let entries = store.list_deployed_files(game.appid)?;
    let mut report = VerifyReport::default();

    // Build the set of absolute target paths we manage, for the orphan walk.
    let mut managed: HashSet<PathBuf> = HashSet::new();

    for entry in &entries {
        let target = crate::resolve_target(&game.install_dir, &entry.target_rel);
        managed.insert(target.clone());

        if !path_exists(&target) {
            report.missing.push(target);
            continue;
        }
        // A symlink/hardlink/reflink/copy we placed: hash whatever bytes it resolves to
        // and compare against the recorded deploy-time hash.
        match hash_on_disk(&target) {
            Ok(hash) if hash == entry.hash => { /* intact */ }
            Ok(_) => report.changed.push(target),
            // A dangling symlink (target removed underneath a link) reads as missing.
            Err(_) => report.missing.push(target),
        }
    }

    // Walk the deploy root; classify any regular file we do not manage and that is not a
    // known vanilla-backed original as an orphan (reported, never deleted).
    let data_dir = crate::deploy_root(&game.install_dir);
    report.orphans = walk_orphans(store, game, &data_dir, &managed)?;

    report.pristine =
        report.missing.is_empty() && report.changed.is_empty() && report.orphans.is_empty();
    Ok(report)
}

/// Re-deploy `missing` files from staging and restore `changed` files to the recorded
/// state; surface (never delete) any orphans.
///
/// Sources are reconstructed from the per-game staging tree as
/// `game.staging_dir.join(target_rel)` — the same contract `journal::replay` uses (Plan
/// 04). A missing/changed file whose staged source is no longer locatable is skipped
/// (reported via the unchanged verify pass on the next call) rather than guessed at.
pub fn repair(store: &Store, game: &Game) -> Result<RepairReport, DeployError> {
    // Start from a fresh verify so we only act on real, current drift.
    let report = verify(store, game)?;
    let entries = store.list_deployed_files(game.appid)?;

    let mut out = RepairReport {
        restored_missing: 0,
        restored_changed: 0,
        orphans: report.orphans.clone(),
    };

    // Map absolute target -> recorded entry for the drift sets.
    for entry in &entries {
        let target = crate::resolve_target(&game.install_dir, &entry.target_rel);
        let is_missing = report.missing.contains(&target);
        let is_changed = report.changed.contains(&target);
        if !is_missing && !is_changed {
            continue;
        }
        if redeploy_from_staging(game, entry, &target)? {
            if is_missing {
                out.restored_missing += 1;
            } else {
                out.restored_changed += 1;
            }
        }
    }

    Ok(out)
}

/// Re-place a single recorded file from its staged source idempotently. Returns whether
/// the file was actually restored (false if its staged source is no longer present).
fn redeploy_from_staging(
    game: &Game,
    entry: &FileEntry,
    target: &Path,
) -> Result<bool, DeployError> {
    let staged_src = game.staging_dir.join(&entry.target_rel);
    if !staged_src.is_file() {
        // The staged source is gone; cannot re-deploy. Leave the file as-is (verify will
        // continue to report it) rather than fabricate content.
        return Ok(false);
    }
    // apply_idempotent is remove-if-present-then-create, so restoring a `changed` file
    // (overwrite) and a `missing` file (fresh create) share one safe path.
    apply_idempotent(entry.method, &staged_src, target)?;
    Ok(true)
}

/// Walk `data_dir` and return every regular file (absolute path) that is neither a
/// managed target nor a known vanilla-backed original.
fn walk_orphans(
    store: &Store,
    game: &Game,
    data_dir: &Path,
    managed: &HashSet<PathBuf>,
) -> Result<Vec<PathBuf>, DeployError> {
    let mut orphans = Vec::new();
    if !data_dir.exists() {
        return Ok(orphans);
    }
    for entry in WalkDir::new(data_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        // Consider regular files and symlinks (a placed symlink is a "file" to us); skip
        // directories.
        let ft = entry.file_type();
        if ft.is_dir() {
            continue;
        }
        let path = entry.path().to_path_buf();
        if managed.contains(&path) {
            continue;
        }
        // A path we backed up a vanilla original for is explained (the vanilla file may
        // legitimately sit here when no mod overwrote it). Compute its Data/-relative
        // path and check the vanilla ledger.
        if let Some(rel) = data_relative(&game.install_dir, &path) {
            if store.vanilla_for(game.appid, &rel)?.is_some() {
                continue;
            }
        }
        orphans.push(path);
    }
    Ok(orphans)
}

/// Express an absolute on-disk path as a `Data/`-rooted relpath (matching the manifest's
/// `target_rel` shape) so it can be looked up in the vanilla ledger. Returns `None` if
/// the path is not under the deploy root.
fn data_relative(install_dir: &Path, abs: &Path) -> Option<PathBuf> {
    let root = crate::deploy_root(install_dir);
    let rel = abs.strip_prefix(&root).ok()?;
    // The manifest stores target_rel as `Data/...`; re-root under the canonical Data name.
    let data_name = root.file_name()?;
    Some(Path::new(data_name).join(rel))
}

/// blake3-hash the bytes the path resolves to (following links, mirroring how the game
/// reads the deployed file).
fn hash_on_disk(path: &Path) -> Result<String, DeployError> {
    backup::blake3_file(path)
}

/// "Is there anything (file or symlink) at this path" — does not follow a dangling link
/// into nonexistence.
fn path_exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}
