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

    // The whole archive validated. Detect a cosmetic wrapper folder (e.g. an archive
    // packaged as `MyMod/Data/foo.esp` instead of `Data/foo.esp`) so the staged tree is
    // `Data/`-rooted rather than double-nested under `Data/MyMod/...`. This runs strictly
    // between extract-validate and the move — the validated extract→validate→move→
    // read-only ordering is preserved. (Carried Phase-2 gap; acute for FOMOD-02 because
    // `<file>/<folder>` source resolution depends on the detected archive root.)
    let move_source = detect_archive_root(temp_root)?;

    // Move the (possibly unwrapped) tree into the staging root.
    move_into_staging(&move_source, staging_root)?;
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

/// Recognized top-level game-root items (case-insensitive). A wrapper directory that
/// directly contains one of these — or a `Data` folder — is treated as the real root.
///
/// Kept SMALL and explicit (threat T-04-04): a too-broad list would wrongly flatten a
/// legitimate multi-folder mod. `Data` is handled separately below (it is the dominant
/// Bethesda root); these are the common script-extender / config siblings that ship at the
/// game root alongside `Data`.
const RECOGNIZED_ROOT_ITEMS: &[&str] = &["data", "skse", "skse64", "f4se", "obse", "nvse", "mwse"];

/// Detect whether the validated `temp_root` is wrapped in a single cosmetic top-level
/// directory and, if so, return that subdirectory as the real move source. Otherwise
/// return `temp_root` unchanged.
///
/// Heuristic (RESEARCH Pitfall 1): the tree is "wrapped" iff its top level is EXACTLY one
/// directory (no sibling files or dirs) AND that directory directly contains a recognizable
/// game root — a child named `Data` (case-insensitively) or one of
/// [`RECOGNIZED_ROOT_ITEMS`]. A real multi-folder mod (more than one top-level entry) or a
/// tree already rooted at `Data/` is never flattened. Detection is applied at most once
/// (a single wrapper level), so `Outer/Inner/Data/...` only unwraps `Outer` when `Outer`
/// itself contains the recognizable root — it does not recursively strip arbitrary depth.
pub(crate) fn detect_archive_root(temp_root: &Path) -> Result<PathBuf, ExtractError> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(temp_root).map_err(|e| ExtractError::io(temp_root, e))? {
        let entry = entry.map_err(|e| ExtractError::io(temp_root, e))?;
        let path = entry.path();
        let ft = entry.file_type().map_err(|e| ExtractError::io(&path, e))?;
        entries.push((path, ft.is_dir()));
    }

    // Exactly one top-level entry, and it must be a directory.
    let single_dir = match entries.as_slice() {
        [(path, true)] => path.clone(),
        _ => return Ok(temp_root.to_path_buf()),
    };

    // The top level is already a recognized root (e.g. the single dir IS `Data/`): do NOT
    // flatten — that is a legitimately Data-rooted tree, not a wrapper.
    if is_recognized_root_name(&single_dir) {
        return Ok(temp_root.to_path_buf());
    }

    // The single wrapper dir must itself contain a recognizable game root to be unwrapped.
    if wrapper_contains_recognized_root(&single_dir)? {
        tracing::debug!(
            wrapper = %single_dir.display(),
            "detected cosmetic wrapper folder; staging from its contents",
        );
        return Ok(single_dir);
    }

    Ok(temp_root.to_path_buf())
}

/// Whether `path`'s file name is a recognized game-root token (case-insensitive).
fn is_recognized_root_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| {
            RECOGNIZED_ROOT_ITEMS
                .iter()
                .any(|tok| n.eq_ignore_ascii_case(tok))
        })
        .unwrap_or(false)
}

/// Whether `dir` directly contains a recognizable game-root child (case-insensitive).
fn wrapper_contains_recognized_root(dir: &Path) -> Result<bool, ExtractError> {
    for entry in std::fs::read_dir(dir).map_err(|e| ExtractError::io(dir, e))? {
        let entry = entry.map_err(|e| ExtractError::io(dir, e))?;
        if is_recognized_root_name(&entry.path()) {
            return Ok(true);
        }
    }
    Ok(false)
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

#[cfg(test)]
mod root_detection_tests {
    use super::detect_archive_root;
    use std::fs;
    use std::path::Path;

    /// Create an empty file at `root/rel`, making parent dirs as needed.
    fn touch(root: &Path, rel: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, b"x").unwrap();
    }

    #[test]
    fn wrapper_folder_is_flattened() {
        // `MyMod/Data/foo.esp` ⇒ the move source becomes `.../MyMod`, so staging is
        // `Data/foo.esp` (NOT `MyMod/Data/foo.esp`).
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "MyMod/Data/foo.esp");

        let source = detect_archive_root(tmp.path()).unwrap();
        assert_eq!(source, tmp.path().join("MyMod"));
        assert!(source.join("Data/foo.esp").is_file());
    }

    #[test]
    fn already_data_rooted_is_unchanged() {
        // A tree already rooted at `Data/foo.esp` must NOT be flattened (the single
        // top-level dir IS `Data`, a recognized root — not a cosmetic wrapper).
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "Data/foo.esp");

        let source = detect_archive_root(tmp.path()).unwrap();
        assert_eq!(source, tmp.path());
    }

    #[test]
    fn multi_folder_mod_is_never_flattened() {
        // More than one top-level entry ⇒ a legitimate multi-folder mod; leave it as-is
        // even though one child is a recognizable root. This is the T-04-04 guard.
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "Data/foo.esp");
        touch(tmp.path(), "readme.txt");

        let source = detect_archive_root(tmp.path()).unwrap();
        assert_eq!(source, tmp.path());
    }

    #[test]
    fn nested_wrapper_unwraps_only_one_level() {
        // `Outer/Inner/Data/...`: the single top-level dir is `Outer`, but `Outer` does
        // NOT directly contain a recognized root (only `Inner` does) ⇒ NOT flattened.
        // Detection strips at most one cosmetic level and never guesses through depth.
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "Outer/Inner/Data/foo.esp");

        let source = detect_archive_root(tmp.path()).unwrap();
        assert_eq!(source, tmp.path(), "Outer lacks a direct Data child ⇒ unchanged");
    }

    #[test]
    fn known_top_level_item_is_recognized_root() {
        // A single wrapper dir whose child is a known top-level game item (`SKSE`) is
        // treated as the root and unwrapped.
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "WrapDir/SKSE/plugins/foo.dll");

        let source = detect_archive_root(tmp.path()).unwrap();
        assert_eq!(source, tmp.path().join("WrapDir"));
        assert!(source.join("SKSE/plugins/foo.dll").is_file());
    }
}
