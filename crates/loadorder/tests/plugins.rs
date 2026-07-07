//! PLUGIN-01/02 + masterlist integration tests (the apply-write path).
//!
//! These exercise the REAL libloot seam against a fixture Proton prefix built by
//! `testkit::fake_proton_prefix`, asserting:
//!   * PLUGIN-02: `apply_load_order` writes an asterisk-format, masters-first Plugins.txt
//!     at libloot's `active_plugins_file_path`, bounded inside the prefix.
//!   * PLUGIN-01: a disabled plugin is NOT written with a leading asterisk (inactive).
//!   * Masterlist caching: a fresh cache is reused with no network, and the bundled CC0
//!     snapshot seeds the cache when offline.
//!
//! libloot/libloadorder header-parse every named plugin, so the fixture writes minimal but
//! VALID 24-byte TES4 records in the game `Data/` dir (matching the Plan-02 spike).

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use loadorder::loot::{appdata_local_path, apply_load_order, propose_sort, protected_plugins};
use loadorder::masterlist::{cache_path, ensure_masterlist};
use nextwist_core::{Plugin, PluginKind};
use tempfile::TempDir;

const SKYRIM_SE: u32 = 489830;
const GAME_FOLDER: &str = "Skyrim Special Edition";
const FALLOUT4: u32 = 377160;
const FO4_FOLDER: &str = "Fallout4";
const STARFIELD: u32 = 1716740;
const SF_FOLDER: &str = "Starfield";

/// Minimal esplugin-parseable plugin file (a bare 24-byte TES4 header record).
fn write_min_plugin(data_dir: &Path, name: &str, master: bool) {
    let mut bytes = Vec::with_capacity(24);
    bytes.extend_from_slice(b"TES4");
    bytes.extend_from_slice(&0u32.to_le_bytes()); // size_of_subrecords = 0
    let flags: u32 = if master { 0x1 } else { 0x0 };
    bytes.extend_from_slice(&flags.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes()); // form_id
    bytes.extend_from_slice(&[0u8; 8]); // version control + unknown (ignored)
    fs::write(data_dir.join(name), &bytes).unwrap();
}

fn plugin(name: &str, kind: PluginKind, enabled: bool, order: u32) -> Plugin {
    Plugin { name: name.into(), kind, enabled, order }
}

/// PLUGIN-02: apply a desired order and assert the written Plugins.txt is asterisk-format,
/// masters-first, bounded under the prefix.
#[test]
fn writes_asterisk_masters_first() {
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("install");
    let data = install.join("Data");
    fs::create_dir_all(&data).unwrap();
    // A master, a second master, a regular plugin, and a disabled regular plugin.
    write_min_plugin(&data, "Skyrim.esm", true);
    write_min_plugin(&data, "Update.esm", true);
    write_min_plugin(&data, "Mod.esp", false);
    write_min_plugin(&data, "Off.esp", false);

    let prefix_root = tmp.path().join("pfx");
    testkit::fake_proton_prefix(&prefix_root, GAME_FOLDER, None).unwrap();
    let appdata_local = appdata_local_path(&prefix_root, GAME_FOLDER);

    // Deliberately pass a .esp BEFORE the .esm in the slice; masters-first must hoist.
    let desired = vec![
        plugin("Mod.esp", PluginKind::Esp, true, 0),
        plugin("Skyrim.esm", PluginKind::Esm, true, 1),
        plugin("Update.esm", PluginKind::Esm, true, 2),
        plugin("Off.esp", PluginKind::Esp, false, 3),
    ];

    let written = apply_load_order(SKYRIM_SE, &install, &appdata_local, &desired).unwrap();

    // The write is bounded inside the prefix AppData (T-02-11).
    assert!(
        written.starts_with(&appdata_local),
        "plugins.txt {written:?} must be under the prefix AppData {appdata_local:?}"
    );

    let body = fs::read_to_string(&written).unwrap();

    // libloot's SkyrimSE asterisk format lists only TOGGLEABLE (non-master) plugins;
    // .esm masters are implicitly active and governed by the load order, not the
    // active-plugins file (verified against libloot 0.29.5 output). So the enabled regular
    // plugin appears with a leading asterisk, and masters are NOT listed here.
    assert!(body.contains("*Mod.esp"), "active regular plugin is asterisk-listed:\n{body}");
    assert!(
        !body.contains("Skyrim.esm") && !body.contains("Update.esm"),
        "masters are implicitly active and NOT written to the asterisk file:\n{body}"
    );

    // Masters-first is enforced in the LOAD ORDER (libloot), observable via the order +
    // active queries — re-open and assert masters precede the regular plugin and are active.
    let game = loadorder::loot::open_game(SKYRIM_SE, &install, &appdata_local).unwrap();
    // (open_game does not load state; re-load to read the persisted order.)
    let mut game = game;
    game.load_current_load_order_state().unwrap();
    let order: Vec<&str> = game.load_order();
    let pos_skyrim = order.iter().position(|n| *n == "Skyrim.esm");
    let pos_update = order.iter().position(|n| *n == "Update.esm");
    let pos_mod = order.iter().position(|n| *n == "Mod.esp");
    if let (Some(su), Some(up), Some(md)) = (pos_skyrim, pos_update, pos_mod) {
        assert!(su < md && up < md, "masters must load before Mod.esp: {order:?}");
    }
    assert!(game.is_plugin_active("Mod.esp"), "Mod.esp is active");
    assert!(!game.is_plugin_active("Off.esp"), "Off.esp is inactive");
}

/// PLUGIN-01: a disabled plugin is reflected as NOT active (no leading asterisk).
#[test]
fn toggle_active_reflected() {
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("install");
    let data = install.join("Data");
    fs::create_dir_all(&data).unwrap();
    write_min_plugin(&data, "Skyrim.esm", true);
    write_min_plugin(&data, "Off.esp", false);

    let prefix_root = tmp.path().join("pfx");
    testkit::fake_proton_prefix(&prefix_root, GAME_FOLDER, None).unwrap();
    let appdata_local = appdata_local_path(&prefix_root, GAME_FOLDER);

    let desired = vec![
        plugin("Skyrim.esm", PluginKind::Esm, true, 0),
        plugin("Off.esp", PluginKind::Esp, false, 1),
    ];
    let written = apply_load_order(SKYRIM_SE, &install, &appdata_local, &desired).unwrap();
    let body = fs::read_to_string(&written).unwrap();

    // Off.esp appears (order recorded) but WITHOUT a leading asterisk (inactive).
    assert!(
        !body.contains("*Off.esp"),
        "disabled Off.esp must not be asterisk-active:\n{body}"
    );
}

/// Plugins with no on-disk file are dropped from the order (stale store row must not abort
/// the write — libloot would error header-parsing a missing file).
#[test]
fn missing_on_disk_plugin_is_dropped_not_fatal() {
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("install");
    let data = install.join("Data");
    fs::create_dir_all(&data).unwrap();
    write_min_plugin(&data, "Skyrim.esm", true);
    // "Ghost.esp" is NOT written to disk.

    let prefix_root = tmp.path().join("pfx");
    testkit::fake_proton_prefix(&prefix_root, GAME_FOLDER, None).unwrap();
    let appdata_local = appdata_local_path(&prefix_root, GAME_FOLDER);

    let desired = vec![
        plugin("Skyrim.esm", PluginKind::Esm, true, 0),
        plugin("Ghost.esp", PluginKind::Esp, true, 1),
    ];
    // Must not error even though Ghost.esp has no file on disk (it is dropped from the
    // order before libloot header-parses it).
    let written = apply_load_order(SKYRIM_SE, &install, &appdata_local, &desired).unwrap();
    let body = fs::read_to_string(&written).unwrap();
    assert!(!body.contains("Ghost.esp"), "missing plugin dropped:\n{body}");
    // Skyrim.esm (a master, implicitly active) is loaded but not written to the file.
    let mut game = loadorder::loot::open_game(SKYRIM_SE, &install, &appdata_local).unwrap();
    game.load_current_load_order_state().unwrap();
    assert!(game.is_plugin_active("Skyrim.esm"), "master is active");
}

/// Masterlist: a fresh cache is reused with no network (D-10).
#[test]
fn ensure_masterlist_uses_cache_offline() {
    let dir = TempDir::new().unwrap();
    let cache = cache_path(dir.path(), SKYRIM_SE);
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, "globals: []\n").unwrap();

    // refresh=false + fresh cache returns the cache (network code never runs; if it did
    // and there is no network in CI it would still pass via the bundled fallback, but the
    // contents prove the SEEDED cache was returned, not a re-fetch/re-seed).
    let got = ensure_masterlist(dir.path(), SKYRIM_SE, false).unwrap();
    assert_eq!(got, cache);
    assert_eq!(fs::read_to_string(&got).unwrap(), "globals: []\n");
}

/// Masterlist: the bundled CC0 snapshot is a real, non-empty masterlist (offline floor).
#[test]
fn bundled_snapshot_is_present_and_nonempty() {
    let dir = TempDir::new().unwrap();
    // No cache present, refresh=true → fetch is attempted; in a no-network CI the bundled
    // fallback seeds the cache. Either way the result is a non-empty masterlist file.
    let got = ensure_masterlist(dir.path(), SKYRIM_SE, true).unwrap();
    let body = fs::read_to_string(&got).unwrap();
    assert!(!body.is_empty(), "masterlist must be non-empty");
}

/// PLUGIN-03 (D-12): propose_sort returns a proposed order and WRITES NOTHING — the
/// prefix Plugins.txt is untouched until a separate apply.
#[test]
fn propose_sort_returns_order_without_writing() {
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("install");
    let data = install.join("Data");
    fs::create_dir_all(&data).unwrap();
    write_min_plugin(&data, "Skyrim.esm", true);
    write_min_plugin(&data, "Mod.esp", false);

    let prefix_root = tmp.path().join("pfx");
    testkit::fake_proton_prefix(&prefix_root, GAME_FOLDER, None).unwrap();
    let appdata_local = appdata_local_path(&prefix_root, GAME_FOLDER);
    // app_data dir for the masterlist cache (the bundled CC0 snapshot seeds it offline).
    let app_data = tmp.path().join("appdata");
    fs::create_dir_all(&app_data).unwrap();

    let desired = vec![
        plugin("Skyrim.esm", PluginKind::Esm, true, 0),
        plugin("Mod.esp", PluginKind::Esp, true, 1),
    ];

    // Confirm no Plugins.txt exists before the proposal.
    let plugins_txt = appdata_local.join("Plugins.txt");
    assert!(!plugins_txt.exists(), "no Plugins.txt before propose");

    let proposal =
        propose_sort(SKYRIM_SE, &install, &appdata_local, &app_data, &desired).unwrap();

    // A proposed order is returned over the loaded plugins.
    assert!(
        proposal.proposed.iter().any(|n| n == "Skyrim.esm"),
        "Skyrim.esm in proposed: {:?}",
        proposal.proposed
    );
    assert!(
        proposal.proposed.iter().any(|n| n == "Mod.esp"),
        "Mod.esp in proposed: {:?}",
        proposal.proposed
    );
    // D-12: propose writes NOTHING — the prefix Plugins.txt must still be absent.
    assert!(
        !plugins_txt.exists(),
        "propose_sort must not write Plugins.txt (propose-then-apply)"
    );
    // warnings is a list (may be empty); type contract holds.
    let _ = &proposal.warnings;
}

/// RC1 regression (debug `loadorder-active-write`): a REAL-DATA-STYLE Fallout 4 set with
/// MULTIPLE DLC masters supplied in NON-hardcoded (alphabetical) order — exactly the shape
/// the synthetic single-plugin fixtures missed.
///
/// Fallout 4's hardcoded early-loader DLC order is `Fallout4.esm, DLCRobot.esm,
/// DLCworkshop01.esm, DLCCoast.esm, ...`. NexTwist's store rows all carry `order == 0` when
/// the user has not manually reordered, so the old alphabetical master sort produced
/// `DLCCoast` before `DLCRobot`, which libloadorder REJECTS with "load order interaction
/// failed". This test passes that exact alphabetical order in and asserts:
///   1. `apply_load_order` SUCCEEDS (no "load order interaction failed"),
///   2. the persisted order is libloot's hardcoded DLC order, game-master first
///      (`Fallout4.esm` before every DLC; `DLCRobot.esm` before `DLCCoast.esm`),
///   3. a regular enabled `.esp` survives as an ACTIVE `*Name` line in Plugins.txt,
///   4. a regular disabled `.esp` stays ordered but WITHOUT an asterisk.
///
/// Early-loading DLC `.esm` are implicitly active and intentionally OMITTED from
/// Plugins.txt by libloot, so the file legitimately lists only the regular mods.
#[test]
fn fo4_multi_master_game_master_first_active_survives() {
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("install");
    let data = install.join("Data");
    fs::create_dir_all(&data).unwrap();
    // Game master + three hardcoded DLC masters (in NON-hardcoded order on disk) + two
    // regular mods (one enabled, one disabled).
    write_min_plugin(&data, "Fallout4.esm", true);
    write_min_plugin(&data, "DLCCoast.esm", true);
    write_min_plugin(&data, "DLCRobot.esm", true);
    write_min_plugin(&data, "DLCworkshop01.esm", true);
    write_min_plugin(&data, "EnabledMod.esp", false);
    write_min_plugin(&data, "DisabledMod.esp", false);

    let prefix_root = tmp.path().join("pfx");
    testkit::fake_proton_prefix(&prefix_root, FO4_FOLDER, None).unwrap();
    let appdata_local = appdata_local_path(&prefix_root, FO4_FOLDER);

    // Desired set with every order_index == 0 (the real DB shape that triggered RC1) and
    // the DLCs listed ALPHABETICALLY — the order the old code would have sent to libloot.
    let desired = vec![
        plugin("DLCCoast.esm", PluginKind::Esm, true, 0),
        plugin("DLCRobot.esm", PluginKind::Esm, true, 0),
        plugin("DLCworkshop01.esm", PluginKind::Esm, true, 0),
        plugin("Fallout4.esm", PluginKind::Esm, true, 0),
        plugin("DisabledMod.esp", PluginKind::Esp, false, 0),
        plugin("EnabledMod.esp", PluginKind::Esp, true, 0),
    ];

    // MUST NOT error (the RC1 failure mode).
    let written = apply_load_order(FALLOUT4, &install, &appdata_local, &desired)
        .expect("apply_load_order must not fail on a multi-DLC FO4 set (RC1)");

    // Re-open and read the persisted order to assert game-master-first + hardcoded DLC order.
    let mut game = loadorder::loot::open_game(FALLOUT4, &install, &appdata_local).unwrap();
    game.load_current_load_order_state().unwrap();
    let order: Vec<&str> = game.load_order();
    let pos = |n: &str| order.iter().position(|x| *x == n);

    let fo4 = pos("Fallout4.esm").expect("Fallout4.esm present");
    let robot = pos("DLCRobot.esm").expect("DLCRobot.esm present");
    let coast = pos("DLCCoast.esm").expect("DLCCoast.esm present");
    let ws01 = pos("DLCworkshop01.esm").expect("DLCworkshop01.esm present");

    assert!(fo4 < robot && fo4 < coast && fo4 < ws01, "Fallout4.esm must be first: {order:?}");
    // Hardcoded FO4 order is Fallout4, DLCRobot, DLCworkshop01, DLCCoast — NOT alphabetical.
    assert!(robot < coast, "DLCRobot.esm must precede DLCCoast.esm (hardcoded order): {order:?}");
    assert!(ws01 < coast, "DLCworkshop01.esm must precede DLCCoast.esm (hardcoded order): {order:?}");

    // Active state: DLC masters are implicitly active; mods follow the seed.
    assert!(game.is_plugin_active("Fallout4.esm"), "game master active");
    assert!(game.is_plugin_active("DLCRobot.esm"), "DLC master active");
    assert!(game.is_plugin_active("EnabledMod.esp"), "enabled mod active");
    assert!(!game.is_plugin_active("DisabledMod.esp"), "disabled mod inactive");

    // Plugins.txt: regular mods only (DLC .esm are early-loaders, omitted by libloot).
    let body = fs::read_to_string(&written).unwrap();
    assert!(body.contains("*EnabledMod.esp"), "enabled mod is asterisk-active:\n{body}");
    assert!(
        body.contains("DisabledMod.esp") && !body.contains("*DisabledMod.esp"),
        "disabled mod present without asterisk:\n{body}"
    );
    assert!(
        !body.contains("Fallout4.esm") && !body.contains("DLCRobot.esm"),
        "early-loading masters are implicitly active and NOT written to Plugins.txt:\n{body}"
    );
}

/// SFLO-03: `protected_plugins` flags a libloot-implicitly-active base master (Starfield.esm)
/// and does NOT flag a user-enabled regular `.esp`. The protected set is derived purely from
/// `is_plugin_active` — no hard-coded name list in the engine.
#[test]
fn protected_plugins_flags_implicit_master_not_user_esp() {
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("install");
    let data = install.join("Data");
    fs::create_dir_all(&data).unwrap();
    write_min_plugin(&data, "Starfield.esm", true); // game master (implicitly active)
    write_min_plugin(&data, "Mod.esp", false); // a user regular plugin

    let prefix_root = tmp.path().join("pfx");
    // Seed a user *-line for Mod.esp so it counts as NexTwist-enabled (never protected).
    testkit::fake_proton_prefix(&prefix_root, SF_FOLDER, Some("*Mod.esp\n")).unwrap();
    let appdata_local = appdata_local_path(&prefix_root, SF_FOLDER);

    let enabled: HashSet<String> = ["Mod.esp".to_string()].into_iter().collect();
    let protected = protected_plugins(STARFIELD, &install, &appdata_local, &enabled).unwrap();

    assert!(
        protected.contains("Starfield.esm"),
        "the implicit game master is protected: {protected:?}"
    );
    assert!(
        !protected.contains("Mod.esp"),
        "a user-enabled *-line plugin is never protected: {protected:?}"
    );
}

/// CR-01 (the REAL save flow, previously untested): a locked Starfield base master arrives at
/// its RESTING representation — `enabled == false` (it is active WITHOUT a `*` line; NexTwist
/// never asterisk-writes masters, libloot owns their activation) — included UNCHANGED in a
/// normal desired list. This is exactly what `list_plugins` produces for a protected master and
/// what the UI sends back unchanged, so it MUST save successfully with NO false `ProtectedMaster`
/// rejection, and the asterisk `plugins.txt` is still written for the enabled regular plugins.
#[test]
fn starfield_locked_master_saves_at_resting_state() {
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("install");
    let data = install.join("Data");
    fs::create_dir_all(&data).unwrap();
    write_min_plugin(&data, "Starfield.esm", true);
    write_min_plugin(&data, "Mod.esp", false);

    let prefix_root = tmp.path().join("pfx");
    testkit::fake_proton_prefix(&prefix_root, SF_FOLDER, None).unwrap();
    let appdata_local = appdata_local_path(&prefix_root, SF_FOLDER);

    // The base master at its RESTING state (`enabled == false`, as `list_plugins` produces for a
    // protected master), included UNCHANGED alongside an enabled regular plugin.
    let desired = vec![
        plugin("Starfield.esm", PluginKind::Esm, false, 0),
        plugin("Mod.esp", PluginKind::Esp, true, 1),
    ];

    let written = apply_load_order(STARFIELD, &install, &appdata_local, &desired)
        .expect("a locked master at its resting enabled==false state must save (CR-01)");

    let body = fs::read_to_string(&written).unwrap();
    assert!(body.contains("*Mod.esp"), "enabled regular plugin is asterisk-listed:\n{body}");
    assert!(
        !body.contains("Starfield.esm"),
        "the implicit master is NOT written to the asterisk file:\n{body}"
    );
}

/// CR-01 (a GENUINE reorder of a pinned master is still prevented — now by libloot itself, not a
/// NexTwist name/enabled guard): a request that SWAPS two masters (Constellation before the
/// Starfield.esm game master), both at their resting `enabled == false` state. `reconcile_order`
/// forces every master into libloot's canonical position UNCONDITIONALLY, so the save SUCCEEDS
/// but the persisted order keeps the game master pinned FIRST — the swap is neutralized, never
/// honored. (There is no way to move a pinned master through `apply_load_order`.)
#[test]
fn starfield_pinned_master_reorder_is_neutralized() {
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("install");
    let data = install.join("Data");
    fs::create_dir_all(&data).unwrap();
    write_min_plugin(&data, "Starfield.esm", true);
    write_min_plugin(&data, "Constellation.esm", true);
    write_min_plugin(&data, "Mod.esp", false);

    let prefix_root = tmp.path().join("pfx");
    testkit::fake_proton_prefix(&prefix_root, SF_FOLDER, None).unwrap();
    let appdata_local = appdata_local_path(&prefix_root, SF_FOLDER);

    // Request SWAPS the masters (Constellation before Starfield), both at resting state.
    let desired = vec![
        plugin("Constellation.esm", PluginKind::Esm, false, 0),
        plugin("Starfield.esm", PluginKind::Esm, false, 1),
        plugin("Mod.esp", PluginKind::Esp, true, 2),
    ];

    // Succeeds (no false rejection) — the swap is neutralized by reconcile_order, not rejected.
    apply_load_order(STARFIELD, &install, &appdata_local, &desired)
        .expect("save succeeds; the master swap is neutralized, not rejected (CR-01)");

    // The persisted order keeps the game master pinned FIRST despite the swapped request.
    let mut game = loadorder::loot::open_game(STARFIELD, &install, &appdata_local).unwrap();
    game.load_current_load_order_state().unwrap();
    let order: Vec<&str> = game.load_order();
    let pos = |n: &str| order.iter().position(|x| *x == n);
    let sf = pos("Starfield.esm").expect("Starfield.esm present");
    let con = pos("Constellation.esm").expect("Constellation.esm present");
    assert!(
        sf < con,
        "the game master stays pinned before Constellation.esm despite the swap: {order:?}"
    );
}

/// SFLO-01: `apply_load_order` writes an asterisk-format Plugins.txt bounded under the
/// Starfield prefix AppData — enabled regular plugins as `*Name`, implicit masters omitted.
#[test]
fn starfield_asterisk() {
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("install");
    let data = install.join("Data");
    fs::create_dir_all(&data).unwrap();
    write_min_plugin(&data, "Starfield.esm", true);
    write_min_plugin(&data, "Mod.esp", false);
    write_min_plugin(&data, "Off.esp", false);

    let prefix_root = tmp.path().join("pfx");
    testkit::fake_proton_prefix(&prefix_root, SF_FOLDER, None).unwrap();
    let appdata_local = appdata_local_path(&prefix_root, SF_FOLDER);

    // The game master is user-enabled (normal case) → never protected → guard dormant.
    let desired = vec![
        plugin("Starfield.esm", PluginKind::Esm, true, 0),
        plugin("Mod.esp", PluginKind::Esp, true, 1),
        plugin("Off.esp", PluginKind::Esp, false, 2),
    ];
    let written = apply_load_order(STARFIELD, &install, &appdata_local, &desired).unwrap();

    // Bounded inside the prefix AppData (T-07-03 / T-02-11).
    assert!(
        written.starts_with(&appdata_local),
        "plugins.txt {written:?} must be under the Starfield prefix AppData {appdata_local:?}"
    );
    let body = fs::read_to_string(&written).unwrap();
    assert!(body.contains("*Mod.esp"), "active regular plugin is asterisk-listed:\n{body}");
    assert!(
        !body.contains("*Off.esp"),
        "disabled regular plugin has no asterisk:\n{body}"
    );
    assert!(
        !body.contains("Starfield.esm"),
        "the implicit master is NOT written to the asterisk file:\n{body}"
    );
}

/// SFLO-02: `propose_sort` over the bundled Starfield masterlist is DETERMINISTIC (same
/// inputs → same order across runs) and carries the recorded masterlist snapshot date.
#[test]
fn starfield_sort_determinism() {
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("install");
    let data = install.join("Data");
    fs::create_dir_all(&data).unwrap();
    write_min_plugin(&data, "Starfield.esm", true);
    write_min_plugin(&data, "Mod.esp", false);

    let prefix_root = tmp.path().join("pfx");
    testkit::fake_proton_prefix(&prefix_root, SF_FOLDER, None).unwrap();
    let appdata_local = appdata_local_path(&prefix_root, SF_FOLDER);
    let app_data = tmp.path().join("appdata");
    fs::create_dir_all(&app_data).unwrap();

    let desired = vec![
        plugin("Starfield.esm", PluginKind::Esm, true, 0),
        plugin("Mod.esp", PluginKind::Esp, true, 1),
    ];

    let first = propose_sort(STARFIELD, &install, &appdata_local, &app_data, &desired).unwrap();
    let second = propose_sort(STARFIELD, &install, &appdata_local, &app_data, &desired).unwrap();

    assert_eq!(
        first.proposed, second.proposed,
        "propose_sort must be deterministic over the bundled masterlist"
    );
    assert_eq!(
        first.masterlist_date, "2026-07-07",
        "the recorded Starfield masterlist snapshot date is surfaced (SFLO-02)"
    );
}
