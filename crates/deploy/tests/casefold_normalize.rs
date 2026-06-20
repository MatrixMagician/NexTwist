//! casefold_normalize (DEPLOY-08): a mixed-case mod path is rewritten so its DIRECTORY
//! components match the game's canonical `Data/` casing, while leaf-file casing and
//! mod-introduced (game-absent) directories are preserved unchanged.
//!
//! Wine/Proton does NOT abstract the filesystem: a Windows `open("Data\\Textures\\x")`
//! becomes a case-sensitive Linux `open()`. A mod authored on case-insensitive NTFS may
//! carry `TEXTURES/Foo.DDS`; if we deploy it verbatim onto a case-sensitive Linux tree
//! whose real dir is `Textures/`, the game's `open()` fails and the mod silently does
//! nothing. We rewrite each directory component to the game's real casing using the
//! canonical map produced by `steam::canonical_data_casing` (Plan 02).

use std::path::{Path, PathBuf};

use deploy::normalize_to_canonical;
use steam::canonical_data_casing;

/// Build a realistic mixed-case Bethesda `Data/` tree under a tempdir and return the
/// install dir + the derived canonical casing map.
fn fixture() -> (tempfile::TempDir, steam::CasingMap) {
    let dir = tempfile::TempDir::new().unwrap();
    let install = dir.path();
    std::fs::create_dir_all(install.join("Data/Textures/Actors")).unwrap();
    std::fs::create_dir_all(install.join("Data/Meshes")).unwrap();
    std::fs::create_dir_all(install.join("Data/Scripts")).unwrap();
    let casing = canonical_data_casing(install).unwrap();
    (dir, casing)
}

#[test]
fn mixed_case_directory_components_are_normalized_to_canonical() {
    let (_dir, casing) = fixture();

    // TEXTURES -> Textures (single component).
    assert_eq!(
        normalize_to_canonical(Path::new("TEXTURES/x.dds"), &casing),
        PathBuf::from("Textures/x.dds"),
    );

    // meshes -> Meshes.
    assert_eq!(
        normalize_to_canonical(Path::new("meshes/y.nif"), &casing),
        PathBuf::from("Meshes/y.nif"),
    );

    // Nested: TEXTURES/ACTORS -> Textures/Actors.
    assert_eq!(
        normalize_to_canonical(Path::new("TEXTURES/ACTORS/z.dds"), &casing),
        PathBuf::from("Textures/Actors/z.dds"),
    );
}

#[test]
fn leaf_filename_casing_is_preserved() {
    let (_dir, casing) = fixture();
    // The LEAF (Foo.DDS) is never lowercased — only directory components are mapped.
    assert_eq!(
        normalize_to_canonical(Path::new("TEXTURES/Foo.DDS"), &casing),
        PathBuf::from("Textures/Foo.DDS"),
    );
}

#[test]
fn already_canonical_path_is_returned_unchanged() {
    let (_dir, casing) = fixture();
    let p = Path::new("Textures/Actors/a.dds");
    assert_eq!(normalize_to_canonical(p, &casing), PathBuf::from("Textures/Actors/a.dds"));
}

#[test]
fn mod_introduced_directory_not_in_game_is_preserved() {
    let (_dir, casing) = fixture();
    // The game has no `Sound/` dir; a mod that introduces one keeps its own casing
    // (there is no canonical answer to defer to).
    assert_eq!(
        normalize_to_canonical(Path::new("Sound/fx/boom.wav"), &casing),
        PathBuf::from("Sound/fx/boom.wav"),
    );
}

#[test]
fn data_rooted_path_keeps_data_segment_and_normalizes_beneath_it() {
    let (_dir, casing) = fixture();
    // A `Data/`-rooted relpath: the leading Data segment is normalized to the canonical
    // data dir name and the remainder is mapped relative to Data/.
    assert_eq!(
        normalize_to_canonical(Path::new("DATA/TEXTURES/x.dds"), &casing),
        PathBuf::from("Data/Textures/x.dds"),
    );
}
