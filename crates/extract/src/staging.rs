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
    let plan = detect_archive_root(temp_root)?;

    // Move the (possibly unwrapped, possibly filtered) tree into the staging root.
    move_into_staging(&plan, staging_root)?;
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

/// The move plan produced by [`detect_archive_root`]: which source tree, and (when a
/// cosmetic wrapper was unwrapped) which of its children are game content worth staging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MoveSource {
    /// No wrapper detected (already `Data/`-rooted, a loose-file mod, or a multi-folder
    /// mod): stage the validated tree at `root` VERBATIM — every entry is kept. This is the
    /// conservative path; a legitimate loose-file mod whose archive root already IS the
    /// `Data/` contents must not have any file dropped.
    Whole { root: PathBuf },
    /// A single cosmetic wrapper directory was detected. Stage ONLY its recognized-root
    /// children (`Data/`, `SKSE/`, …) and drop non-game siblings (`Info.txt`,
    /// `Screenshot/`, readmes, `fomod/` config dir, …) which would otherwise leak into the
    /// game `Data/` directory at deploy time.
    WrapperChildren { wrapper: PathBuf, children: Vec<PathBuf> },
}

/// Detect whether the validated `temp_root` is wrapped in a single cosmetic top-level
/// directory and, if so, plan to stage only that directory's GAME content. Otherwise
/// plan to stage the whole tree unchanged.
///
/// Heuristic (RESEARCH Pitfall 1): the tree is "wrapped" iff its top level is EXACTLY one
/// directory (no sibling files or dirs) AND that directory directly contains a recognizable
/// game root — a child named `Data` (case-insensitively) or one of
/// [`RECOGNIZED_ROOT_ITEMS`]. A real multi-folder mod (more than one top-level entry) or a
/// tree already rooted at `Data/` is never flattened. Detection is applied at most once
/// (a single wrapper level), so `Outer/Inner/Data/...` only unwraps `Outer` when `Outer`
/// itself contains the recognizable root — it does not recursively strip arbitrary depth.
///
/// When a wrapper IS unwrapped, only its recognized-root children are staged (Vortex/MO2
/// "fixup" behavior): the wrapper's structure has told us the game layout, so non-game
/// siblings are documentation/junk and are excluded rather than copied into the game
/// `Data/`. The no-wrapper path stays verbatim — it has no such signal and a loose-file
/// mod's files legitimately belong directly in `Data/`.
pub(crate) fn detect_archive_root(temp_root: &Path) -> Result<MoveSource, ExtractError> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(temp_root).map_err(|e| ExtractError::io(temp_root, e))? {
        let entry = entry.map_err(|e| ExtractError::io(temp_root, e))?;
        let path = entry.path();
        let ft = entry.file_type().map_err(|e| ExtractError::io(&path, e))?;
        entries.push((path, ft.is_dir()));
    }

    let whole = || MoveSource::Whole { root: temp_root.to_path_buf() };

    // Exactly one top-level entry, and it must be a directory.
    let single_dir = match entries.as_slice() {
        [(path, true)] => path.clone(),
        _ => return Ok(whole()),
    };

    // The top level is already a recognized root (e.g. the single dir IS `Data/`): do NOT
    // flatten — that is a legitimately Data-rooted tree, not a wrapper.
    if is_recognized_root_name(&single_dir) {
        return Ok(whole());
    }

    // The single wrapper dir must itself contain at least one recognizable game root to be
    // unwrapped. Collect EVERY recognized-root child as the game content to stage; any other
    // child (Info.txt, Screenshot/, readmes, fomod/, …) is a non-game sibling we exclude.
    let children = recognized_root_children(&single_dir)?;
    if children.is_empty() {
        return Ok(whole());
    }

    tracing::debug!(
        wrapper = %single_dir.display(),
        kept = children.len(),
        "detected cosmetic wrapper folder; staging only its recognized-root children",
    );
    Ok(MoveSource::WrapperChildren {
        wrapper: single_dir,
        children,
    })
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

/// Collect `dir`'s direct children whose name is a recognized game-root token
/// (case-insensitive). These are the entries that constitute the mod's game content; all
/// other siblings are treated as non-game documentation/config and excluded. Returns paths
/// in sorted order so staging is deterministic regardless of `read_dir` order.
fn recognized_root_children(dir: &Path) -> Result<Vec<PathBuf>, ExtractError> {
    let mut kept = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| ExtractError::io(dir, e))? {
        let entry = entry.map_err(|e| ExtractError::io(dir, e))?;
        let path = entry.path();
        if is_recognized_root_name(&path) {
            kept.push(path);
        }
    }
    kept.sort();
    Ok(kept)
}

/// Move the planned source into `staging_root`.
///
/// For [`MoveSource::Whole`] this moves the entire validated tree; for
/// [`MoveSource::WrapperChildren`] it moves only the selected recognized-root children,
/// dropping the wrapper's non-game siblings. Either way it tries an atomic rename first
/// (fast, same-fs) and falls back to a recursive per-file move (cross-device, or when the
/// staging root already exists), preserving the validated layout.
fn move_into_staging(plan: &MoveSource, staging_root: &Path) -> Result<(), ExtractError> {
    if let Some(parent) = staging_root.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ExtractError::io(parent, e))?;
    }

    match plan {
        MoveSource::Whole { root } => move_dir_contents(root, staging_root),
        MoveSource::WrapperChildren { children, .. } => {
            // Stage only the recognized-root children; the wrapper itself and its non-game
            // siblings are left behind in the temp dir (cleaned up by the TempDir guard or
            // simply discarded). Create the staging root explicitly since we move into it
            // child-by-child rather than renaming a whole directory onto it.
            std::fs::create_dir_all(staging_root)
                .map_err(|e| ExtractError::io(staging_root, e))?;
            for child in children {
                let dst = staging_root.join(
                    child
                        .file_name()
                        .ok_or_else(|| ExtractError::io(child, bad_name_io()))?,
                );
                move_path(child, &dst)?;
            }
            Ok(())
        }
    }
}

/// Move the CONTENTS of `from` into `to`: atomic rename of the whole directory when `to`
/// does not yet exist, else a recursive per-file move.
fn move_dir_contents(from: &Path, to: &Path) -> Result<(), ExtractError> {
    // Fast path: atomic rename when the staging root does not yet exist.
    if !to.exists() {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(_) => { /* fall through to recursive move (cross-device etc.) */ }
        }
    }

    std::fs::create_dir_all(to).map_err(|e| ExtractError::io(to, e))?;
    recursive_move(from, to)
}

/// Move a single path (file or directory) from `src` to `dst`, falling back to a recursive
/// move when a plain rename is not possible (cross-device, or `dst` already exists).
fn move_path(src: &Path, dst: &Path) -> Result<(), ExtractError> {
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    if src.is_dir() {
        std::fs::create_dir_all(dst).map_err(|e| ExtractError::io(dst, e))?;
        recursive_move(src, dst)
    } else {
        std::fs::copy(src, dst).map_err(|e| ExtractError::io(dst, e))?;
        std::fs::remove_file(src).map_err(|e| ExtractError::io(src, e))
    }
}

/// An `InvalidInput` I/O error for a path with no final component (should be unreachable
/// for a `read_dir` child, but handled rather than `unwrap`ped).
fn bad_name_io() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "directory child has no file name",
    )
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
    use super::{detect_archive_root, MoveSource};
    use std::fs;
    use std::path::{Path, PathBuf};

    /// Create an empty file at `root/rel`, making parent dirs as needed.
    fn touch(root: &Path, rel: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, b"x").unwrap();
    }

    /// Assert the plan is `Whole { root }` rooted at `expected`.
    fn assert_whole(plan: &MoveSource, expected: &Path) {
        match plan {
            MoveSource::Whole { root } => assert_eq!(root, expected),
            other => panic!("expected Whole {{ root: {expected:?} }}, got {other:?}"),
        }
    }

    /// Assert the plan is `WrapperChildren` rooted at `wrapper` with exactly `names`
    /// (file-name, sorted) as the kept children; returns the child paths for follow-up.
    fn assert_wrapper(plan: &MoveSource, wrapper: &Path, names: &[&str]) -> Vec<PathBuf> {
        match plan {
            MoveSource::WrapperChildren {
                wrapper: w,
                children,
            } => {
                assert_eq!(w, wrapper, "wrapper dir mismatch");
                let got: Vec<String> = children
                    .iter()
                    .map(|c| c.file_name().unwrap().to_string_lossy().into_owned())
                    .collect();
                assert_eq!(got, names, "kept-children mismatch");
                children.clone()
            }
            other => panic!("expected WrapperChildren, got {other:?}"),
        }
    }

    #[test]
    fn wrapper_folder_is_flattened() {
        // `MyMod/Data/foo.esp` ⇒ plan stages the `MyMod` wrapper's `Data` child, so staging
        // is `Data/foo.esp` (NOT `MyMod/Data/foo.esp`).
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "MyMod/Data/foo.esp");

        let plan = detect_archive_root(tmp.path()).unwrap();
        let children = assert_wrapper(&plan, &tmp.path().join("MyMod"), &["Data"]);
        assert!(children[0].join("foo.esp").is_file());
    }

    #[test]
    fn wrapper_non_game_siblings_are_excluded() {
        // THE REGRESSION: `Wrapper/Data/Plugin.esp` + non-game `Wrapper/Info.txt` and
        // `Wrapper/Screenshot/shot.png` ⇒ only `Data` is staged; the junk siblings are
        // dropped so they never leak into the game `Data/` at deploy time.
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "Wrapper/Data/Plugin.esp");
        touch(tmp.path(), "Wrapper/Info.txt");
        touch(tmp.path(), "Wrapper/Screenshot/shot.png");

        let plan = detect_archive_root(tmp.path()).unwrap();
        // Only the `Data` recognized-root child survives.
        assert_wrapper(&plan, &tmp.path().join("Wrapper"), &["Data"]);
    }

    #[test]
    fn wrapper_keeps_all_recognized_roots() {
        // A wrapper carrying BOTH `Data/` and `F4SE/` (two recognized roots) plus junk:
        // both game roots are kept (sorted), the junk dropped.
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "Wrapper/Data/Plugin.esp");
        touch(tmp.path(), "Wrapper/F4SE/Plugins/x.dll");
        touch(tmp.path(), "Wrapper/readme.md");

        let plan = detect_archive_root(tmp.path()).unwrap();
        // Sorted by path ⇒ "Data" before "F4SE".
        assert_wrapper(&plan, &tmp.path().join("Wrapper"), &["Data", "F4SE"]);
    }

    #[test]
    fn already_data_rooted_is_unchanged() {
        // A tree already rooted at `Data/foo.esp` must NOT be flattened (the single
        // top-level dir IS `Data`, a recognized root — not a cosmetic wrapper).
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "Data/foo.esp");

        let plan = detect_archive_root(tmp.path()).unwrap();
        assert_whole(&plan, tmp.path());
    }

    #[test]
    fn multi_folder_mod_is_never_flattened() {
        // More than one top-level entry ⇒ a legitimate multi-folder mod; leave it as-is
        // even though one child is a recognizable root. This is the T-04-04 guard.
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "Data/foo.esp");
        touch(tmp.path(), "readme.txt");

        let plan = detect_archive_root(tmp.path()).unwrap();
        assert_whole(&plan, tmp.path());
    }

    #[test]
    fn loose_file_mod_is_kept_verbatim() {
        // A loose-file mod with NO Data/ wrapper and several top-level files belongs
        // directly in Data/; the no-wrapper path stages every file verbatim (no exclusion).
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "Plugin.esp");
        touch(tmp.path(), "textures/rock.dds");

        let plan = detect_archive_root(tmp.path()).unwrap();
        assert_whole(&plan, tmp.path());
    }

    #[test]
    fn nested_wrapper_unwraps_only_one_level() {
        // `Outer/Inner/Data/...`: the single top-level dir is `Outer`, but `Outer` does
        // NOT directly contain a recognized root (only `Inner` does) ⇒ NOT flattened.
        // Detection strips at most one cosmetic level and never guesses through depth.
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "Outer/Inner/Data/foo.esp");

        let plan = detect_archive_root(tmp.path()).unwrap();
        assert_whole(&plan, tmp.path());
    }

    #[test]
    fn known_top_level_item_is_recognized_root() {
        // A single wrapper dir whose child is a known top-level game item (`SKSE`) is
        // treated as the root and unwrapped (only `SKSE` kept).
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "WrapDir/SKSE/plugins/foo.dll");

        let plan = detect_archive_root(tmp.path()).unwrap();
        let children = assert_wrapper(&plan, &tmp.path().join("WrapDir"), &["SKSE"]);
        assert!(children[0].join("plugins/foo.dll").is_file());
    }
}
