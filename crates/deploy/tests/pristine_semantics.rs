//! `pristine` must describe OUR deployment, not the user's whole game folder.
//!
//! The orphan walk reports every file under the deploy root that the manifest does not
//! claim, and its own module header says that set includes "the untouched vanilla tree".
//! That is the right thing to *report*: a purge must never delete a file it cannot explain,
//! so surfacing them is the safety behaviour.
//!
//! Folding them into `pristine` is a different question, and it was wrong. A freshly
//! managed, never-deployed-to game has thousands of vanilla files and zero deployed ones,
//! so `pristine` came back false and the UI announced "Drift detected" with a five-figure
//! orphan count — on a game nothing had touched. That trains the user to ignore the one
//! signal that is supposed to mean something.
//!
//! `pristine` answers "is our deployment consistent with the manifest": missing, changed,
//! and INI drift. Orphans stay in the report, undiminished, for the user to inspect.

use std::fs;

use deploy::{StagedFiles, deploy, purge, verify};
use nextwist_core::Game;
use store::Store;
use tempfile::TempDir;

const APPID: u32 = 489830;

struct Fixture {
    root: TempDir,
    install: std::path::PathBuf,
    staging: std::path::PathBuf,
    db: std::path::PathBuf,
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

/// A game the user just added and has never deployed to is pristine by definition: we have
/// not touched it. Its vanilla files are still reported as orphans for inspection.
#[test]
fn a_freshly_added_game_with_no_deployment_is_pristine() {
    let fx = Fixture::new();
    let game = fx.game();
    let store = fx.open_store();

    fs::create_dir_all(fx.install.join("Data/textures")).unwrap();
    fs::write(fx.install.join("Data/Skyrim.esm"), b"VANILLA-MASTER").unwrap();
    fs::write(fx.install.join("Data/Update.esm"), b"VANILLA-UPDATE").unwrap();
    fs::write(fx.install.join("Data/textures/rock.dds"), b"VANILLA-ROCK").unwrap();

    let report = verify(&store, &game).unwrap();

    assert!(
        report.pristine,
        "an untouched game must not report drift — we have deployed nothing to it"
    );
    assert_eq!(
        report.orphans.len(),
        3,
        "the vanilla files are still REPORTED, so a purge never deletes what it cannot explain"
    );
    assert!(report.missing.is_empty());
    assert!(report.changed.is_empty());
}

/// Real drift in OUR files still breaks pristine — the signal must keep its meaning.
#[test]
fn drift_in_a_deployed_file_still_breaks_pristine() {
    let fx = Fixture::new();
    let game = fx.game();
    let store = fx.open_store();

    fs::write(fx.install.join("Data/Skyrim.esm"), b"VANILLA-MASTER").unwrap();

    let mod_root = fx.staging.join("SomeMod");
    fs::create_dir_all(mod_root.join("Data")).unwrap();
    fs::write(mod_root.join("Data/a.esp"), b"MOD-A").unwrap();
    let staged = StagedFiles {
        staging_root: mod_root,
        files: vec![std::path::PathBuf::from("Data/a.esp")],
    };
    deploy(&store, &game, &staged).unwrap();

    assert!(
        verify(&store, &game).unwrap().pristine,
        "a clean deployment beside vanilla files is pristine"
    );

    // Corrupt OUR file.
    let target = fx.install.join("Data/a.esp");
    fs::remove_file(&target).unwrap();
    fs::write(&target, b"CORRUPTED").unwrap();

    let report = verify(&store, &game).unwrap();
    assert!(
        !report.pristine,
        "drift in a deployed file must be surfaced"
    );
    assert_eq!(report.changed.len(), 1);

    // And a deleted deployed file too.
    fs::remove_file(&target).unwrap();
    let report = verify(&store, &game).unwrap();
    assert!(!report.pristine);
    assert_eq!(report.missing.len(), 1);
}

/// The purge round trip is unaffected: after a purge the game is pristine again, and the
/// vanilla files it never owned are still there and still reported.
#[test]
fn purge_leaves_the_game_pristine_with_vanilla_files_reported() {
    let fx = Fixture::new();
    let game = fx.game();
    let store = fx.open_store();

    fs::write(fx.install.join("Data/Skyrim.esm"), b"VANILLA-MASTER").unwrap();

    let mod_root = fx.staging.join("SomeMod");
    fs::create_dir_all(mod_root.join("Data")).unwrap();
    fs::write(mod_root.join("Data/a.esp"), b"MOD-A").unwrap();
    deploy(
        &store,
        &game,
        &StagedFiles {
            staging_root: mod_root,
            files: vec![std::path::PathBuf::from("Data/a.esp")],
        },
    )
    .unwrap();

    purge(&store, &game).unwrap();

    let report = verify(&store, &game).unwrap();
    assert!(report.pristine, "a purged game is pristine");
    assert_eq!(
        fs::read(fx.install.join("Data/Skyrim.esm")).unwrap(),
        b"VANILLA-MASTER",
        "the vanilla file the purge could not explain is untouched"
    );
    assert_eq!(
        report.orphans.len(),
        1,
        "and it is still reported rather than silently ignored"
    );
}

/// A vanilla game that ships an EMPTY directory (Bethesda titles do — `Data/Video`) must
/// not read as drift either. This one is easy to miss: it lands in `orphan_dirs` rather
/// than `orphans`, so excluding only the file set would still cry wolf on these games.
#[test]
fn an_empty_vanilla_directory_is_not_drift() {
    let fx = Fixture::new();
    let game = fx.game();
    let store = fx.open_store();

    fs::create_dir_all(fx.install.join("Data/Video")).unwrap();
    fs::write(fx.install.join("Data/Skyrim.esm"), b"VANILLA-MASTER").unwrap();

    let report = verify(&store, &game).unwrap();

    assert!(
        report.pristine,
        "an empty directory shipped by the game is not our drift"
    );
    assert_eq!(
        report.orphan_dirs.len(),
        1,
        "it is still reported, so the user can see what NexTwist does not manage"
    );
}
