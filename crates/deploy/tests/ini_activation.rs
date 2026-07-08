//! ini_activation (SFINI-04/05) — the reversibility suite for StarfieldCustom.ini
//! loose-file activation, driven ENTIRELY through the public engine choke points
//! (`deploy` / `deploy_winners` / `redeploy_winners` / `purge` / `recover_on_launch`)
//! so it proves the WIRING, not just the `gameconfig` module.
//!
//! Modelled on `vanilla_restore.rs` + `crash_recovery.rs`: a Starfield fixture (appid
//! 1716740, a fake `My Games/Starfield` prefix, install/staging on one tempdir), then
//! `snapshot_tree(&game.prefix)` before/after for the byte-for-byte pristine assertions
//! across BOTH provenance branches (PreExisting bytes / CreatedByNexTwist absence+prune).
//!
//! RED-first: until the `is_starfield`-gated INI hooks are wired at the deploy/purge tails
//! (Task 2), the reversibility assertions FAIL while compiling cleanly.

use std::fs;
use std::path::{Path, PathBuf};

use deploy::{
    deploy, journal, preview_ini_activation, purge, recover_on_launch, redeploy_winners, repair,
    verify, StagedFiles, WinnerFile, INI_FILENAME,
};
use nextwist_core::Game;
use store::Store;
use tempfile::TempDir;
use testkit::{assert_trees_identical, fake_my_games_prefix, snapshot_tree, MyGamesOpts};

const STARFIELD: u32 = 1716740;
const SKYRIM_SE: u32 = 489830;

/// UTF-8 byte-order mark used by the CRLF+BOM byte-fidelity fixtures.
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// The exact bytes a fresh (CreatedByNexTwist) activation writes: `[Archive]` + the two
/// owned keys, CRLF, no BOM. The idempotency baseline.
const FRESH_INI: &[u8] = b"[Archive]\r\nbInvalidateOlderFiles=1\r\nsResourceDataDirsFinal=\r\n";

/// The on-disk INI target inside the prefix (matches the engine's own resolution).
fn ini_path(game: &Game) -> PathBuf {
    steam::my_games_path(&game.prefix).join(INI_FILENAME)
}

/// Count `[Archive]` section headers (case-insensitive) — the idempotency oracle.
fn archive_count(bytes: &[u8]) -> usize {
    String::from_utf8_lossy(bytes)
        .to_ascii_lowercase()
        .matches("[archive]")
        .count()
}

/// A managed game (given appid) whose install `Data/` + staging live on one tempdir and
/// whose `prefix` is the supplied Proton-prefix tree.
fn game_with_prefix(root: &Path, appid: u32, prefix: PathBuf) -> (Store, Game) {
    let install = root.join("install");
    let staging = root.join("staging");
    fs::create_dir_all(install.join("Data")).unwrap();
    fs::create_dir_all(&staging).unwrap();
    let store = Store::open(&root.join("d.db")).unwrap();
    let game = Game {
        appid,
        name: "Game".into(),
        install_dir: install,
        prefix,
        staging_dir: staging,
    };
    store.add_managed_game(&game).unwrap();
    (store, game)
}

/// A Starfield fixture whose `My Games/Starfield` subtree is shaped by `opts`.
fn sf_fixture(root: &Path, opts: MyGamesOpts<'_>) -> (Store, Game) {
    let prefix = fake_my_games_prefix(&root.join("prefix"), "Starfield", opts).unwrap();
    game_with_prefix(root, STARFIELD, prefix)
}

/// Stage a single loose mod file under its own root and deploy it through the single-root
/// public `deploy` (which, for Starfield, auto-activates the INI at its tail).
fn deploy_loose_mod(store: &Store, game: &Game) {
    let modroot = game.staging_dir.join("mod1");
    fs::create_dir_all(modroot.join("Data/meshes")).unwrap();
    fs::write(modroot.join("Data/meshes/x.nif"), b"MESH-BYTES").unwrap();
    let staged = StagedFiles {
        staging_root: modroot,
        files: vec![PathBuf::from("Data/meshes/x.nif")],
    };
    deploy(store, game, &staged).unwrap();
}

/// The same loose mod as a winner set (for the `deploy_winners` / `redeploy_winners` path).
fn loose_winners(game: &Game) -> Vec<WinnerFile> {
    let modroot = game.staging_dir.join("mod1");
    fs::create_dir_all(modroot.join("Data/meshes")).unwrap();
    fs::write(modroot.join("Data/meshes/x.nif"), b"MESH-BYTES").unwrap();
    vec![WinnerFile {
        mod_id: 1,
        staging_root: modroot,
        rel: PathBuf::from("Data/meshes/x.nif"),
    }]
}

/// Seed a pre-existing INI (creating the `My Games/Starfield` dir chain as needed).
fn seed_ini(game: &Game, bytes: &[u8]) {
    let ini = ini_path(game);
    fs::create_dir_all(ini.parent().unwrap()).unwrap();
    fs::write(&ini, bytes).unwrap();
}

// ---------------------------------------------------------------------------
// SFINI-01 — preview reports intent without writing.
// ---------------------------------------------------------------------------

#[test]
fn ini_preview_reports_intent_without_writing() {
    let dir = TempDir::new().unwrap();
    let (_store, game) = sf_fixture(dir.path(), MyGamesOpts::default());
    let p = preview_ini_activation(&game).unwrap();
    assert!(p.will_create && !p.will_edit, "absent INI → will_create");
    assert_eq!(
        p.lines,
        vec![
            "bInvalidateOlderFiles=1".to_string(),
            "sResourceDataDirsFinal=".to_string()
        ]
    );
    assert!(p.conflict.is_none());
    assert!(!ini_path(&game).exists(), "preview must not write the INI");
}

// ---------------------------------------------------------------------------
// SFINI-02a — PreExisting INI restored byte-for-byte on purge.
// ---------------------------------------------------------------------------

#[test]
fn ini_restore_preexisting_byte_for_byte() {
    let dir = TempDir::new().unwrap();
    let (store, game) = sf_fixture(dir.path(), MyGamesOpts::default());

    // A pre-existing CRLF + BOM INI with unrelated content and an [Archive] section that
    // does NOT yet carry our bInvalidateOlderFiles key (so activation must edit it).
    let mut original = BOM.to_vec();
    original.extend_from_slice(
        b"; user config\r\n[Display]\r\niSize=1080\r\n[Archive]\r\nsResourceDataDirsFinal=\r\n",
    );
    seed_ini(&game, &original);

    let pristine = snapshot_tree(&game.prefix).unwrap();

    deploy_loose_mod(&store, &game); // Starfield deploy auto-activates the INI.
    assert_ne!(
        fs::read(ini_path(&game)).unwrap(),
        original,
        "activation must have edited the pre-existing INI"
    );

    purge(&store, &game).unwrap();

    let after = snapshot_tree(&game.prefix).unwrap();
    assert_trees_identical(&pristine, &after);
    assert_eq!(
        fs::read(ini_path(&game)).unwrap(),
        original,
        "PreExisting INI restored byte-for-byte on purge"
    );
    assert!(store.pending_ops().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// SFINI-02b — CreatedByNexTwist INI: purge deletes the file AND prunes created dirs.
// ---------------------------------------------------------------------------

#[test]
fn ini_restore_absence_prunes_created_dirs() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // A prefix with Documents present but NO `My Games/Starfield` — so NexTwist genuinely
    // creates the config dir chain + INI, and restore must prune exactly what it created
    // while leaving the pre-existing Documents boundary untouched.
    let prefix = root.join("prefix");
    fs::create_dir_all(prefix.join("drive_c/users/steamuser/Documents")).unwrap();
    let (store, game) = game_with_prefix(root, STARFIELD, prefix);

    let pristine = snapshot_tree(&game.prefix).unwrap();
    assert!(!ini_path(&game).exists(), "no INI (nor its dir) before deploy");

    deploy_loose_mod(&store, &game);
    assert!(ini_path(&game).exists(), "ensure created the INI + its dir chain");

    purge(&store, &game).unwrap();

    let after = snapshot_tree(&game.prefix).unwrap();
    assert_trees_identical(&pristine, &after);
    assert!(!ini_path(&game).exists(), "CreatedByNexTwist INI deleted on purge");
    assert!(
        !ini_path(&game).parent().unwrap().exists(),
        "NexTwist-created Starfield config dir pruned"
    );
    assert!(
        game.prefix
            .join("drive_c/users/steamuser/Documents")
            .exists(),
        "the pre-existing Documents boundary is never removed"
    );
}

// ---------------------------------------------------------------------------
// SFINI-03a — surgical merge preserves everything else, exactly one [Archive].
// ---------------------------------------------------------------------------

#[test]
fn ini_merge_nonclobber() {
    let dir = TempDir::new().unwrap();
    let (store, game) = sf_fixture(dir.path(), MyGamesOpts::default());

    let original = b"; my config\n[Display]\niSize W=2560\n\n[Archive]\nbUseArchives=1\n";
    seed_ini(&game, original);

    deploy_loose_mod(&store, &game);

    let text = fs::read_to_string(ini_path(&game)).unwrap();
    assert_eq!(archive_count(text.as_bytes()), 1, "exactly one [Archive]");
    assert!(
        text.starts_with("; my config\n[Display]\niSize W=2560\n\n[Archive]\n"),
        "every unrelated section/comment/order byte preserved"
    );
    assert!(text.contains("bUseArchives=1\n"), "unrelated key preserved");
    assert!(text.contains("bInvalidateOlderFiles=1\n"), "owned key merged");
    assert!(text.contains("sResourceDataDirsFinal=\n"), "owned key merged");
    assert!(!text.contains("\r\n"), "an LF file stays LF");
}

// ---------------------------------------------------------------------------
// SFINI-03b — a CRLF+BOM file already carrying both keys is a byte-identical no-op.
// ---------------------------------------------------------------------------

#[test]
fn ini_crlf_bom_preserved() {
    let dir = TempDir::new().unwrap();
    let (store, game) = sf_fixture(dir.path(), MyGamesOpts::default());

    let mut original = BOM.to_vec();
    original.extend_from_slice(
        b"[Archive]\r\nbInvalidateOlderFiles=1\r\nsResourceDataDirsFinal=\r\n[Other]\r\nx=1\r\n",
    );
    seed_ini(&game, &original);

    deploy_loose_mod(&store, &game);
    assert_eq!(
        fs::read(ini_path(&game)).unwrap(),
        original,
        "an already-active CRLF+BOM INI is a byte-identical no-op"
    );
}

// ---------------------------------------------------------------------------
// SFINI-03c — a non-empty user sResourceDataDirsFinal blocks auto-activation.
// ---------------------------------------------------------------------------

#[test]
fn ini_conflict_blocks() {
    let dir = TempDir::new().unwrap();
    let (store, game) = sf_fixture(dir.path(), MyGamesOpts::default());

    let original = b"[Archive]\r\nsResourceDataDirsFinal=Mods\\\r\n";
    seed_ini(&game, original);

    // The deploy tail runs ensure under Block → returns Blocked (Ok), writes nothing, and
    // does NOT fail the deploy (the loose files still land).
    deploy_loose_mod(&store, &game);

    assert_eq!(
        fs::read(ini_path(&game)).unwrap(),
        original,
        "a non-empty user value is never clobbered by auto-activation"
    );
    assert!(
        game.install_dir.join("Data/meshes/x.nif").exists(),
        "the deploy itself still succeeded despite the INI conflict"
    );
}

// ---------------------------------------------------------------------------
// SFINI-04 — deploy×2 / profile-switch / crash-replay all converge to one [Archive].
// ---------------------------------------------------------------------------

#[test]
fn ini_idempotent_one_archive() {
    let dir = TempDir::new().unwrap();
    let (store, game) = sf_fixture(dir.path(), MyGamesOpts::default());

    // (a) deploy ×2 — the second is an AlreadyActive no-op.
    deploy_loose_mod(&store, &game);
    let baseline = fs::read(ini_path(&game)).unwrap();
    assert_eq!(baseline, FRESH_INI, "fresh activation is the CRLF/no-BOM recipe");
    deploy_loose_mod(&store, &game);
    let after_two = fs::read(ini_path(&game)).unwrap();
    assert_eq!(after_two, baseline, "deploy×2 is byte-identical");
    assert_eq!(archive_count(&after_two), 1);

    // (b) profile switch = redeploy_winners (purge + deploy_winners) round-trips for free.
    let winners = loose_winners(&game);
    redeploy_winners(&store, &game, &winners).unwrap();
    let after_switch = fs::read(ini_path(&game)).unwrap();
    assert_eq!(after_switch, baseline, "profile switch converges to one [Archive]");
    assert_eq!(archive_count(&after_switch), 1);

    // (c) crash-replay then re-deploy — the interrupted ensure rolls back to provenance,
    //     the next deploy re-activates, still exactly one [Archive], identical bytes.
    journal::begin_ini(&store, game.appid, Path::new(INI_FILENAME)).unwrap();
    recover_on_launch(&store, &game).unwrap();
    deploy_loose_mod(&store, &game);
    let after_replay = fs::read(ini_path(&game)).unwrap();
    assert_eq!(after_replay, baseline, "crash-replay + re-deploy converges");
    assert_eq!(archive_count(&after_replay), 1);
    assert!(store.pending_ops().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// SFINI-05 — recover_on_launch replays a crashed KIND_INI op to a consistent state.
// ---------------------------------------------------------------------------

#[test]
fn ini_crash_recovery_consistent() {
    // CreatedByNexTwist: a crash mid-op replays to ABSENCE (file gone), no pending rows.
    {
        let dir = TempDir::new().unwrap();
        let (store, game) = sf_fixture(dir.path(), MyGamesOpts::default());
        deploy_loose_mod(&store, &game);
        assert!(ini_path(&game).exists());
        // Simulate a crash mid-INI-op: a lingering pending KIND_INI row.
        journal::begin_ini(&store, game.appid, Path::new(INI_FILENAME)).unwrap();
        assert!(!store.pending_ops().unwrap().is_empty());
        recover_on_launch(&store, &game).unwrap();
        assert!(
            !ini_path(&game).exists(),
            "a CreatedByNexTwist INI replays to absence"
        );
        assert!(
            store.pending_ops().unwrap().is_empty(),
            "recovery leaves zero pending rows"
        );
    }

    // PreExisting: a crash mid-op replays to the original bytes, no pending rows.
    {
        let dir = TempDir::new().unwrap();
        let (store, game) = sf_fixture(dir.path(), MyGamesOpts::default());
        let mut original = BOM.to_vec();
        original.extend_from_slice(b"; user\r\n[Archive]\r\nsResourceDataDirsFinal=\r\n");
        seed_ini(&game, &original);
        deploy_loose_mod(&store, &game);
        assert_ne!(fs::read(ini_path(&game)).unwrap(), original);
        journal::begin_ini(&store, game.appid, Path::new(INI_FILENAME)).unwrap();
        recover_on_launch(&store, &game).unwrap();
        assert_eq!(
            fs::read(ini_path(&game)).unwrap(),
            original,
            "a PreExisting INI replays to its recorded provenance bytes"
        );
        assert!(store.pending_ops().unwrap().is_empty());
    }
}

// ---------------------------------------------------------------------------
// SFINI-04 (unit-style) — the KIND_INI target is under the prefix, never under Data/.
// ---------------------------------------------------------------------------

#[test]
fn ini_kind_path_under_prefix() {
    let dir = TempDir::new().unwrap();
    let (store, game) = sf_fixture(dir.path(), MyGamesOpts::default());

    let ini = ini_path(&game);
    assert_eq!(
        ini,
        steam::my_games_path(&game.prefix).join(INI_FILENAME),
        "the INI target is my_games_path(prefix)/StarfieldCustom.ini"
    );
    assert!(
        ini.starts_with(game.prefix.join("drive_c")),
        "the INI target is under <prefix>/drive_c"
    );

    // Activate, then crash-replay: the replay must resolve via my_games_path, never Data/.
    let data_ini = game.install_dir.join("Data").join(INI_FILENAME);
    deploy_loose_mod(&store, &game);
    journal::begin_ini(&store, game.appid, Path::new(INI_FILENAME)).unwrap();
    recover_on_launch(&store, &game).unwrap();
    assert!(
        !data_ini.exists(),
        "KIND_INI replay must NEVER touch <install>/Data/StarfieldCustom.ini"
    );
    assert!(!ini.exists(), "the created INI was rolled back under the prefix");
}

// ---------------------------------------------------------------------------
// Non-Starfield control — a Skyrim SE deploy touches NO INI and takes no vanilla row.
// ---------------------------------------------------------------------------

#[test]
fn nonstarfield_deploy_touches_no_ini() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    // A Skyrim SE game that HAS a Starfield-shaped prefix (so the only thing stopping an
    // INI write is the is_starfield gate, not a missing prefix).
    let prefix = fake_my_games_prefix(&root.join("prefix"), "Starfield", MyGamesOpts::default())
        .unwrap();
    let (store, game) = game_with_prefix(root, SKYRIM_SE, prefix);

    deploy_loose_mod(&store, &game);

    assert!(
        !steam::my_games_path(&game.prefix).join(INI_FILENAME).exists(),
        "a non-Starfield deploy writes no StarfieldCustom.ini"
    );
    assert!(
        store
            .vanilla_for(game.appid, Path::new(INI_FILENAME))
            .unwrap()
            .is_none(),
        "a non-Starfield deploy takes no StarfieldCustom.ini vanilla row"
    );
}

// ---------------------------------------------------------------------------
// SFINI-05 — verify()/repair() treat the INI like a deployed file.
// ---------------------------------------------------------------------------

#[test]
fn ini_verify_pristine_when_active() {
    let dir = TempDir::new().unwrap();
    let (store, game) = sf_fixture(dir.path(), MyGamesOpts::default());
    deploy_loose_mod(&store, &game); // auto-activates the INI
    assert!(
        verify(&store, &game).unwrap().pristine,
        "verify is pristine when the INI is active and the Data/ files intact"
    );
}

#[test]
fn ini_verify_detects_removed_ini() {
    let dir = TempDir::new().unwrap();
    let (store, game) = sf_fixture(dir.path(), MyGamesOpts::default());
    deploy_loose_mod(&store, &game);
    let baseline = fs::read(ini_path(&game)).unwrap();
    assert!(verify(&store, &game).unwrap().pristine);

    // Delete the INI on disk while the loose files are still deployed → drift.
    fs::remove_file(ini_path(&game)).unwrap();
    assert!(
        !verify(&store, &game).unwrap().pristine,
        "a removed StarfieldCustom.ini is drift while loose files are deployed"
    );

    repair(&store, &game).unwrap();
    assert!(ini_path(&game).exists(), "repair re-activated the INI");
    assert!(
        verify(&store, &game).unwrap().pristine,
        "pristine again after repair"
    );
    assert_eq!(
        fs::read(ini_path(&game)).unwrap(),
        baseline,
        "the restored INI is byte-identical to the post-activation baseline"
    );
}

#[test]
fn ini_verify_detects_key_stripped_ini() {
    let dir = TempDir::new().unwrap();
    let (store, game) = sf_fixture(dir.path(), MyGamesOpts::default());
    deploy_loose_mod(&store, &game);
    let baseline = fs::read(ini_path(&game)).unwrap();

    // Strip the two owned keys out of the on-disk INI → drift.
    fs::write(ini_path(&game), b"[Archive]\r\n").unwrap();
    assert!(
        !verify(&store, &game).unwrap().pristine,
        "a key-stripped StarfieldCustom.ini is drift"
    );

    repair(&store, &game).unwrap();
    let restored = fs::read(ini_path(&game)).unwrap();
    assert_eq!(archive_count(&restored), 1, "repair leaves exactly one [Archive]");
    assert_eq!(restored, baseline, "repair restores both owned keys byte-identically");
    assert!(verify(&store, &game).unwrap().pristine);
}

#[test]
fn ini_verify_conflict_is_not_drift() {
    let dir = TempDir::new().unwrap();
    let (store, game) = sf_fixture(dir.path(), MyGamesOpts::default());

    // A pre-existing non-empty user value: blocked at deploy, left as-is.
    let original = b"[Archive]\r\nsResourceDataDirsFinal=Mods\\\r\n";
    seed_ini(&game, original);
    deploy_loose_mod(&store, &game);
    assert_eq!(fs::read(ini_path(&game)).unwrap(), original);

    // A user-blocked value is a recorded state, NOT repairable drift.
    assert!(
        verify(&store, &game).unwrap().pristine,
        "a user-blocked conflict is not verify drift"
    );
    repair(&store, &game).unwrap();
    assert_eq!(
        fs::read(ini_path(&game)).unwrap(),
        original,
        "repair must never clobber a user-blocked value"
    );
}

#[test]
fn ini_verify_ignores_nonstarfield() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let prefix = fake_my_games_prefix(&root.join("prefix"), "Starfield", MyGamesOpts::default())
        .unwrap();
    let (store, game) = game_with_prefix(root, SKYRIM_SE, prefix);

    deploy_loose_mod(&store, &game);
    assert!(
        verify(&store, &game).unwrap().pristine,
        "a non-Starfield game never reports INI drift"
    );
    repair(&store, &game).unwrap();
    assert!(
        !steam::my_games_path(&game.prefix).join(INI_FILENAME).exists(),
        "repair never creates an INI for a non-Starfield game"
    );
}
