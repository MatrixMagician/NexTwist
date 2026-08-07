//! The whole user workflow, from a mod ARCHIVE to a pristine game folder.
//!
//! `download_stage.rs` covers download → extract → stage. `real_game_roundtrip.rs` covers
//! deploy → verify → purge. Nothing joined the two, so the actual sequence a user performs
//! — drop in an archive, install it, then uninstall — was never driven end to end.
//!
//! This runs it over a copy of a REAL Skyrim SE Data tree: build a zip the way a mod ships
//! (nested `Data/` layout, mixed-case paths, a file that overwrites genuine game content),
//! push it through the SAME `extract::install_archive` the app uses, register it as a
//! managed mod, deploy through the conflict resolver, verify, then purge and assert the
//! real tree is byte-for-byte what it was.
//!
//! Skipped unless the sandbox exists (populate with `scripts/realtest-setup.sh`, which only
//! ever READS the real install).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use deploy::{ModInput, deploy_winners, purge, resolve, verify};
use nextwist_core::{Game, ManagedMod};
use store::Store;
use tempfile::TempDir;
use testkit::{assert_trees_identical, snapshot_tree};
use zip::write::SimpleFileOptions;

const APPID: u32 = 489830;

fn sandbox() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target/realtest/game")
        .canonicalize()
        .ok()?;
    p.join("Data").is_dir().then_some(p)
}

/// A zip shaped like a real Nexus mod: a top-level `Data/` folder with mixed-case paths.
fn build_mod_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default();
    for (name, bytes) in entries {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

/// Copy the read-only sandbox into a per-run temp dir.
///
/// The test deploys into and purges from this tree, so it must never operate on the shared
/// sandbox directly: a failure mid-run would leave that tree modified, and the NEXT run
/// would take its "vanilla" baseline from the damaged state and pass against it. Copying
/// per run makes the baseline unforgeable and the test idempotent.
fn stage_game_copy(src: &Path, dst: &Path) {
    for entry in walkdir_files(src) {
        let rel = entry.strip_prefix(src).unwrap();
        let target = dst.join(rel);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(&entry, &target).unwrap();
    }
}

fn walkdir_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

#[test]
fn archive_to_installed_to_uninstalled_leaves_the_real_game_pristine() {
    let Some(source) = sandbox() else {
        eprintln!("SKIP: no target/realtest/game sandbox (run scripts/realtest-setup.sh)");
        return;
    };
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("game");
    stage_game_copy(&source, &install);

    let vanilla = snapshot_tree(&install).unwrap();
    eprintln!("real game tree: {} files", vanilla.len());

    let staging_dir = tmp.path().join("staging/489830");
    fs::create_dir_all(&staging_dir).unwrap();

    let game = Game {
        appid: APPID,
        name: "Skyrim Special Edition".into(),
        install_dir: install.clone(),
        prefix: tmp.path().join("prefix"),
        staging_dir: staging_dir.clone(),
    };
    let store = Store::open(&tmp.path().join("nextwist.db")).unwrap();
    store.add_managed_game(&game).unwrap();

    // A mod that overwrites a genuine game plugin AND adds files, with the mixed-case
    // paths Wine has to fold.
    let real_plugin = vanilla
        .keys()
        .find(|p| p.to_string_lossy().eq_ignore_ascii_case("Data/Skyrim.esm"))
        .expect("a real Skyrim install has Skyrim.esm")
        .clone();

    let archive = tmp.path().join("CoolMod-1.0.zip");
    build_mod_zip(
        &archive,
        &[
            ("Data/Skyrim.esm", b"MODDED-MASTER"),
            ("Data/CoolMod.esp", b"COOL"),
            ("Data/Textures/Armor/cool.dds", b"COOL-TEXTURE"),
            ("Data/Meshes/cool.nif", b"COOL-MESH"),
        ],
    );

    // The SAME validated extractor the app's install paths use, into a per-mod staging
    // subdir — exactly the production layout.
    let staging_root = staging_dir.join(extract::staging_dir_name("Cool Mod", "nexus-mod"));
    let staged = extract::install_archive(&archive, &staging_root).expect("extract must succeed");
    eprintln!(
        "staged {} files at {}",
        staged.files.len(),
        staged.staging_root.display()
    );
    assert_eq!(staged.files.len(), 4);

    let mod_id = store
        .add_mod(
            APPID,
            &ManagedMod {
                id: 0,
                name: "Cool Mod".into(),
                staging_root: staged.staging_root.clone(),
                enabled: true,
                rank: 1,
            },
        )
        .unwrap();

    let (winners, conflicts) = resolve(&[ModInput {
        mod_id,
        staging_root: staged.staging_root.clone(),
        rank: 1,
    }])
    .unwrap();
    assert!(conflicts.is_empty(), "a single mod contests nothing");

    let report = deploy_winners(&store, &game, &winners).unwrap();
    eprintln!(
        "deployed {} backed_up {} warnings {:?}",
        report.deployed, report.backed_up, report.fs_warnings
    );
    assert_eq!(report.deployed, 4);
    assert_eq!(
        report.backed_up, 1,
        "exactly the one real plugin the mod overwrites"
    );

    // The mod is live over real game content.
    assert_eq!(
        fs::read(install.join(&real_plugin)).unwrap(),
        b"MODDED-MASTER"
    );
    assert!(verify(&store, &game).unwrap().pristine);

    // Uninstall.
    let purged = purge(&store, &game).unwrap();
    eprintln!(
        "purge removed {} restored {} orphans {}",
        purged.removed,
        purged.restored,
        purged.orphans.len()
    );
    assert_eq!(purged.restored, 1, "the real plugin must be restored");

    // The pristine oracle: blake3 over every file, compared whole-tree.
    let after = snapshot_tree(&install).unwrap();
    assert_trees_identical(&vanilla, &after);
    // And the mod's added directories are gone, not left as empty litter.
    assert!(!install.join("Data/Textures/Armor").exists());
    assert!(!install.join("Data/Meshes").exists());
}
