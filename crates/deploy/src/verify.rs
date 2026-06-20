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
use serde::{Deserialize, Serialize};
use store::Store;
use walkdir::WalkDir;

use crate::backup;
use crate::error::DeployError;
use crate::method::apply_idempotent;

/// The result of a [`verify`] pass: manifest-vs-disk drift, classified.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VerifyReport {
    /// Recorded files absent on disk (absolute target paths).
    pub missing: Vec<PathBuf>,
    /// Recorded files whose on-disk bytes hash differently than recorded.
    pub changed: Vec<PathBuf>,
    /// On-disk files under the deploy root that provenance does not explain. Reported,
    /// never deleted.
    pub orphans: Vec<PathBuf>,
    /// EMPTY directories under the deploy root that provenance does not explain — not on
    /// the ancestor path of any managed target nor of a vanilla-backed original. A
    /// non-empty directory is NEVER an orphan dir (its contents are classified as file
    /// orphans/managed). `repair` removes exactly these (and only these). Distinct from
    /// `orphans` (files) so the UI can tell the two apart.
    pub orphan_dirs: Vec<PathBuf>,
    /// True when no drift of any kind was found.
    pub pristine: bool,
}

/// The result of a [`repair`] pass.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RepairReport {
    /// Number of `missing` files re-deployed from staging.
    pub restored_missing: usize,
    /// Number of `changed` files restored to the recorded state.
    pub restored_changed: usize,
    /// Number of orphan EMPTY directories removed (the ONLY thing repair deletes — file
    /// orphans remain report-only, so no file is ever deleted; T-01-16 / T-01-20).
    pub removed_orphan_dirs: usize,
    /// Orphans (files) surfaced for the UI — NEVER deleted by repair.
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

    // Collect EMPTY directories under the deploy root that provenance does not explain —
    // not on the ancestor path of any managed target nor of a vanilla-backed original.
    report.orphan_dirs = walk_orphan_dirs(store, game, &data_dir, &managed)?;

    report.pristine = report.missing.is_empty()
        && report.changed.is_empty()
        && report.orphans.is_empty()
        && report.orphan_dirs.is_empty();
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
        removed_orphan_dirs: 0,
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

    // Remove the orphan EMPTY dirs (the ONLY thing repair ever deletes — file orphans
    // stay report-only, T-01-16 / T-01-20). Iterate to a fixed point so a chain of nested
    // empty dirs (a leaf whose parent becomes empty once the leaf is gone) is fully
    // cleaned in one repair call, leaving the tree pristine. Each pass removes deepest-
    // first; `remove_dir` refuses non-empty dirs (so a dir that holds an unmanaged file is
    // never removed); NotFound/DirectoryNotEmpty are benign skips, any other IO error
    // propagates. Never the deploy root or an ancestor of it.
    let root = crate::deploy_root(&game.install_dir);
    let managed: HashSet<PathBuf> = entries
        .iter()
        .map(|e| crate::resolve_target(&game.install_dir, &e.target_rel))
        .collect();
    let data_dir = crate::deploy_root(&game.install_dir);
    loop {
        let mut dirs = walk_orphan_dirs(store, game, &data_dir, &managed)?;
        if dirs.is_empty() {
            break;
        }
        dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
        let mut removed_any = false;
        for dir in &dirs {
            if *dir == root || root.starts_with(dir) {
                continue;
            }
            match std::fs::remove_dir(dir) {
                Ok(()) => {
                    out.removed_orphan_dirs += 1;
                    removed_any = true;
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::DirectoryNotEmpty
                        || e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(DeployError::io(dir, e)),
            }
        }
        // No progress this pass (every candidate refused) — stop to avoid a busy loop.
        if !removed_any {
            break;
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
        if let Some(rel) = data_relative(&game.install_dir, &path)
            && store.vanilla_for(game.appid, &rel)?.is_some()
        {
            continue;
        }
        orphans.push(path);
    }
    Ok(orphans)
}

/// Walk `data_dir` and return every EMPTY directory (absolute path) under the deploy root
/// that provenance does not explain — i.e. not on the ancestor path of any managed target
/// nor of any vanilla-backed original.
///
/// "Explained" directories are the ancestor chains (strictly below the deploy root) of
/// every managed target and every vanilla-backed original: those are directories
/// NexTwist (or the vanilla tree we backed up) legitimately relies on. Any other directory
/// under the root that is currently EMPTY is an orphan dir — typically a directory deploy
/// created that a later edit (or a partial cleanup) left behind. The deploy root itself is
/// never reported. A non-empty directory is never an orphan dir (its file contents are
/// classified by [`walk_orphans`]); `remove_dir` in repair is the second safety net.
fn walk_orphan_dirs(
    store: &Store,
    game: &Game,
    data_dir: &Path,
    managed: &HashSet<PathBuf>,
) -> Result<Vec<PathBuf>, DeployError> {
    let mut orphan_dirs = Vec::new();
    if !data_dir.exists() {
        return Ok(orphan_dirs);
    }

    // Build the set of "explained" directories: every ancestor (under the root) of a
    // managed target or a vanilla-backed original. An empty dir not in this set, and not
    // the root, is unexplained.
    let mut explained: HashSet<PathBuf> = HashSet::new();
    let mut add_ancestors = |target: &Path| {
        let mut p = target.parent();
        while let Some(d) = p {
            if d == data_dir || !d.starts_with(data_dir) {
                break;
            }
            explained.insert(d.to_path_buf());
            p = d.parent();
        }
    };
    for target in managed {
        add_ancestors(target);
    }
    // Note: a vanilla-backed original's directory is, by construction, also the ancestor
    // of the managed target that overwrote it (same relpath), so `managed` already
    // explains it; we still re-check the vanilla ledger per empty dir below as a belt-and-
    // braces guard against ever removing a dir a vanilla file legitimately sits in.

    for entry in WalkDir::new(data_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path();
        // Never report the deploy root itself.
        if path == data_dir {
            continue;
        }
        // Only EMPTY directories are orphan-dir candidates.
        let is_empty = std::fs::read_dir(path)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false);
        if !is_empty {
            continue;
        }
        // An empty dir that is NOT an explained ancestor of any managed/vanilla target is
        // an orphan dir. (An explained dir cannot actually be empty while it still holds a
        // managed target, but we keep the check so a vanilla-pre-existing dir whose only
        // child a mod overwrote — then purged — is never misclassified.)
        if explained.contains(path) {
            continue;
        }
        // A directory that holds a known vanilla-backed original is explained — but such a
        // dir is non-empty, so the is_empty guard already excluded it. Empty + unexplained:
        if let Some(rel) = data_relative(&game.install_dir, path)
            && store.vanilla_for(game.appid, &rel)?.is_some()
        {
            continue;
        }
        orphan_dirs.push(path.to_path_buf());
    }
    Ok(orphan_dirs)
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
