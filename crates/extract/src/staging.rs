//! The `install_archive` orchestrator: extract-to-temp → validate → move → lock.
//!
//! This is the public entry point. It never extracts directly into the staging
//! root or the game tree: it extracts into a `tempfile::TempDir`, validates every
//! entry as it goes, and only moves the finished tree into `staging_root` once the
//! WHOLE archive has validated. Staged files are then marked read-only so a later
//! hardlink deploy cannot be mutated through the staging copy.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use crate::validate::ExtractError;
use crate::{list_files_rel, mark_tree_readonly, rar, sevenz, zip, ArchiveFormat};

/// A validated, read-only per-mod staging tree produced by [`install_archive`].
///
/// Derives serde so the Tauri command layer (Plan 06) can return it to the webview and
/// receive it back for the deploy call — it maps 1:1 onto `deploy::StagedFiles`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedMod {
    /// Root directory of the staged tree (a child of the supplied `staging_root`).
    pub staging_root: PathBuf,
    /// Every staged regular file, as a path relative to `staging_root`.
    pub files: Vec<PathBuf>,
}

/// Install `archive` into `staging_root`, returning the validated [`StagedMod`].
///
/// `staging_root` is the directory the staged tree should occupy. It must not
/// already exist (or must be empty) — the validated temp tree is moved into it.
///
/// Steps: detect format → extract into a temp dir (validating every entry) → move
/// the validated tree into `staging_root` → mark every file read-only → return the
/// file manifest.
pub fn install_archive(archive: &Path, staging_root: &Path) -> Result<StagedMod, ExtractError> {
    if !archive.is_file() {
        return Err(ExtractError::io(
            archive,
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "archive path is not an existing file",
            ),
        ));
    }

    let format = ArchiveFormat::detect(archive)?;
    tracing::debug!(?format, archive = %archive.display(), "installing archive");

    // Extract into a temp dir on, ideally, the same filesystem as the staging
    // root so the final move is a cheap rename rather than a copy. Falling back to
    // the system temp dir is fine (the move handles cross-device automatically).
    let temp = make_temp_near(staging_root)?;
    let temp_root = temp.path();

    match format {
        ArchiveFormat::Zip => zip::extract_zip(archive, temp_root)?,
        ArchiveFormat::SevenZip => sevenz::extract_7z(archive, temp_root)?,
        ArchiveFormat::Rar => rar::extract_rar(archive, temp_root)?,
    }

    // The whole archive validated. Move the temp tree into the staging root.
    move_into_staging(temp_root, staging_root)?;
    // Consume the TempDir guard without deleting (contents were moved out).
    let _ = temp.keep();

    // Lock the staged tree down and enumerate it.
    mark_tree_readonly(staging_root)?;
    let files = list_files_rel(staging_root)?;

    Ok(StagedMod {
        staging_root: staging_root.to_path_buf(),
        files,
    })
}

/// Create a temp directory near `staging_root` (same filesystem when possible).
fn make_temp_near(staging_root: &Path) -> Result<TempDir, ExtractError> {
    // Prefer the staging root's parent so the later move is an intra-fs rename.
    if let Some(parent) = staging_root.parent() {
        if parent.is_dir() {
            return tempfile::Builder::new()
                .prefix(".nextwist-extract-")
                .tempdir_in(parent)
                .map_err(|e| ExtractError::io(parent, e));
        }
        // Parent does not exist yet — create it so staging can land there too.
        std::fs::create_dir_all(parent).map_err(|e| ExtractError::io(parent, e))?;
        return tempfile::Builder::new()
            .prefix(".nextwist-extract-")
            .tempdir_in(parent)
            .map_err(|e| ExtractError::io(parent, e));
    }
    tempfile::Builder::new()
        .prefix(".nextwist-extract-")
        .tempdir()
        .map_err(|e| ExtractError::io(staging_root, e))
}

/// Move the validated `temp_root` tree into `staging_root`.
///
/// Tries a single atomic rename first (fast, same-fs). If that fails — typically
/// because the temp dir and staging root are on different filesystems, or staging
/// already exists — it falls back to a recursive per-file move that preserves the
/// validated layout.
fn move_into_staging(temp_root: &Path, staging_root: &Path) -> Result<(), ExtractError> {
    if let Some(parent) = staging_root.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ExtractError::io(parent, e))?;
    }

    // Fast path: atomic rename when the staging root does not yet exist.
    if !staging_root.exists() {
        match std::fs::rename(temp_root, staging_root) {
            Ok(()) => return Ok(()),
            Err(_) => { /* fall through to recursive move (cross-device etc.) */ }
        }
    }

    std::fs::create_dir_all(staging_root).map_err(|e| ExtractError::io(staging_root, e))?;
    recursive_move(temp_root, staging_root)
}

/// Recursively move the contents of `from` into `to`, creating directories and
/// copy-then-removing files when a plain rename is not possible.
fn recursive_move(from: &Path, to: &Path) -> Result<(), ExtractError> {
    for entry in std::fs::read_dir(from).map_err(|e| ExtractError::io(from, e))? {
        let entry = entry.map_err(|e| ExtractError::io(from, e))?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let ft = entry.file_type().map_err(|e| ExtractError::io(&src, e))?;
        if ft.is_dir() {
            std::fs::create_dir_all(&dst).map_err(|e| ExtractError::io(&dst, e))?;
            recursive_move(&src, &dst)?;
        } else {
            if std::fs::rename(&src, &dst).is_err() {
                std::fs::copy(&src, &dst).map_err(|e| ExtractError::io(&dst, e))?;
                std::fs::remove_file(&src).map_err(|e| ExtractError::io(&src, e))?;
            }
        }
    }
    Ok(())
}
