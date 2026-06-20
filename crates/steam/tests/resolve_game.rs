//! Integration test (ENV-01/02/03): resolve Skyrim SE / Fallout 4 from a SYNTHETIC
//! Steam-layout fixture built under a tempdir. No real Steam install is needed, so
//! this runs deterministically in CI.
//!
//! The fixture mirrors a real Steam library:
//!   <root>/steamapps/libraryfolders.vdf
//!   <root>/steamapps/appmanifest_489830.acf            (installdir = "Skyrim Special Edition")
//!   <root>/steamapps/common/Skyrim Special Edition/Data/...
//!   <root>/steamapps/compatdata/489830/pfx/
//!
//! Resolution is driven through the public `resolve_from_root` test seam, which the
//! production `resolve_game` delegates equivalent logic to — so the seam exercises the
//! same install-dir + prefix derivation without touching the host's Steam install.

use std::path::Path;

use steam::{FALLOUT4, SKYRIM_SE, canonical_data_casing, resolve_from_root};
use tempfile::TempDir;

/// Build a synthetic Steam library root for `appid` with the given install dir name,
/// optionally creating the Proton prefix. Returns the held TempDir (keep it alive).
fn build_synthetic_library(
    appid: u32,
    installdir: &str,
    name: &str,
    with_prefix: bool,
    data_subdirs: &[&str],
) -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    let steamapps = root.join("steamapps");

    // libraryfolders.vdf pointing this library root at itself (path = <root>).
    let library_folders = format!(
        "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\"{}\"\n\t\t\"apps\"\n\t\t{{\n\t\t\t\"{appid}\"\t\"0\"\n\t\t}}\n\t}}\n}}\n",
        root.display()
    );
    std::fs::create_dir_all(&steamapps).unwrap();
    std::fs::write(steamapps.join("libraryfolders.vdf"), library_folders).unwrap();

    // appmanifest_<appid>.acf with the installdir field resolve reads.
    let acf = format!(
        "\"AppState\"\n{{\n\t\"appid\"\t\"{appid}\"\n\t\"name\"\t\"{name}\"\n\t\"installdir\"\t\"{installdir}\"\n}}\n"
    );
    std::fs::write(steamapps.join(format!("appmanifest_{appid}.acf")), acf).unwrap();

    // The matching install tree with a (possibly mixed-case) Data/ subtree.
    let game_root = steamapps.join("common").join(installdir);
    std::fs::create_dir_all(game_root.join("Data")).unwrap();
    for sub in data_subdirs {
        std::fs::create_dir_all(game_root.join("Data").join(sub)).unwrap();
    }

    if with_prefix {
        std::fs::create_dir_all(
            steamapps
                .join("compatdata")
                .join(appid.to_string())
                .join("pfx"),
        )
        .unwrap();
    }

    dir
}

#[test]
fn resolves_skyrim_se_from_synthetic_fixture() {
    let dir = build_synthetic_library(
        SKYRIM_SE,
        "Skyrim Special Edition",
        "The Elder Scrolls V: Skyrim Special Edition",
        true,
        &["Textures", "Meshes", "Scripts"],
    );
    let root = dir.path();

    let resolved = resolve_from_root(root, SKYRIM_SE).expect("resolve skyrim");

    assert_eq!(resolved.appid, SKYRIM_SE);
    assert_eq!(
        resolved.install_dir,
        root.join("steamapps/common/Skyrim Special Edition")
    );
    assert_eq!(
        resolved.prefix,
        root.join("steamapps/compatdata/489830/pfx")
    );
    assert!(
        resolved.prefix_exists,
        "the fixture created the pfx dir, so it must be reported as existing"
    );

    // The resolved install_dir feeds the canonical Data/ casing map (DEPLOY-08 input).
    let casing = canonical_data_casing(&resolved.install_dir).expect("casing map");
    assert_eq!(casing.canonical_dir("textures"), Some("Textures"));
    assert_eq!(casing.canonical_dir("meshes"), Some("Meshes"));
    assert_eq!(casing.canonical_dir("scripts"), Some("Scripts"));
}

#[test]
fn resolves_fallout4_from_synthetic_fixture() {
    let dir = build_synthetic_library(FALLOUT4, "Fallout 4", "Fallout 4", true, &["Textures"]);
    let root = dir.path();

    let resolved = resolve_from_root(root, FALLOUT4).expect("resolve fallout4");
    assert_eq!(resolved.appid, FALLOUT4);
    assert_eq!(resolved.install_dir, root.join("steamapps/common/Fallout 4"));
    assert_eq!(
        resolved.prefix,
        root.join("steamapps/compatdata/377160/pfx")
    );
    assert!(resolved.prefix_exists);
}

#[test]
fn missing_prefix_is_resolved_but_flagged_absent() {
    // Game installed but Proton prefix not yet created (never launched).
    let dir = build_synthetic_library(SKYRIM_SE, "Skyrim Special Edition", "Skyrim", false, &[]);
    let root: &Path = dir.path();

    let resolved = resolve_from_root(root, SKYRIM_SE).expect("resolve");
    assert_eq!(
        resolved.prefix,
        root.join("steamapps/compatdata/489830/pfx")
    );
    assert!(
        !resolved.prefix_exists,
        "prefix path is derived but does not exist yet — caller surfaces a warning"
    );
}
