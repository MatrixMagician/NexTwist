//! End-to-end validation against a copy of a REAL Skyrim SE install.
//!
//! Every other test in this repo builds its game tree from synthetic fixtures. This one
//! runs the full user workflow — probe, casing map, conflict resolve, deploy, verify,
//! plugin scan, purge — over the actual 80-plugin Skyrim SE Data set (Creation Club
//! content included, with its real mixed-case filenames), then asserts the tree comes
//! back byte-for-byte.
//!
//! Skipped unless the sandbox exists, so CI and other machines are unaffected. Populate it
//! with `scripts/realtest-setup.sh`, which only ever READS the real install.

use std::fs;
use std::path::{Path, PathBuf};

use deploy::{ModInput, deploy_winners, purge, recover_on_launch, repair, resolve, verify};
use nextwist_core::{Game, ManagedMod};
use store::Store;
use tempfile::TempDir;
use testkit::{assert_trees_identical, snapshot_tree};

const APPID: u32 = 489830;

fn sandbox() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/realtest/game")
        .canonicalize()
        .ok()?;
    p.join("Data").is_dir().then_some(p)
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
fn real_skyrim_data_survives_a_full_deploy_verify_purge_cycle() {
    let Some(source) = sandbox() else {
        eprintln!("SKIP: no target/realtest/game sandbox (run scripts/realtest-setup.sh)");
        return;
    };
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("game");
    stage_game_copy(&source, &install);

    let vanilla = snapshot_tree(&install).unwrap();
    assert!(
        vanilla.len() >= 50,
        "expected a substantial real game tree, found {} files",
        vanilla.len()
    );
    eprintln!("real game tree: {} files", vanilla.len());

    let staging = tmp.path().join("staging/489830");
    fs::create_dir_all(&staging).unwrap();
    let game = Game {
        appid: APPID,
        name: "Skyrim Special Edition".into(),
        install_dir: install.clone(),
        prefix: tmp.path().join("prefix"),
        staging_dir: staging.clone(),
    };
    let store = Store::open(&tmp.path().join("nextwist.db")).unwrap();
    store.add_managed_game(&game).unwrap();

    // Two mods staged the production way. ModA overwrites a REAL game plugin (the
    // backup-before-overwrite path over genuine multi-megabyte content); both add files,
    // and they contest one path so the conflict resolver is exercised too.
    let real_plugin = vanilla
        .keys()
        .find(|p| {
            p.extension().is_some_and(|e| e.eq_ignore_ascii_case("esm"))
                && fs::metadata(install.join(p)).map(|m| m.len()).unwrap_or(0) > 1024
        })
        .expect("the real tree must contain a non-trivial .esm")
        .clone();
    eprintln!("overwriting real plugin: {}", real_plugin.display());

    let mut inputs = Vec::new();
    for (name, rank, marker) in [("ModA", 1u32, &b"MOD-A"[..]), ("ModB", 2, &b"MOD-B"[..])] {
        let mod_root = staging.join(name);
        fs::create_dir_all(mod_root.join("Data/meshes")).unwrap();
        // Contested by both mods (lower rank wins).
        fs::write(mod_root.join("Data/meshes/shared.nif"), marker).unwrap();
        // Unique to each.
        fs::write(mod_root.join(format!("Data/{name}.esp")), marker).unwrap();
        if name == "ModA" {
            // Overwrite a genuine vanilla plugin.
            fs::write(mod_root.join(&real_plugin), b"MOD-A-OVERWRITE").unwrap();
        }
        let mod_id = store
            .add_mod(
                APPID,
                &ManagedMod {
                    id: 0,
                    name: name.into(),
                    staging_root: mod_root.clone(),
                    enabled: true,
                    rank,
                },
            )
            .unwrap();
        inputs.push(ModInput {
            mod_id,
            staging_root: mod_root,
            rank,
        });
    }

    let (winners, conflicts) = resolve(&inputs).unwrap();
    assert_eq!(conflicts.len(), 1, "the contested file must be surfaced");

    let report = deploy_winners(&store, &game, &winners).unwrap();
    eprintln!(
        "deployed {} files, backed up {}, methods {:?}, warnings {:?}",
        report.deployed, report.backed_up, report.methods, report.fs_warnings
    );
    assert!(report.deployed >= 3);
    assert_eq!(
        report.backed_up, 1,
        "overwriting a real vanilla plugin must take exactly one backup"
    );

    // The winner wrote over genuine game content.
    assert_eq!(
        fs::read(install.join(&real_plugin)).unwrap(),
        b"MOD-A-OVERWRITE"
    );
    // Lower rank won the contested file.
    assert_eq!(
        fs::read(install.join("Data/meshes/shared.nif")).unwrap(),
        b"MOD-A"
    );

    assert!(
        verify(&store, &game).unwrap().pristine,
        "a fresh deploy over a real game tree must verify clean"
    );

    // Recovery over a real tree is a no-op, and repair on a clean tree changes nothing.
    let recovery = recover_on_launch(&store, &game).unwrap();
    assert_eq!(recovery.replayed, 0);
    assert!(recovery.drift.pristine);
    let repaired = repair(&store, &game).unwrap();
    assert_eq!(repaired.restored_changed + repaired.restored_missing, 0);

    // Drift a deployed file and prove repair fixes it on real data.
    let drifted = install.join("Data/ModA.esp");
    fs::remove_file(&drifted).unwrap();
    fs::write(&drifted, b"CORRUPTED").unwrap();
    assert!(!verify(&store, &game).unwrap().pristine);
    let repaired = repair(&store, &game).unwrap();
    assert_eq!(repaired.restored_changed, 1, "repair must fix real drift");
    assert!(verify(&store, &game).unwrap().pristine);

    // THE ASSERTION: purge returns the real game tree byte-for-byte.
    let purged = purge(&store, &game).unwrap();
    eprintln!(
        "purge removed {} restored {} orphans {}",
        purged.removed,
        purged.restored,
        purged.orphans.len()
    );
    assert_eq!(
        purged.restored, 1,
        "the overwritten vanilla plugin must be restored"
    );

    // The pristine oracle: blake3 over every file AND the directory shape.
    let after = snapshot_tree(&install).unwrap();
    assert_trees_identical(&vanilla, &after);
}

/// A mod shipping a wrong-case LEAF against a real vanilla file.
///
/// Mods are authored on case-insensitive NTFS, so `Data/hearthfires.esm` is a normal thing
/// to ship against a game whose real file is `Data/HearthFires.esm`. Leaves are preserved
/// verbatim (see `casefold`'s header), so on a case-sensitive Linux filesystem this lands
/// as a SECOND file instead of replacing the original.
///
/// The mod silently not taking effect is a real user-facing limitation, documented where
/// the decision lives. What this test pins is the part that must never regress: the
/// reversibility guarantee survives it. The vanilla file is untouched, no backup is taken
/// (nothing was overwritten), and purge removes only what we added.
#[test]
fn a_wrong_case_leaf_never_damages_the_real_vanilla_file() {
    let Some(source) = sandbox() else {
        eprintln!("SKIP: no target/realtest/game sandbox (run scripts/realtest-setup.sh)");
        return;
    };
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("game");
    stage_game_copy(&source, &install);
    let vanilla = snapshot_tree(&install).unwrap();

    // Find a real mixed-case plugin to shadow.
    let mixed = fs::read_dir(install.join("Data"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.chars().any(char::is_uppercase) && n.to_lowercase().ends_with(".esm"))
        .expect("a real install ships mixed-case plugins");
    eprintln!("shadowing real plugin: {mixed}");

    let staging = tmp.path().join("staging/489830");
    fs::create_dir_all(&staging).unwrap();
    let game = Game {
        appid: APPID,
        name: "Skyrim Special Edition".into(),
        install_dir: install.clone(),
        prefix: tmp.path().join("prefix"),
        staging_dir: staging.clone(),
    };
    let store = Store::open(&tmp.path().join("nextwist.db")).unwrap();
    store.add_managed_game(&game).unwrap();

    // The mod ships the all-lowercase name, and a lowercase `data/` dir for good measure.
    let mod_root = staging.join("CaseMod");
    fs::create_dir_all(mod_root.join("data")).unwrap();
    fs::write(mod_root.join("data").join(mixed.to_lowercase()), b"MODDED").unwrap();

    let mod_id = store
        .add_mod(
            APPID,
            &ManagedMod {
                id: 0,
                name: "CaseMod".into(),
                staging_root: mod_root.clone(),
                enabled: true,
                rank: 1,
            },
        )
        .unwrap();
    let (winners, _) = resolve(&[ModInput {
        mod_id,
        staging_root: mod_root,
        rank: 1,
    }])
    .unwrap();

    let report = deploy_winners(&store, &game, &winners).unwrap();

    // Normalization happens inside the engine at deploy time (the resolver hands over the
    // mod's raw relpath), so the recorded manifest path is where it is observable: the
    // lowercase `data/` DIRECTORY is rewritten to the game's real `Data/`, and only the
    // leaf keeps the mod's casing.
    let recorded = &store.list_deployed_files(APPID).unwrap()[0].target_rel;
    assert!(
        recorded.starts_with("Data"),
        "the directory component must be case-normalized, got {}",
        recorded.display()
    );
    assert_eq!(
        recorded.file_name().unwrap().to_string_lossy(),
        mixed.to_lowercase(),
        "the leaf is preserved verbatim — that is the documented limitation"
    );
    assert_eq!(
        report.backed_up, 0,
        "nothing was overwritten, so nothing may be backed up"
    );

    // The real vanilla file is untouched.
    assert_eq!(
        blake3::hash(&fs::read(install.join("Data").join(&mixed)).unwrap())
            .to_hex()
            .to_string(),
        vanilla[&PathBuf::from("Data").join(&mixed)],
        "the real vanilla plugin must be byte-identical"
    );

    // Purge removes only what we added, and the tree returns exactly.
    purge(&store, &game).unwrap();
    let after = snapshot_tree(&install).unwrap();
    assert_trees_identical(&vanilla, &after);
}
