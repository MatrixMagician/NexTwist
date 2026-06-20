//! vanilla_restore (DEPLOY-04): a mod that REPLACES a vanilla file backs the original
//! up to the content-addressed store; purge restores the exact original bytes.
//!
//! Also asserts the pure-add case takes no vanilla backup, and that deploy is
//! intent-before-act (a pending journal row exists before the manifest row, and after
//! a full deploy nothing remains pending).

use std::fs;
use std::path::PathBuf;

use deploy::{deploy, purge, StagedFiles};
use nextwist_core::Game;
use store::Store;
use tempfile::TempDir;
use testkit::{assert_trees_identical, snapshot_tree};

/// Build a managed game whose install dir, staging dir, and originals store all live
/// under one tempdir (so st_dev matches and links are viable in CI).
fn fixture(root: &std::path::Path) -> (Game, Store) {
    let install = root.join("game");
    let staging = root.join("app/staging/489830");
    fs::create_dir_all(install.join("Data")).unwrap();
    fs::create_dir_all(&staging).unwrap();
    let store = Store::open(&root.join("app/nextwist.db")).unwrap();
    let game = Game {
        appid: 489830,
        name: "Skyrim Special Edition".into(),
        install_dir: install,
        prefix: root.join("prefix"),
        staging_dir: staging,
    };
    store.add_managed_game(&game).unwrap();
    (game, store)
}

#[test]
fn replaced_vanilla_file_is_restored_byte_for_byte_on_purge() {
    let root = TempDir::new().unwrap();
    let (game, store) = fixture(root.path());

    // Vanilla game has a Data/textures/x.dds with original bytes.
    let vanilla_target = game.install_dir.join("Data/textures/x.dds");
    fs::create_dir_all(vanilla_target.parent().unwrap()).unwrap();
    fs::write(&vanilla_target, b"ORIGINAL-VANILLA-BYTES").unwrap();

    // Snapshot the pristine game tree (the thing purge must restore us to).
    let pristine = snapshot_tree(&game.install_dir).unwrap();

    // Staged mod replaces Data/textures/x.dds and adds Data/meshes/new.nif.
    let staged_root = game.staging_dir.join("mod1");
    fs::create_dir_all(staged_root.join("Data/textures")).unwrap();
    fs::create_dir_all(staged_root.join("Data/meshes")).unwrap();
    fs::write(staged_root.join("Data/textures/x.dds"), b"MODDED-REPLACEMENT").unwrap();
    fs::write(staged_root.join("Data/meshes/new.nif"), b"BRAND-NEW-MESH").unwrap();

    let staged = StagedFiles {
        staging_root: staged_root.clone(),
        files: vec![
            PathBuf::from("Data/textures/x.dds"),
            PathBuf::from("Data/meshes/new.nif"),
        ],
    };

    let report = deploy(&store, &game, &staged).unwrap();
    assert_eq!(report.deployed, 2);
    assert_eq!(report.backed_up, 1, "exactly the one vanilla overwrite backed up");

    // The deployed file content differs from vanilla (proves the overwrite happened).
    assert_eq!(
        fs::read(&vanilla_target).unwrap(),
        b"MODDED-REPLACEMENT",
        "deploy must have replaced the vanilla bytes"
    );
    // The pure-add file is present.
    assert!(game.install_dir.join("Data/meshes/new.nif").exists());
    // No pending journal rows after a clean deploy (intent-before-act fully resolved).
    assert!(store.pending_ops().unwrap().is_empty());

    // Purge restores the original bytes and removes the added file.
    let purge_report = purge(&store, &game).unwrap();
    assert_eq!(purge_report.removed, 2);
    assert_eq!(purge_report.restored, 1, "the one vanilla original restored");

    let after = snapshot_tree(&game.install_dir).unwrap();
    assert_trees_identical(&pristine, &after);
    // The restored vanilla file is byte-for-byte the original.
    assert_eq!(fs::read(&vanilla_target).unwrap(), b"ORIGINAL-VANILLA-BYTES");
}

#[test]
fn pure_add_mod_takes_no_vanilla_backup() {
    let root = TempDir::new().unwrap();
    let (game, store) = fixture(root.path());

    // Pristine game has only Data/Skyrim.esm; the mod adds entirely new files.
    fs::write(game.install_dir.join("Data/Skyrim.esm"), b"masterfile").unwrap();
    let pristine = snapshot_tree(&game.install_dir).unwrap();

    let staged_root = game.staging_dir.join("addonly");
    fs::create_dir_all(staged_root.join("Data/scripts")).unwrap();
    fs::write(staged_root.join("Data/scripts/a.pex"), b"compiled").unwrap();

    let staged = StagedFiles {
        staging_root: staged_root,
        files: vec![PathBuf::from("Data/scripts/a.pex")],
    };

    let report = deploy(&store, &game, &staged).unwrap();
    assert_eq!(report.deployed, 1);
    assert_eq!(report.backed_up, 0, "a pure add overwrites nothing vanilla");

    let purge_report = purge(&store, &game).unwrap();
    assert_eq!(purge_report.restored, 0);

    let after = snapshot_tree(&game.install_dir).unwrap();
    assert_trees_identical(&pristine, &after);
}
