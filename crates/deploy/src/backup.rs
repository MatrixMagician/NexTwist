//! Backup-before-overwrite into a content-addressed vanilla store (DEPLOY-04).
//!
//! This is the single most important safety mechanism: corruption of a vanilla game
//! file is otherwise only fixable by a Steam re-verify. Before a deploy overwrites
//! any pre-existing game file that NexTwist did not itself deploy, we copy the
//! original into a per-game content-addressed store keyed by its blake3 hash and
//! record `(appid, target_rel, hash)` in the `vanilla_backup` ledger. Purge restores
//! the exact original bytes from that store.
//!
//! ## Store layout
//!
//! `<originals_root>/<appid>/<blake3-hex>` — the hash both keys the blob (so multiple
//! targets with identical original content dedupe to one file) and lets us verify the
//! restored bytes are byte-for-byte the original.

use std::fs;
use std::path::{Path, PathBuf};

use nextwist_core::Game;
use store::Store;

use crate::error::DeployError;

/// The per-game content-addressed vanilla store directory.
///
/// Placed as a sibling of the game's staging dir (`<staging>/../originals/<appid>`)
/// so it lives in NexTwist's app-managed area, never inside the game tree.
pub fn originals_dir(game: &Game) -> PathBuf {
    let base = game
        .staging_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| game.staging_dir.clone());
    base.join("originals").join(game.appid.to_string())
}

/// blake3-hash a file's bytes, returning the lowercase hex digest.
pub fn blake3_file(path: &Path) -> Result<String, DeployError> {
    let bytes = fs::read(path).map_err(|e| DeployError::io(path, e))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

/// Back up the vanilla file at `target` before it is overwritten, if it is a real
/// pre-existing file that NexTwist did not deploy. Returns `true` if a backup was
/// taken (i.e. the target was a vanilla file), `false` for a pure add.
///
/// Idempotent and content-addressed: re-running for the same original is a no-op
/// (the blob already exists and the row is upserted). A target that is one of our own
/// deployed files (a symlink/hardlink/reflink into staging) is NOT vanilla and is
/// skipped — `is_ours` is decided from the manifest, not by guessing.
pub fn backup_vanilla_if_absent(
    store: &Store,
    game: &Game,
    target: &Path,
    target_rel: &Path,
) -> Result<bool, DeployError> {
    // Nothing on disk -> pure add, nothing to back up.
    if !path_exists(target) {
        return Ok(false);
    }
    // Already recorded as ours in the manifest -> not a vanilla file.
    if is_ours(store, game.appid, target_rel)? {
        return Ok(false);
    }
    // A symlink we previously deployed (cross-device) also isn't vanilla; but a
    // symlink we did NOT record is suspicious — treat a non-recorded regular file as
    // the vanilla original. We only back up regular files (never follow into staging).
    let meta = fs::symlink_metadata(target).map_err(|e| DeployError::io(target, e))?;
    if meta.file_type().is_symlink() {
        // A stray symlink at the target that we don't own: do not copy through it.
        return Ok(false);
    }

    let hash = blake3_file(target)?;
    let dir = originals_dir(game);
    fs::create_dir_all(&dir).map_err(|e| DeployError::io(&dir, e))?;
    let blob = dir.join(&hash);
    if !path_exists(&blob) {
        // Content-addressed dedupe: only copy if this content key is absent on disk.
        fs::copy(target, &blob).map_err(|e| DeployError::io(&blob, e))?;
    }
    store.record_vanilla(game.appid, target_rel, &hash)?;
    Ok(true)
}

/// Restore the recorded vanilla original for `target_rel` back to `target`, if one
/// was backed up. Returns `true` if a restore happened. Idempotent: restoring the
/// same bytes twice yields the same result.
pub fn restore_vanilla(
    store: &Store,
    game: &Game,
    target: &Path,
    target_rel: &Path,
) -> Result<bool, DeployError> {
    let Some(hash) = store.vanilla_for(game.appid, target_rel)? else {
        return Ok(false);
    };
    let blob = originals_dir(game).join(&hash);
    if !path_exists(&blob) {
        return Err(DeployError::NotPristine(format!(
            "vanilla blob {hash} missing from store for {}",
            target_rel.display()
        )));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| DeployError::io(parent, e))?;
    }
    // Remove any current (ours) placement first, then copy the original back.
    crate::method::remove_if_present(target).map_err(|e| DeployError::io(target, e))?;
    fs::copy(&blob, target).map_err(|e| DeployError::io(target, e))?;
    Ok(true)
}

/// Whether `appid`/`target_rel` is recorded in the deploy manifest as one of ours.
fn is_ours(store: &Store, appid: u32, target_rel: &Path) -> Result<bool, DeployError> {
    let files = store.list_deployed_files(appid)?;
    Ok(files.iter().any(|f| f.target_rel == target_rel))
}

/// `Path::exists` that does not follow a dangling symlink into nonexistence — we
/// want "is there anything (file or symlink) at this path".
fn path_exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}
