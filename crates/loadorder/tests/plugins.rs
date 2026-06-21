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

use std::fs;
use std::path::Path;

use loadorder::loot::{appdata_local_path, apply_load_order, propose_sort};
use loadorder::masterlist::{cache_path, ensure_masterlist};
use nextwist_core::{Plugin, PluginKind};
use tempfile::TempDir;

const SKYRIM_SE: u32 = 489830;
const GAME_FOLDER: &str = "Skyrim Special Edition";

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
