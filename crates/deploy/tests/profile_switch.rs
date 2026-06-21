//! profile_switch (PROF-02/PROF-03) — BLOCKING-PRISTINE.
//!
//! The profile slice's safety gate: switching the active profile must reconcile the
//! deployment THROUGH the existing journaled safe engine — `switch_profile` does
//! `purge(old) -> resolve+deploy_winners(new profile's enabled set) -> apply_load_order
//! (new profile's plugins.txt) -> set_active(new)`. There is NO diff-deploy shortcut
//! (Pitfall 4): every switch is a full purge-to-pristine then a fresh deploy of the
//! target profile's winner set, so a profile's unique files can never leak into another
//! profile (T-02-15).
//!
//! The NON-NEGOTIABLE assertion is byte-for-byte pristine ACROSS switches: after a
//! sequence of switches A->B->A, a final purge returns the install **byte-for-byte
//! pristine** (testkit DIR_SENTINEL harness, empty-dir shape included) — the game stays
//! restorable to vanilla no matter how many times the user switches (T-02-14).
//!
//! PROF-03 is proven by asserting A->B->A reproduces profile A's EXACT deployed set
//! (each profile preserves its own membership + per-profile ranks).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use deploy::{purge, switch_profile, SwitchReport};
use nextwist_core::{Game, ManagedMod};
use store::Store;
use tempfile::TempDir;
use testkit::{assert_trees_identical, fake_staged_mod, snapshot_tree};

/// Harness mirroring conflict_redeploy.rs: a TempDir root with install/db plus per-mod
/// staging roots laid OUTSIDE `game.staging_dir` (a profile's enabled mods come from
/// independent shared staging roots).
struct Fixture {
    root: TempDir,
    install: PathBuf,
    db: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let install = root.path().join("game");
        let db = root.path().join("app/nextwist.db");
        fs::create_dir_all(install.join("Data")).unwrap();
        Fixture { root, install, db }
    }

    fn game(&self) -> Game {
        Game {
            appid: 489830,
            name: "Skyrim Special Edition".into(),
            install_dir: self.install.clone(),
            prefix: self.root.path().join("prefix"),
            staging_dir: self.root.path().join("app/staging/489830"),
        }
    }

    /// Open a fresh store handle against the same DB (simulates a relaunch) and
    /// re-register the game (idempotent), as a real launch would.
    fn open_store(&self) -> Store {
        let store = Store::open(&self.db).unwrap();
        store.add_managed_game(&self.game()).unwrap();
        store
    }

    /// Stage a mod under its OWN root (not game.staging_dir) and return that root.
    fn stage_mod(&self, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let root = self.root.path().join(format!("mods/{name}"));
        fake_staged_mod(&root, files).unwrap()
    }
}

/// Lay a vanilla install Data/ tree, snapshot it as pristine.
fn lay_vanilla(fx: &Fixture) -> BTreeMap<PathBuf, String> {
    fs::create_dir_all(fx.install.join("Data/textures")).unwrap();
    fs::write(fx.install.join("Data/Skyrim.esm"), b"VANILLA-MASTER").unwrap();
    fs::write(fx.install.join("Data/textures/rock.dds"), b"VANILLA-ROCK").unwrap();
    snapshot_tree(&fx.install).unwrap()
}

/// Register a managed mod row and return its id (the store assigns staging_root from the
/// passed value, which the profile switch reads back to build the winner set).
fn add_mod(store: &Store, appid: u32, name: &str, root: &std::path::Path, rank: u32) -> i64 {
    store
        .add_mod(
            appid,
            &ManagedMod {
                id: 0,
                name: name.into(),
                staging_root: root.to_path_buf(),
                enabled: true,
                rank,
            },
        )
        .unwrap()
}

/// PROF-02 + PROF-03 (BLOCKING-PRISTINE): switching the active profile reconciles
/// deployment through the safe engine, each profile preserves its own enabled set/ranks
/// (A->B->A reproduces A's exact deployed set), and the install stays byte-for-byte
/// reversible to pristine across the switches.
#[test]
fn profile_switch_round_trips_pristine_across_switches() {
    let fx = Fixture::new();
    let pristine = lay_vanilla(&fx);
    let game = fx.game();

    // Three shared staged mods. mod1 + mod2 contest Data/shared.esp (different bytes);
    // each also has a unique file. mod3 is profile B's exclusive mod.
    let mod1_root = fx.stage_mod(
        "mod1",
        &[("Data/shared.esp", b"M1-SHARED"), ("Data/only1.esp", b"M1-ONLY")],
    );
    let mod2_root = fx.stage_mod(
        "mod2",
        &[("Data/shared.esp", b"M2-SHARED"), ("Data/only2.esp", b"M2-ONLY")],
    );
    let mod3_root = fx.stage_mod("mod3", &[("Data/only3.esp", b"M3-ONLY")]);

    let store = fx.open_store();
    let m1 = add_mod(&store, game.appid, "mod1", &mod1_root, 1);
    let m2 = add_mod(&store, game.appid, "mod2", &mod2_root, 2);
    let m3 = add_mod(&store, game.appid, "mod3", &mod3_root, 3);

    // Profile A: mods {1, 2}, mod1 wins the shared.esp conflict (per-profile rank 1 < 2).
    let prof_a = store.create_profile(game.appid, "A").unwrap();
    store.set_profile_mod(prof_a, m1, true, 1).unwrap();
    store.set_profile_mod(prof_a, m2, true, 2).unwrap();
    // mod3 is present but NOT enabled in A.
    store.set_profile_mod(prof_a, m3, false, 3).unwrap();

    // Profile B: only mod {3} enabled — a completely different deployed set.
    let prof_b = store.create_profile(game.appid, "B").unwrap();
    store.set_profile_mod(prof_b, m1, false, 1).unwrap();
    store.set_profile_mod(prof_b, m2, false, 2).unwrap();
    store.set_profile_mod(prof_b, m3, true, 1).unwrap();

    // --- Switch to A (fresh store handle each time, simulating relaunch resilience). ---
    let report_a1: SwitchReport = {
        let store = fx.open_store();
        switch_profile(&store, &game, prof_a).unwrap()
    };
    assert_eq!(report_a1.deployed.deployed, 3, "A deploys shared + only1 + only2");
    // Profile A's winner for the contested path is mod1's bytes.
    assert_eq!(
        fs::read(fx.install.join("Data/shared.esp")).unwrap(),
        b"M1-SHARED",
        "profile A's rank-1 mod1 wins shared.esp"
    );
    assert!(fx.install.join("Data/only1.esp").is_file());
    assert!(fx.install.join("Data/only2.esp").is_file());
    assert!(!fx.install.join("Data/only3.esp").exists(), "mod3 disabled in A");
    // Active profile is now exactly A.
    {
        let store = fx.open_store();
        assert_eq!(store.active_profile(game.appid).unwrap().unwrap().id, prof_a);
    }
    // Capture A's exact deployed set (target_rel + bytes) for the PROF-03 reproduction check.
    let a_deployed_snapshot = snapshot_tree(&fx.install).unwrap();

    // --- Switch to B: purges A to pristine then deploys B's exclusive set. ---
    let report_b: SwitchReport = {
        let store = fx.open_store();
        switch_profile(&store, &game, prof_b).unwrap()
    };
    // The purge half restored A's 3 files (T-02-15: A's files do not leak into B).
    assert_eq!(report_b.purged.removed, 3, "switching to B purges A's 3 deployed files");
    assert_eq!(report_b.deployed.deployed, 1, "B deploys only mod3's single file");
    assert!(fx.install.join("Data/only3.esp").is_file(), "B's mod3 file is deployed");
    assert!(!fx.install.join("Data/shared.esp").exists() || {
        // shared.esp may exist ONLY as the vanilla original would (it does not in vanilla);
        // here vanilla has no shared.esp, so it must be gone after purging A.
        fs::read(fx.install.join("Data/shared.esp")).unwrap() != b"M1-SHARED"
    }, "A's shared.esp winner must not survive into B");
    assert!(!fx.install.join("Data/only1.esp").exists(), "A's only1.esp purged");
    assert!(!fx.install.join("Data/only2.esp").exists(), "A's only2.esp purged");
    {
        let store = fx.open_store();
        assert_eq!(store.active_profile(game.appid).unwrap().unwrap().id, prof_b);
    }

    // --- Switch back to A: must reproduce A's EXACT deployed set (PROF-03). ---
    {
        let store = fx.open_store();
        switch_profile(&store, &game, prof_a).unwrap();
    }
    let a_again = snapshot_tree(&fx.install).unwrap();
    assert_trees_identical(&a_deployed_snapshot, &a_again);
    assert_eq!(
        fs::read(fx.install.join("Data/shared.esp")).unwrap(),
        b"M1-SHARED",
        "A->B->A reproduces profile A's winner set exactly (PROF-03)"
    );

    // --- NON-NEGOTIABLE: a final purge returns the install byte-for-byte pristine. ---
    {
        let store = fx.open_store();
        let purged = purge(&store, &game).unwrap();
        assert!(purged.orphans.is_empty(), "purge leaves no orphans: {:?}", purged.orphans);
    }
    let after = snapshot_tree(&fx.install).unwrap();
    assert_trees_identical(&pristine, &after);
    {
        let store = fx.open_store();
        assert!(store.list_deployed_files(game.appid).unwrap().is_empty());
    }
}

/// PROF-02: switch_profile writes the target profile's plugins.txt at the prefix after
/// deploy (apply_load_order is called with the new profile's plugin order). We assert the
/// SwitchReport carries a plugins_txt path under the resolved prefix AppData location.
///
/// The deep libloot plugins.txt content round-trip is covered by the Plan-04 loadorder
/// tests (crates/loadorder/tests/plugins.rs); here we assert the WIRING — that a switch
/// produces a plugins.txt at the expected prefix path for the active profile's plugins.
#[test]
fn switch_writes_target_profile_plugins_txt_at_prefix() {
    let fx = Fixture::new();
    lay_vanilla(&fx);
    let game = fx.game();

    let store = fx.open_store();
    let prof = store.create_profile(game.appid, "Plugged").unwrap();
    // No enabled mods/plugins => apply_load_order writes an (empty) asterisk plugins.txt.
    let report = switch_profile(&store, &game, prof).unwrap();

    // The written plugins.txt path lives under the Proton-prefix AppData/Local/<game>.
    let expected_under = game
        .prefix
        .join("drive_c/users/steamuser/AppData/Local/Skyrim Special Edition");
    assert!(
        report.plugins_txt.starts_with(&expected_under),
        "plugins.txt {:?} must live under the prefix AppData location {:?}",
        report.plugins_txt,
        expected_under
    );
    assert!(report.plugins_txt.is_file(), "switch wrote the plugins.txt file");
}
