//! round_trip_pristine (DEPLOY-01/02/03): for randomized game + mod file trees —
//! pure-adds AND overwrites of vanilla files — snapshot vanilla, deploy, purge, and
//! assert the game tree is byte-for-byte identical to the vanilla snapshot, with no
//! orphans.
//!
//! The byte-for-byte assertion is testkit's blake3 `assert_trees_identical`. The
//! phase gate re-runs this on the dev btrfs filesystem (the hardest fs case) per
//! VALIDATION.md; here it runs deterministically on a tempdir.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use deploy::{deploy, purge, StagedFiles};
use nextwist_core::Game;
use proptest::prelude::*;
use store::Store;
use testkit::{assert_trees_identical, snapshot_tree};

/// One generated file: a relative path (under Data/) and its content bytes.
#[derive(Debug, Clone)]
struct GenFile {
    rel: PathBuf,
    bytes: Vec<u8>,
}

/// Generate a `Data/`-rooted relpath from DISJOINT directory and file alphabets so a
/// single tree can never make the same name both a file and a directory (which no
/// real, extract-validated mod tree can). The small alphabets ensure vanilla and mod
/// trees overlap often, forcing real overwrites. Never emits `.`/`..`.
fn rel_path_strategy() -> impl Strategy<Value = PathBuf> {
    // Intermediate directory segments (never used as leaf filenames).
    let dir_seg = prop::sample::select(vec!["textures", "meshes", "scripts", "interface"]);
    // Leaf filenames (never used as directory segments).
    let file_seg = prop::sample::select(vec!["a.esp", "b.nif", "c.dds", "d.pex"]);
    (prop::collection::vec(dir_seg, 0..3), file_seg).prop_map(|(dirs, file)| {
        let mut p = PathBuf::from("Data");
        for d in dirs {
            p.push(d);
        }
        p.push(file);
        p
    })
}

fn file_strategy() -> impl Strategy<Value = GenFile> {
    (rel_path_strategy(), prop::collection::vec(any::<u8>(), 0..32))
        .prop_map(|(rel, bytes)| GenFile { rel, bytes })
}

/// Dedupe a generated file list by relpath (last write wins) so a single tree never
/// declares the same path twice — mirrors a real, well-formed file tree.
fn dedupe(files: Vec<GenFile>) -> Vec<GenFile> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for f in files.into_iter().rev() {
        if seen.insert(f.rel.clone()) {
            out.push(f);
        }
    }
    out.reverse();
    out
}

/// Materialize `files` under `root`.
fn write_files(root: &Path, files: &[GenFile]) {
    for f in files {
        let p = root.join(&f.rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, &f.bytes).unwrap();
    }
}

fn run_round_trip(vanilla: Vec<GenFile>, modfiles: Vec<GenFile>) {
    let vanilla = dedupe(vanilla);
    let modfiles = dedupe(modfiles);

    let root = tempfile::TempDir::new().unwrap();
    let install = root.path().join("game");
    let staging = root.path().join("app/staging/489830");
    fs::create_dir_all(install.join("Data")).unwrap();
    fs::create_dir_all(&staging).unwrap();

    // Lay down the vanilla game tree and snapshot it (the pristine baseline).
    write_files(&install, &vanilla);
    let pristine = snapshot_tree(&install).unwrap();

    let store = Store::open(&root.path().join("app/nextwist.db")).unwrap();
    let game = Game {
        appid: 489830,
        name: "Skyrim Special Edition".into(),
        install_dir: install.clone(),
        prefix: root.path().join("prefix"),
        staging_dir: staging.clone(),
    };
    store.add_managed_game(&game).unwrap();

    // Stage the mod tree (read from a separate staging root).
    let staged_root = staging.join("mod");
    write_files(&staged_root, &modfiles);
    let staged = StagedFiles {
        staging_root: staged_root,
        files: modfiles.iter().map(|f| f.rel.clone()).collect(),
    };

    // Deploy then purge; the game tree must return to byte-for-byte pristine.
    deploy(&store, &game, &staged).unwrap();
    let report = purge(&store, &game).unwrap();
    assert!(
        report.orphans.is_empty(),
        "purge must leave no orphans: {:?}",
        report.orphans
    );

    let after = snapshot_tree(&install).unwrap();
    assert_trees_identical(&pristine, &after);
    // The manifest must be empty after a full purge.
    assert!(store.list_deployed_files(game.appid).unwrap().is_empty());
    assert!(store.pending_ops().unwrap().is_empty());
}

proptest! {
    // A handful of cases keeps CI fast while still exercising overwrites + adds.
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn deploy_then_purge_is_byte_for_byte_pristine(
        vanilla in prop::collection::vec(file_strategy(), 0..8),
        modfiles in prop::collection::vec(file_strategy(), 0..8),
    ) {
        run_round_trip(vanilla, modfiles);
    }
}

#[test]
fn empty_mod_edge_case_is_pristine() {
    let vanilla = vec![GenFile {
        rel: PathBuf::from("Data/Skyrim.esm"),
        bytes: b"master".to_vec(),
    }];
    run_round_trip(vanilla, vec![]);
}

#[test]
fn all_overwrite_edge_case_is_pristine() {
    // Every mod file overwrites a vanilla file at the same path.
    let vanilla = vec![
        GenFile { rel: PathBuf::from("Data/textures/a.dds"), bytes: b"van-a".to_vec() },
        GenFile { rel: PathBuf::from("Data/meshes/b.nif"), bytes: b"van-b".to_vec() },
    ];
    let modfiles = vec![
        GenFile { rel: PathBuf::from("Data/textures/a.dds"), bytes: b"MOD-A".to_vec() },
        GenFile { rel: PathBuf::from("Data/meshes/b.nif"), bytes: b"MOD-B".to_vec() },
    ];
    run_round_trip(vanilla, modfiles);
}
