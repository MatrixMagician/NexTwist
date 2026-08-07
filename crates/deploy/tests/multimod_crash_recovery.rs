//! An interrupted **multi-mod** deploy must still round-trip to pristine.
//!
//! `deploy_winners` is the path real users take: the conflict resolver hands the engine a
//! winner set whose files come from DIFFERENT per-mod staging roots. Unlike the
//! single-root `deploy`, it has no crash-injection seam, so no test drove a crash through
//! it — the `crash_recovery` centrepiece only ever exercised `deploy_with_abort`.
//!
//! This reconstructs the crash state directly: a durable `pending` intent recorded for a
//! winner, killed between the intent (step 1) and the vanilla backup (step 2). Against the
//! pre-fix engine the vanilla file ends up **deleted outright** with nothing to restore;
//! the fix makes recovery roll forward from the mod's own staging root and purge back to
//! the original bytes.

use std::fs;
use std::path::PathBuf;

use deploy::{ModInput, deploy_winners, journal, purge, recover_on_launch, resolve, verify};
use nextwist_core::{DeployMethod, Game, ManagedMod};
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

    /// Register a mod staged the way production stages it: under its own subdirectory.
    fn add_mod(&self, store: &Store, name: &str, files: &[(&str, &[u8])], rank: u32) -> ModInput {
        let mod_root = self.staging.join(name);
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
                    staging_root: mod_root.clone(),
                    enabled: true,
                    rank,
                },
            )
            .unwrap();
        ModInput {
            mod_id,
            staging_root: mod_root,
            rank,
        }
    }
}

/// A crash between the durable intent and the vanilla backup, on the winner-set path.
/// Recovery must complete the deploy from the winning mod's OWN staging root, and the
/// subsequent purge must restore the original bytes.
#[test]
fn interrupted_winner_set_deploy_recovers_and_purges_to_pristine() {
    let fx = Fixture::new();
    let game = fx.game();
    let store = fx.open_store();

    let vanilla = fx.install.join("Data/Skyrim.esm");
    fs::write(&vanilla, b"VANILLA-MASTER").unwrap();

    let a = fx.add_mod(&store, "ModA", &[("Data/Skyrim.esm", b"A-BYTES")], 1);

    // The crash state: intent durable, file not yet placed, vanilla not yet backed up.
    let hash = blake3::hash(&fs::read(a.staging_root.join("Data/Skyrim.esm")).unwrap())
        .to_hex()
        .to_string();
    journal::begin_deploy(
        &store,
        APPID,
        &PathBuf::from("Data/Skyrim.esm"),
        DeployMethod::Copy,
        &hash,
        &a.staging_root,
    )
    .unwrap();

    assert_eq!(
        fs::read(&vanilla).unwrap(),
        b"VANILLA-MASTER",
        "the vanilla file is untouched at the moment of the crash"
    );

    // Relaunch.
    let recovery = recover_on_launch(&store, &game).unwrap();
    assert_eq!(recovery.replayed, 1, "the interrupted op must be replayed");

    // Rolled FORWARD from the mod's own staging root, backing up the original on the way.
    assert_eq!(
        fs::read(&vanilla).unwrap(),
        b"A-BYTES",
        "recovery must complete the deploy, not silently undo it"
    );
    assert!(
        store
            .vanilla_for(APPID, &PathBuf::from("Data/Skyrim.esm"))
            .unwrap()
            .is_some(),
        "the original must have been backed up before the overwrite, or it is unrecoverable"
    );
    assert!(store.pending_ops().unwrap().is_empty());

    // The whole point: the round trip still returns the game byte-for-byte.
    purge(&store, &game).unwrap();
    assert_eq!(
        fs::read(&vanilla).unwrap(),
        b"VANILLA-MASTER",
        "purge must restore the original after an interrupted multi-mod deploy"
    );
}

/// A full winner-set deploy across two contesting mods round-trips to pristine, and the
/// losing mod's file never reaches the game tree.
#[test]
fn winner_set_deploy_round_trips_to_pristine() {
    let fx = Fixture::new();
    let game = fx.game();
    let store = fx.open_store();

    let vanilla = fx.install.join("Data/Skyrim.esm");
    fs::write(&vanilla, b"VANILLA-MASTER").unwrap();

    // Both mods contest Skyrim.esm; ModA has the lower (winning) rank.
    let a = fx.add_mod(
        &store,
        "ModA",
        &[("Data/Skyrim.esm", b"A-BYTES"), ("Data/a.esp", b"A-ESP")],
        1,
    );
    let b = fx.add_mod(
        &store,
        "ModB",
        &[("Data/Skyrim.esm", b"B-BYTES"), ("Data/b.esp", b"B-ESP")],
        2,
    );

    let (winners, conflicts) = resolve(&[a, b]).unwrap();
    assert_eq!(
        conflicts.len(),
        1,
        "the contested file must be reported to the user, not silently resolved"
    );
    deploy_winners(&store, &game, &winners).unwrap();

    assert_eq!(
        fs::read(&vanilla).unwrap(),
        b"A-BYTES",
        "the lower-ranked mod wins the contested file"
    );
    assert!(fx.install.join("Data/a.esp").is_file());
    assert!(fx.install.join("Data/b.esp").is_file());

    assert!(verify(&store, &game).unwrap().pristine);

    // Recovery on a clean deployment is a no-op that changes nothing.
    let recovery = recover_on_launch(&store, &game).unwrap();
    assert_eq!(recovery.replayed, 0);
    assert!(recovery.drift.pristine);

    let report = purge(&store, &game).unwrap();
    assert!(
        report.orphans.is_empty(),
        "a clean purge must leave nothing unexplained behind"
    );
    assert_eq!(
        fs::read(&vanilla).unwrap(),
        b"VANILLA-MASTER",
        "purge restores the vanilla original byte-for-byte"
    );
    assert!(!fx.install.join("Data/a.esp").exists());
    assert!(!fx.install.join("Data/b.esp").exists());
}
