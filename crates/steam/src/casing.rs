//! Per-game canonical `Data/` casing map (DEPLOY-08 input).
//!
//! Wine/Proton does NOT abstract the filesystem: a Windows `open("Data\\Textures\\x")`
//! becomes a case-sensitive Linux `open()`, so mixed-case mod paths (authored on
//! case-insensitive NTFS) silently fail to load (RESEARCH.md Pitfall 4). The deploy
//! engine's `casefold.rs` (Plan 05) rewrites incoming mod paths to the game's REAL
//! casing — and the knowledge of that real casing lives HERE.
//!
//! This module ONLY produces the canonical-casing knowledge; it performs NO rewriting
//! (that is deploy's job per the Responsibility Map). The map is a simple, serializable
//! structure keyed by the *relative-to-`Data/`* lowercase path, so deploy can look up
//! any incoming component chain and rewrite it to the on-disk casing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::SteamError;

/// A canonical-casing map for one game's `Data/` directory tree.
///
/// Keys are the lowercased relative path (components joined with `/`, e.g.
/// `textures/actors`); values are the actual on-disk relative path with its real
/// casing (e.g. `Textures/Actors`). The empty key maps to the canonical `Data` dir
/// name itself. Only *directories* are recorded — deploy normalizes directory
/// components; leaf filenames are matched separately at deploy time if needed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasingMap {
    /// The real on-disk name of the top-level data directory (e.g. `Data`).
    pub data_dir_name: String,
    /// lowercase relative dir path (under Data/) → canonical-cased relative dir path.
    pub dirs: BTreeMap<String, String>,
}

impl CasingMap {
    /// Look up the canonical casing for a lowercase relative directory path (under
    /// `Data/`, `/`-separated). Returns `None` if that directory does not exist
    /// on-disk under the game's `Data/` tree.
    pub fn canonical_dir(&self, lower_rel: &str) -> Option<&str> {
        self.dirs.get(lower_rel).map(String::as_str)
    }

    /// Number of directories recorded (excluding the `Data/` root itself).
    pub fn len(&self) -> usize {
        self.dirs.len()
    }

    /// True if no subdirectories were recorded.
    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty()
    }
}

/// Walk the game's `Data/` directory and produce its canonical-casing map.
///
/// `install_dir` is the resolved game install root (the directory that contains a
/// `Data/` subdirectory). The `Data/` directory itself is located case-insensitively
/// (a game may ship `Data` or, rarely, `data`). Every subdirectory under it is
/// recorded keyed by its lowercased relative path.
///
/// Returns [`SteamError::InvalidGameFolder`] if no `Data/` directory exists under
/// `install_dir`.
pub fn canonical_data_casing(install_dir: &Path) -> Result<CasingMap, SteamError> {
    let data_dir = find_data_dir(install_dir).ok_or_else(|| SteamError::InvalidGameFolder {
        path: install_dir.to_path_buf(),
        missing: "Data/ directory".to_string(),
    })?;

    let data_dir_name = data_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Data")
        .to_string();

    let mut dirs = BTreeMap::new();

    // Walk every directory strictly *under* Data/, recording relative paths.
    for entry in WalkDir::new(&data_dir)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_dir() {
            continue;
        }
        let rel = match entry.path().strip_prefix(&data_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let canonical = rel_to_slash(rel);
        if canonical.is_empty() {
            continue;
        }
        let lower = canonical.to_lowercase();
        dirs.insert(lower, canonical);
    }

    Ok(CasingMap {
        data_dir_name,
        dirs,
    })
}

/// Locate the `Data/` subdir under `install_dir`, case-insensitively.
fn find_data_dir(install_dir: &Path) -> Option<PathBuf> {
    let rd = std::fs::read_dir(install_dir).ok()?;
    for entry in rd.flatten() {
        let name = entry.file_name();
        if name
            .to_str()
            .is_some_and(|n| n.eq_ignore_ascii_case("data"))
            && entry.path().is_dir()
        {
            return Some(entry.path());
        }
    }
    None
}

/// Render a relative path as a `/`-joined string (deterministic, FS-independent key).
fn rel_to_slash(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn maps_mixed_case_data_tree_to_canonical_casing() {
        let dir = TempDir::new().unwrap();
        let install = dir.path();
        // A realistic mixed-case Bethesda Data tree.
        std::fs::create_dir_all(install.join("Data/Textures/Actors")).unwrap();
        std::fs::create_dir_all(install.join("Data/Meshes")).unwrap();
        std::fs::create_dir_all(install.join("Data/Scripts")).unwrap();
        std::fs::create_dir_all(install.join("Data/Interface")).unwrap();

        let map = canonical_data_casing(install).unwrap();
        assert_eq!(map.data_dir_name, "Data");

        // Incoming mod path "textures" → on-disk "Textures".
        assert_eq!(map.canonical_dir("textures"), Some("Textures"));
        assert_eq!(map.canonical_dir("textures/actors"), Some("Textures/Actors"));
        assert_eq!(map.canonical_dir("meshes"), Some("Meshes"));
        assert_eq!(map.canonical_dir("scripts"), Some("Scripts"));
        assert_eq!(map.canonical_dir("interface"), Some("Interface"));
        // A directory the game doesn't have → None.
        assert_eq!(map.canonical_dir("sound"), None);
        assert!(!map.is_empty());
    }

    #[test]
    fn handles_lowercase_data_dir_name() {
        let dir = TempDir::new().unwrap();
        let install = dir.path();
        std::fs::create_dir_all(install.join("data/textures")).unwrap();
        let map = canonical_data_casing(install).unwrap();
        assert_eq!(map.data_dir_name, "data");
        assert_eq!(map.canonical_dir("textures"), Some("textures"));
    }

    #[test]
    fn errors_when_no_data_dir() {
        let dir = TempDir::new().unwrap();
        let err = canonical_data_casing(dir.path()).unwrap_err();
        assert!(matches!(err, SteamError::InvalidGameFolder { .. }));
    }

    #[test]
    fn empty_data_dir_yields_empty_map() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("Data")).unwrap();
        let map = canonical_data_casing(dir.path()).unwrap();
        assert_eq!(map.data_dir_name, "Data");
        assert!(map.is_empty());
    }

    #[test]
    fn casing_map_serializes() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("Data/Textures")).unwrap();
        let map = canonical_data_casing(dir.path()).unwrap();
        let json = serde_json::to_string(&map).unwrap();
        let back: CasingMap = serde_json::from_str(&json).unwrap();
        assert_eq!(map, back);
    }
}
