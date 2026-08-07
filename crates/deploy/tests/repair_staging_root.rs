//! `repair` must actually repair a mod deployed the way production deploys them.
//!
//! `verify` classifies drift; `repair` is what puts it right. To re-place a file it has
//! to find the staged source again, and it reconstructed that as
//! `staging_dir/<target_rel>` — the same assumption that made crash recovery
//! ineffective (see `recovery_vanilla_safety.rs`). Production stages every mod under a
//! per-mod subdirectory, so the reconstructed path does not exist and `repair` silently
//! restores nothing while still reporting success.
//!
//! The manifest already knows the answer: each `FileEntry` records the `source_mod` that
//! owns it, and `ManagedMod` carries that mod's `staging_root`.

use std::fs;
use std::path::PathBuf;

use deploy::{ModInput, deploy_winners, repair, resolve, verify};
use nextwist_core::{Game, ManagedMod};
use store::Store;
use tempfile::TempDir;

const APPID: u32 = 489830;

struct Fixture {
    root: TempDir,
    install: PathBuf,
    staging: PathBuf,
    db: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let install = root.path().join("game");
        let staging = root.path().join("app/staging/489830");
        let db = root.path().join("app/nextwist.db");
        fs::create_dir_all(install.join("Data")).unwrap();
        fs::create_dir_all(&staging).unwrap();
        Fixture {
            root,
            install,
            staging,
            db,
        }
    }

    fn game(&self) -> Game {
        Game {
            appid: APPID,
            name: "Skyrim Special Edition".into(),
            install_dir: self.install.clone(),
            prefix: self.root.path().join("prefix"),
            staging_dir: self.staging.clone(),
        }
    }

    fn open_store(&self) -> Store {
        let store = Store::open(&self.db).unwrap();
        store.add_managed_game(&self.game()).unwrap();
        store
    }
}

/// Deploy one mod staged the production way (a per-mod subdir), corrupt a deployed file,
/// then repair. The file must come back.
#[test]
fn repair_restores_a_changed_file_for_a_mod_staged_in_a_per_mod_subdir() {
    let fx = Fixture::new();
    let game = fx.game();
    let store = fx.open_store();

    // PRODUCTION layout: `staging_dir/<mod-name>/Data/...`
    let mod_root = fx.staging.join("SomeMod");
    fs::create_dir_all(mod_root.join("Data/meshes")).unwrap();
    fs::write(mod_root.join("Data/meshes/m1.nif"), b"MOD-M1").unwrap();

    let mod_id = store
        .add_mod(
            APPID,
            &ManagedMod {
                id: 0,
                name: "SomeMod".into(),
                staging_root: mod_root.clone(),
                enabled: true,
                rank: 1,
            },
        )
        .unwrap();

    let winners = resolve(&[ModInput {
        mod_id,
        staging_root: mod_root.clone(),
        rank: 1,
    }])
    .unwrap();
    deploy_winners(&store, &game, &winners.0).unwrap();

    let target = fx.install.join("Data/meshes/m1.nif");
    assert_eq!(fs::read(&target).unwrap(), b"MOD-M1");

    // Drift: something outside NexTwist corrupts the deployed file. `apply_idempotent`
    // may have hardlinked/reflinked it, so write a fresh file rather than through the
    // link, to model an external tool replacing it.
    fs::remove_file(&target).unwrap();
    fs::write(&target, b"CORRUPTED").unwrap();

    let before = verify(&store, &game).unwrap();
    assert_eq!(before.changed.len(), 1, "verify must see the drift");
    assert!(!before.pristine);

    let fixed = repair(&store, &game).unwrap();
    assert_eq!(
        fixed.restored_changed, 1,
        "repair must restore the drifted file, not silently skip it"
    );
    assert_eq!(
        fs::read(&target).unwrap(),
        b"MOD-M1",
        "the file must be back to the deployed content"
    );

    let after = verify(&store, &game).unwrap();
    assert!(
        after.pristine,
        "the deployment must verify clean after a repair"
    );
}

/// The same for a file deleted out from under us: repair re-places it.
#[test]
fn repair_restores_a_missing_file_for_a_mod_staged_in_a_per_mod_subdir() {
    let fx = Fixture::new();
    let game = fx.game();
    let store = fx.open_store();

    let mod_root = fx.staging.join("SomeMod");
    fs::create_dir_all(mod_root.join("Data")).unwrap();
    fs::write(mod_root.join("Data/a.esp"), b"MOD-A").unwrap();

    let mod_id = store
        .add_mod(
            APPID,
            &ManagedMod {
                id: 0,
                name: "SomeMod".into(),
                staging_root: mod_root.clone(),
                enabled: true,
                rank: 1,
            },
        )
        .unwrap();

    let winners = resolve(&[ModInput {
        mod_id,
        staging_root: mod_root.clone(),
        rank: 1,
    }])
    .unwrap();
    deploy_winners(&store, &game, &winners.0).unwrap();

    let target = fx.install.join("Data/a.esp");
    fs::remove_file(&target).unwrap();

    assert_eq!(verify(&store, &game).unwrap().missing.len(), 1);

    let fixed = repair(&store, &game).unwrap();
    assert_eq!(fixed.restored_missing, 1, "repair must re-place the file");
    assert_eq!(fs::read(&target).unwrap(), b"MOD-A");
    assert!(verify(&store, &game).unwrap().pristine);
}
