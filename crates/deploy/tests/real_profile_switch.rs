//! Profile switching against a copy of a REAL Skyrim SE install.
//!
//! Switching is the highest-risk everyday operation: it purges the current deployment back
//! to pristine and deploys a different mod set, so a bug leaks one profile's files into
//! another or damages the game. The existing `profile_switch.rs` proves the invariants on
//! synthetic trees; this drives the same path over the genuine 80-plugin Skyrim SE Data
//! set, where the purge has to restore a real multi-megabyte master byte-for-byte.
//!
//! Skipped unless the sandbox exists (populate with `scripts/realtest-setup.sh`, which
//! only ever READS the real install).

use std::fs;
use std::path::{Path, PathBuf};

use deploy::{purge, switch_profile, verify};
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

/// Per-run copy, so a mid-run failure can never leave the shared sandbox modified and let a
/// later run take its "vanilla" baseline from the damaged state.
fn stage_copy(src: &Path, dst: &Path) {
    fs::create_dir_all(dst.join("Data")).unwrap();
    for entry in fs::read_dir(src.join("Data")).unwrap() {
        let p = entry.unwrap().path();
        if p.is_file() {
            fs::copy(&p, dst.join("Data").join(p.file_name().unwrap())).unwrap();
        }
    }
}

/// Register a mod staged the production way and add it to a profile.
fn add_mod_to_profile(
    store: &Store,
    staging_dir: &Path,
    profile_id: i64,
    name: &str,
    files: &[(&str, &[u8])],
    rank: u32,
) -> i64 {
    let mod_root = staging_dir.join(name);
    for (rel, bytes) in files {
        let path = mod_root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
    }
    let mod_id = store
        .add_mod(
            APPID,
            &ManagedMod {
                id: 0,
                name: name.into(),
                staging_root: mod_root,
                enabled: true,
                rank,
            },
        )
        .unwrap();
    store
        .set_profile_mod(profile_id, mod_id, true, rank)
        .unwrap();
    mod_id
}

/// A -> B -> A over a real game tree. Each switch must deploy exactly its own profile's
/// files, never leak the other's, and a final purge must return the real tree byte-for-byte.
#[test]
fn switching_profiles_over_a_real_install_never_leaks_or_damages() {
    let Some(source) = sandbox() else {
        eprintln!("SKIP: no target/realtest/game sandbox (run scripts/realtest-setup.sh)");
        return;
    };
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("game");
    stage_copy(&source, &install);
    let vanilla = snapshot_tree(&install).unwrap();
    eprintln!("real game tree: {} entries", vanilla.len());

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

    // Two profiles with DIFFERENT mod sets, both overwriting the same real vanilla master
    // so each switch exercises backup-and-restore over genuine game content.
    let prof_a = store.create_profile(APPID, "Profile A").unwrap();
    let prof_b = store.create_profile(APPID, "Profile B").unwrap();

    add_mod_to_profile(
        &store,
        &staging,
        prof_a,
        "ModA",
        &[
            ("Data/Skyrim.esm", b"PROFILE-A-MASTER"),
            ("Data/OnlyInA.esp", b"A-ONLY"),
        ],
        1,
    );
    add_mod_to_profile(
        &store,
        &staging,
        prof_b,
        "ModB",
        &[
            ("Data/Skyrim.esm", b"PROFILE-B-MASTER"),
            ("Data/OnlyInB.esp", b"B-ONLY"),
        ],
        1,
    );

    // --- Switch to A ---
    switch_profile(&store, &game, prof_a).expect("switching to A must succeed");
    assert_eq!(
        fs::read(install.join("Data/Skyrim.esm")).unwrap(),
        b"PROFILE-A-MASTER"
    );
    assert!(install.join("Data/OnlyInA.esp").is_file());
    assert!(
        !install.join("Data/OnlyInB.esp").exists(),
        "profile B's file must not be present"
    );
    assert!(verify(&store, &game).unwrap().pristine);

    // --- Switch to B: A's unique file must be gone, B's present ---
    switch_profile(&store, &game, prof_b).expect("switching to B must succeed");
    assert_eq!(
        fs::read(install.join("Data/Skyrim.esm")).unwrap(),
        b"PROFILE-B-MASTER"
    );
    assert!(install.join("Data/OnlyInB.esp").is_file());
    assert!(
        !install.join("Data/OnlyInA.esp").exists(),
        "profile A's file must not leak into profile B"
    );
    assert!(verify(&store, &game).unwrap().pristine);

    // --- Back to A: the exact original deployed set returns ---
    switch_profile(&store, &game, prof_a).expect("switching back to A must succeed");
    assert_eq!(
        fs::read(install.join("Data/Skyrim.esm")).unwrap(),
        b"PROFILE-A-MASTER"
    );
    assert!(install.join("Data/OnlyInA.esp").is_file());
    assert!(!install.join("Data/OnlyInB.esp").exists());

    // --- THE ASSERTION: a final purge returns the REAL game byte-for-byte ---
    purge(&store, &game).unwrap();
    let after = snapshot_tree(&install).unwrap();
    assert_trees_identical(&vanilla, &after);
}
