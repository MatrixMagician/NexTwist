//! conflict_redeploy (CONF-03) — BLOCKING-PRISTINE.
//!
//! The conflict slice's safety gate: deploying the user's deterministic conflict-winner
//! set must (1) place exactly ONE owner per `target_rel` (no `deployed_file` UNIQUE
//! violation — the resolver dedups before deploy, Pitfall 3) and (2) remain fully
//! reversible — a `purge` after the multi-mod deploy returns the game **byte-for-byte
//! pristine** (Pitfall 4 safety invariant), INCLUDING after a priority/rank change that
//! flips the winner and triggers a redeploy.
//!
//! These run through the UNCHANGED safe engine (`deploy_winners` reuses the same
//! journaled per-file primitive as Phase-1 `deploy`; `purge` is untouched) and assert
//! pristine via the testkit DIR_SENTINEL harness (empty-dir shape included).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use deploy::{conflict, deploy_winners, purge, redeploy_winners, ModInput, WinnerFile};
use nextwist_core::Game;
use store::Store;
use tempfile::TempDir;
use testkit::{assert_trees_identical, fake_staged_mod, snapshot_tree};

/// Harness mirroring crash_recovery.rs: a TempDir root with install/db plus per-mod
/// staging roots laid OUTSIDE `game.staging_dir` (multi-mod winners come from different
/// roots, the whole point of the conflict slice).
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

/// CONF-03: the deterministic winner set deploys with exactly one owner per path
/// (no UNIQUE violation), and deploy -> purge returns the game byte-for-byte pristine.
#[test]
fn conflict_winner_set_deploys_unique_and_pristine() {
    let fx = Fixture::new();
    let pristine = lay_vanilla(&fx);
    let game = fx.game();

    // TWO mods both provide Data/shared.esp (different bytes) + each a unique file.
    let mod_a = fx.stage_mod(
        "A",
        &[("Data/shared.esp", b"A-SHARED"), ("Data/only_a.esp", b"A-ONLY")],
    );
    let mod_b = fx.stage_mod(
        "B",
        &[("Data/shared.esp", b"B-SHARED"), ("Data/only_b.esp", b"B-ONLY")],
    );

    // mod A rank=1 (winner), mod B rank=2.
    let mods = vec![
        ModInput { mod_id: 1, staging_root: mod_a.clone(), rank: 1 },
        ModInput { mod_id: 2, staging_root: mod_b.clone(), rank: 2 },
    ];
    let (winners, conflicts) = conflict::resolve(&mods).unwrap();

    // Exactly one winner for Data/shared.esp = mod A; the conflict is reported.
    let shared: Vec<&WinnerFile> = winners
        .iter()
        .filter(|w| w.rel == std::path::Path::new("Data/shared.esp"))
        .collect();
    assert_eq!(shared.len(), 1, "exactly one winner for the contested path");
    assert_eq!(shared[0].mod_id, 1, "lower-rank mod A wins");
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].winner, 1);
    // Three unique target_rels total (shared + only_a + only_b).
    assert_eq!(winners.len(), 3);

    // Deploy the winner set through the safe engine — must NOT hit a UNIQUE violation.
    let store = fx.open_store();
    let report = deploy_winners(&store, &game, &winners).unwrap();
    assert_eq!(report.deployed, 3, "the deduped winner set deploys 3 files");

    // The winning bytes landed (mod A's shared.esp, not mod B's).
    assert_eq!(
        fs::read(fx.install.join("Data/shared.esp")).unwrap(),
        b"A-SHARED",
        "the winner's bytes are on disk"
    );
    // The manifest records exactly the deduped set with no duplicate target_rel.
    let deployed = store.list_deployed_files(game.appid).unwrap();
    assert_eq!(deployed.len(), 3);
    let mut rels: Vec<PathBuf> = deployed.iter().map(|e| e.target_rel.clone()).collect();
    rels.sort();
    rels.dedup();
    assert_eq!(rels.len(), 3, "one owner per target_rel");
    // D-03: the winning mod id is recorded for the contested file.
    let shared_entry = deployed
        .iter()
        .find(|e| e.target_rel == std::path::Path::new("Data/shared.esp"))
        .unwrap();
    assert_eq!(shared_entry.source_mod, 1, "manifest records the winning mod id (D-03)");

    // Relaunch (fresh store) then purge -> byte-for-byte pristine.
    let store = fx.open_store();
    let purged = purge(&store, &game).unwrap();
    assert!(purged.orphans.is_empty(), "purge leaves no orphans: {:?}", purged.orphans);
    let after = snapshot_tree(&fx.install).unwrap();
    assert_trees_identical(&pristine, &after);
    assert!(store.list_deployed_files(game.appid).unwrap().is_empty());
}

/// A rank change that flips the winner, redeployed after a purge, also returns pristine
/// — proving redeploy after a priority change is reversible (Pitfall 4 across a switch).
#[test]
fn rank_change_redeploy_stays_pristine() {
    let fx = Fixture::new();
    let pristine = lay_vanilla(&fx);
    let game = fx.game();

    let mod_a = fx.stage_mod(
        "A",
        &[("Data/shared.esp", b"A-SHARED"), ("Data/only_a.esp", b"A-ONLY")],
    );
    let mod_b = fx.stage_mod(
        "B",
        &[("Data/shared.esp", b"B-SHARED"), ("Data/only_b.esp", b"B-ONLY")],
    );

    // First deploy: A wins (rank 1).
    {
        let (winners, _) = conflict::resolve(&[
            ModInput { mod_id: 1, staging_root: mod_a.clone(), rank: 1 },
            ModInput { mod_id: 2, staging_root: mod_b.clone(), rank: 2 },
        ])
        .unwrap();
        let store = fx.open_store();
        deploy_winners(&store, &game, &winners).unwrap();
        assert_eq!(fs::read(fx.install.join("Data/shared.esp")).unwrap(), b"A-SHARED");
    }

    // Purge back to pristine (the switch contract: purge old, then deploy new).
    {
        let store = fx.open_store();
        purge(&store, &game).unwrap();
        let mid = snapshot_tree(&fx.install).unwrap();
        assert_trees_identical(&pristine, &mid);
    }

    // Flip ranks so B wins, resolve + redeploy.
    {
        let (winners, conflicts) = conflict::resolve(&[
            ModInput { mod_id: 1, staging_root: mod_a, rank: 2 },
            ModInput { mod_id: 2, staging_root: mod_b, rank: 1 },
        ])
        .unwrap();
        assert_eq!(conflicts[0].winner, 2, "after the flip mod B wins");
        let store = fx.open_store();
        deploy_winners(&store, &game, &winners).unwrap();
        assert_eq!(
            fs::read(fx.install.join("Data/shared.esp")).unwrap(),
            b"B-SHARED",
            "the new winner's bytes are now on disk"
        );
    }

    // Purge again -> still byte-for-byte pristine after the rank-change redeploy.
    {
        let store = fx.open_store();
        let purged = purge(&store, &game).unwrap();
        assert!(purged.orphans.is_empty(), "no orphans after redeploy purge: {:?}", purged.orphans);
        let after = snapshot_tree(&fx.install).unwrap();
        assert_trees_identical(&pristine, &after);
    }
}

/// CR-01 REGRESSION: the LIVE re-deploy path (`redeploy_winners`, used by the
/// `deploy_winner_set` command) must itself purge-to-pristine before each fresh deploy.
/// This test deploys winner set A, then redeploys a CHANGED set (rank flip + a mod
/// disabled so a path leaves the set) WITHOUT any manual purge between the deploys, then
/// purges and asserts byte-for-byte pristine. A bare `deploy_winners` re-deploy would
/// orphan the dropped path and corrupt the `pre_existing` flag, leaving the game
/// non-pristine — this guards against re-introducing that bug.
#[test]
fn redeploy_winners_reconciles_without_manual_purge() {
    let fx = Fixture::new();
    let pristine = lay_vanilla(&fx);
    let game = fx.game();

    // mod A overrides the VANILLA Data/Skyrim.esm (so pre_existing bookkeeping matters)
    // and provides shared.esp + a unique only_a.esp; mod B provides shared.esp + only_b.esp.
    let mod_a = fx.stage_mod(
        "A",
        &[
            ("Data/Skyrim.esm", b"A-MASTER-OVERRIDE"),
            ("Data/shared.esp", b"A-SHARED"),
            ("Data/only_a.esp", b"A-ONLY"),
        ],
    );
    let mod_b = fx.stage_mod(
        "B",
        &[("Data/shared.esp", b"B-SHARED"), ("Data/only_b.esp", b"B-ONLY")],
    );

    // First reconcile: BOTH mods enabled, A wins (rank 1). A's Skyrim.esm override is
    // deployed over the vanilla master (which is backed up as pre_existing).
    {
        let (winners, _) = conflict::resolve(&[
            ModInput { mod_id: 1, staging_root: mod_a.clone(), rank: 1 },
            ModInput { mod_id: 2, staging_root: mod_b.clone(), rank: 2 },
        ])
        .unwrap();
        let store = fx.open_store();
        let (_purged, deployed) = redeploy_winners(&store, &game, &winners).unwrap();
        // 4 unique paths: Skyrim.esm, shared.esp, only_a.esp, only_b.esp.
        assert_eq!(deployed.deployed, 4);
        assert_eq!(fs::read(fx.install.join("Data/Skyrim.esm")).unwrap(), b"A-MASTER-OVERRIDE");
        assert_eq!(fs::read(fx.install.join("Data/shared.esp")).unwrap(), b"A-SHARED");
    }

    // Second reconcile, NO manual purge between: mod A is DISABLED (drops Skyrim.esm,
    // only_a.esp, and its shared.esp claim), only mod B remains. A bare re-deploy would
    // leave A's Skyrim.esm override on disk over the (now mis-attributed) vanilla master
    // and orphan only_a.esp. `redeploy_winners` purges to pristine first, so only B's
    // files end up deployed and the vanilla Skyrim.esm is restored before B's deploy.
    {
        let (winners, _) = conflict::resolve(&[ModInput {
            mod_id: 2,
            staging_root: mod_b.clone(),
            rank: 1,
        }])
        .unwrap();
        let store = fx.open_store();
        let (purged, deployed) = redeploy_winners(&store, &game, &winners).unwrap();
        // The reconcile purged the 4 prior files (one a vanilla restore) then deployed B's 2.
        assert_eq!(purged.removed, 4, "the prior winner set is purged before redeploy");
        assert_eq!(purged.restored, 1, "the vanilla Skyrim.esm is restored");
        assert_eq!(deployed.deployed, 2, "only mod B's files are now deployed");
        // The vanilla master is back; A's dropped files are gone, not orphaned.
        assert_eq!(fs::read(fx.install.join("Data/Skyrim.esm")).unwrap(), b"VANILLA-MASTER");
        assert!(!fx.install.join("Data/only_a.esp").exists(), "A's unique file is not orphaned");
        assert_eq!(fs::read(fx.install.join("Data/shared.esp")).unwrap(), b"B-SHARED");
    }

    // Final purge -> byte-for-byte pristine, proving the repeated live reconcile never
    // corrupted the pristine-restore path.
    {
        let store = fx.open_store();
        let purged = purge(&store, &game).unwrap();
        assert!(purged.orphans.is_empty(), "no orphans after live reconcile: {:?}", purged.orphans);
        let after = snapshot_tree(&fx.install).unwrap();
        assert_trees_identical(&pristine, &after);
        assert!(store.list_deployed_files(game.appid).unwrap().is_empty());
    }
}
