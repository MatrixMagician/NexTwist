//! collection_round_trip (COLL-04 / COLL-05) — BLOCKING-PRISTINE.
//!
//! The headline Collection safety gate, proven WITHOUT any network: a Collection deploys as
//! a dedicated profile through the EXISTING `switch_profile` path (COLL-04), and uninstalling
//! it — `purge` to pristine → drop the profile (after clearing its active flag, since
//! `delete_profile` rejects an active profile) → remove the staged trees — leaves the game
//! **byte-for-byte vanilla** (COLL-05). This is the exact orchestration
//! `src-tauri/src/commands/collections.rs::{deploy_collection, uninstall_collection}` runs,
//! exercised here directly against the headless engine + store with the testkit blake3
//! DIR_SENTINEL pristine harness — no Tauri, no live Premium account, no download.
//!
//! The live end-to-end (real Premium account → real Collection archive download → deploy →
//! in-game launch → uninstall) remains a manual UAT item (the Plan checkpoint / NEXUS-01
//! live-account gate). The reversibility CONTRACT it would visually confirm is regression-
//! locked here so a refactor can never silently break it.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use deploy::{purge, switch_profile};
use nextwist_core::{Collection, CollectionMod, Game, ManagedMod};
use store::Store;
use tempfile::TempDir;
use testkit::{assert_trees_identical, fake_staged_mod, snapshot_tree};

/// Harness mirroring profile_switch.rs: a TempDir root with install/db + per-mod staging
/// roots laid OUTSIDE `game.staging_dir` (a collection's mods are independent shared trees).
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

    fn open_store(&self) -> Store {
        let store = Store::open(&self.db).unwrap();
        store.add_managed_game(&self.game()).unwrap();
        store
    }

    /// Stage a collection mod under its OWN root (as a real download would) and return it.
    fn stage_mod(&self, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let root = self.root.path().join(format!("mods/{name}"));
        fake_staged_mod(&root, files).unwrap()
    }
}

fn lay_vanilla(fx: &Fixture) -> BTreeMap<PathBuf, String> {
    fs::create_dir_all(fx.install.join("Data/meshes")).unwrap();
    fs::write(fx.install.join("Data/Skyrim.esm"), b"VANILLA-MASTER").unwrap();
    fs::write(fx.install.join("Data/meshes/tree.nif"), b"VANILLA-TREE").unwrap();
    snapshot_tree(&fx.install).unwrap()
}

fn add_mod(store: &Store, appid: u32, name: &str, root: &std::path::Path, rank: u32) -> i64 {
    store
        .add_mod(
            appid,
            &ManagedMod {
                id: 0,
                name: name.into(),
                staging_root: root.to_path_buf(),
                enabled: false,
                rank,
            },
        )
        .unwrap()
}

/// COLL-04 + COLL-05 (BLOCKING-PRISTINE): a Collection deploys as a dedicated profile via
/// `switch_profile`, the modded files land with the rule-derived ranks deciding conflicts,
/// and uninstalling it (purge → delete_profile → remove staged trees) returns the install
/// byte-for-byte pristine.
#[test]
fn collection_install_deploy_uninstall_round_trips_pristine() {
    let fx = Fixture::new();
    let pristine = lay_vanilla(&fx);
    let game = fx.game();

    // Two collection mods. modA + modB contest Data/shared.esp (different bytes); modA's
    // rule-derived rank wins (lower rank number = higher priority). Each has a unique file.
    let mod_a_root = fx.stage_mod(
        "modA",
        &[("Data/shared.esp", b"A-SHARED"), ("Data/onlyA.esp", b"A-ONLY")],
    );
    let mod_b_root = fx.stage_mod(
        "modB",
        &[("Data/shared.esp", b"B-SHARED"), ("Data/onlyB.esp", b"B-ONLY")],
    );

    let store = fx.open_store();
    let m_a = add_mod(&store, game.appid, "modA", &mod_a_root, 1);
    let m_b = add_mod(&store, game.appid, "modB", &mod_b_root, 2);

    // ── Persist the Collection + its mods (V5 facade), as download_collection would. ──
    let collection_id = store
        .add_collection(&Collection {
            id: 0,
            appid: game.appid,
            slug: "test-collection".into(),
            revision: 1,
            name: "Test Collection".into(),
            profile_id: None,
        })
        .unwrap();
    // Ranks: modA rank 1 (wins shared.esp), modB rank 2 — exactly what map_rules_to_ranks
    // would yield for a "modB loads after modA" rule.
    store
        .add_collection_mod(
            collection_id,
            &CollectionMod {
                mod_id: m_a,
                nexus_mod_id: 100,
                file_id: 1000,
                md5: None,
                phase: 0,
                rank: 1,
                choices_json: None,
            },
        )
        .unwrap();
    store
        .add_collection_mod(
            collection_id,
            &CollectionMod {
                mod_id: m_b,
                nexus_mod_id: 200,
                file_id: 2000,
                md5: None,
                phase: 0,
                rank: 2,
                choices_json: None,
            },
        )
        .unwrap();

    // ── DEPLOY (COLL-04): create the dedicated profile, set membership by stored rank,
    //    deploy via the SAME switch_profile path (no new primitive). ──
    let profile_id = store.create_profile(game.appid, "Collection: Test Collection").unwrap();
    let cmods = store.list_collection_mods(collection_id).unwrap();
    assert_eq!(cmods.len(), 2, "both collection mods persisted");
    for cm in &cmods {
        store.set_profile_mod(profile_id, cm.mod_id, true, cm.rank).unwrap();
    }
    let report = switch_profile(&store, &game, profile_id).unwrap();
    assert_eq!(report.deployed.deployed, 3, "deploys shared + onlyA + onlyB");

    // The rule-ranked winner (rank-1 modA) owns the contested path; both unique files land.
    assert_eq!(
        fs::read(fx.install.join("Data/shared.esp")).unwrap(),
        b"A-SHARED",
        "the rank-1 collection mod (modA) wins the shared.esp conflict (Pattern 7)"
    );
    assert!(fx.install.join("Data/onlyA.esp").is_file());
    assert!(fx.install.join("Data/onlyB.esp").is_file());
    assert_eq!(
        store.active_profile(game.appid).unwrap().unwrap().id,
        profile_id,
        "the collection profile is now active"
    );

    // ── UNINSTALL (COLL-05): purge → clear active flag → delete_profile → drop rows. ──
    // 1. Purge to pristine (the deployment is restored byte-for-byte vanilla).
    let purged = purge(&store, &game).unwrap();
    assert!(purged.orphans.is_empty(), "purge leaves no orphans: {:?}", purged.orphans);

    // 2. delete_profile REJECTS an active profile — clear the (now-pristine) active flag
    //    first, exactly as uninstall_collection does. This is the ordering CONTRACT.
    assert!(
        store.delete_profile(profile_id).is_err(),
        "delete_profile must reject the still-active collection profile (CR-02)"
    );
    store.clear_active_profile(game.appid).unwrap();
    assert!(
        store.delete_profile(profile_id).unwrap(),
        "after clearing active, the collection profile deletes cleanly"
    );

    // 3. Remove the staged trees + managed_mod rows + the V5 collection rows.
    for cm in &cmods {
        if let Some(m) = store.list_mods(game.appid).unwrap().into_iter().find(|m| m.id == cm.mod_id) {
            let _ = fs::remove_dir_all(&m.staging_root);
        }
        store.remove_mod(cm.mod_id).unwrap();
    }
    assert!(store.remove_collection(collection_id).unwrap(), "collection rows removed");

    // ── NON-NEGOTIABLE: the install is byte-for-byte pristine after uninstall (COLL-05). ──
    let after = snapshot_tree(&fx.install).unwrap();
    assert_trees_identical(&pristine, &after);
    assert!(
        store.list_deployed_files(game.appid).unwrap().is_empty(),
        "no deployed-file rows remain after uninstall"
    );
    // The V5 rows CASCADE-cleared with the collection (no orphan collection_mod rows).
    assert!(
        store.get_collection(collection_id).unwrap().is_none(),
        "the collection row is gone"
    );
    assert!(
        store.list_collection_mods(collection_id).unwrap().is_empty(),
        "collection_mod rows CASCADE-deleted with the collection"
    );
}
