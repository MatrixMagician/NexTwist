//! A1/A3 de-risk spike: prove the libloot Linux seam.
//!
//! The single largest technical unknown of Phase 2 (RESEARCH Assumptions A1/A3) is
//! whether libloot's load-order machinery works on Linux against a Proton-prefix
//! AppData directory. On Linux `Game::new` returns `NoLocalAppData` (libloadorder's
//! `local_path()` calls `dirs::data_local_dir()` which has no meaning inside a Proton
//! prefix), so NexTwist must ALWAYS supply the AppData path via `Game::with_local_path`
//! (Pitfall 1). This test proves, against a FIXTURE prefix built by
//! `testkit::fake_proton_prefix` (no real hardware), that:
//!
//!   1. `with_local_path` constructs a `Game` with NO `NoLocalAppData` error (the
//!      non-negotiable A1/A3 goal), and
//!   2. a `load → set_load_order` round-trip writes an asterisk-format `Plugins.txt`
//!      at the libloot-reported `active_plugins_file_path()`, which lives under the
//!      fixture AppData/Local path (T-02-04: the write stays bounded inside the prefix).
//!
//! libloot/libloadorder open and header-parse every plugin named in a load order
//! (esplugin `parse_reader(header_only())`), so the fixture places minimal but VALID
//! 24-byte TES4 records in the game `Data/` dir — see [`write_min_plugin`].

use std::fs;
use std::path::Path;

use loadorder::loot::{
    appdata_local_path, game_type_for, load_canonical_order, open_game, set_order_and_save,
};
use loadorder::LoadOrderError;
use tempfile::TempDir;

const SKYRIM_SE: u32 = 489830;
const FALLOUT4: u32 = 377160;
const GAME_FOLDER: &str = "Skyrim Special Edition";

/// Write a minimal, esplugin-parseable SkyrimSE plugin file (a bare TES4 header
/// record) at `data_dir/<name>`. Layout (24-byte non-Morrowind record header):
///   [0..4)   record type b"TES4"
///   [4..8)   size_of_subrecords = 0  (u32 LE) -> header-only, no subrecord bytes
///   [8..12)  flags                   (u32 LE) -> 0x1 = master file (else regular)
///   [12..16) form_id = 0             (u32 LE)
///   [16..24) version-control / unknown (ignored by the header parser)
/// `header_only()` parsing reads exactly these 24 bytes, so this is the smallest
/// file libloadorder will accept when building the load order.
fn write_min_plugin(data_dir: &Path, name: &str, master: bool) {
    let mut bytes = Vec::with_capacity(24);
    bytes.extend_from_slice(b"TES4");
    bytes.extend_from_slice(&0u32.to_le_bytes()); // size_of_subrecords = 0
    let flags: u32 = if master { 0x1 } else { 0x0 };
    bytes.extend_from_slice(&flags.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes()); // form_id
    bytes.extend_from_slice(&[0u8; 8]); // version control + unknown (ignored)
    assert_eq!(bytes.len(), 24, "minimal TES4 header must be exactly 24 bytes");
    fs::write(data_dir.join(name), &bytes).unwrap();
}

#[test]
fn appdata_local_path_targets_the_proton_prefix_appdata_folder() {
    let prefix = Path::new("/games/compatdata/489830/pfx");
    let p = appdata_local_path(prefix, GAME_FOLDER);
    assert_eq!(
        p,
        Path::new(
            "/games/compatdata/489830/pfx/drive_c/users/steamuser/AppData/Local/Skyrim Special Edition"
        )
    );
}

#[test]
fn game_type_for_maps_only_the_two_supported_appids() {
    assert!(game_type_for(SKYRIM_SE).is_some());
    assert!(game_type_for(FALLOUT4).is_some());
    assert!(game_type_for(220).is_none()); // Half-Life 2: not supported
}

/// The PRIMARY A1/A3 assertion: `open_game` against a fixture prefix must NOT return
/// `NoLocalAppData`, proving the `with_local_path` Linux seam works.
#[test]
fn open_game_against_fixture_prefix_does_not_return_no_local_appdata() {
    let tmp = TempDir::new().unwrap();
    // Fixture install dir with a Data/ folder (with_local_path requires game_path to
    // be an existing directory).
    let install = tmp.path().join("install");
    let data = install.join("Data");
    fs::create_dir_all(&data).unwrap();
    write_min_plugin(&data, "Skyrim.esm", true); // implicitly-active hardcoded master

    // Fixture Proton-prefix AppData/Local/<game> (the with_local_path target).
    let prefix_root = tmp.path().join("pfx");
    testkit::fake_proton_prefix(&prefix_root, GAME_FOLDER, None).unwrap();
    let appdata_local = appdata_local_path(&prefix_root, GAME_FOLDER);

    let game = open_game(SKYRIM_SE, &install, &appdata_local);
    match game {
        Ok(_) => {}
        Err(LoadOrderError::NoLocalAppData(p)) => {
            panic!("A1/A3 REGRESSION: with_local_path still hit NoLocalAppData at {p:?}")
        }
        Err(e) => panic!("open_game failed for a non-seam reason: {e}"),
    }
}

/// Full round-trip: load state, set a masters-first order, and assert libloot wrote an
/// asterisk-format Plugins.txt at its reported path, bounded under the fixture AppData.
#[test]
fn set_order_round_trip_writes_asterisk_plugins_txt_under_the_fixture_appdata() {
    let tmp = TempDir::new().unwrap();
    let install = tmp.path().join("install");
    let data = install.join("Data");
    fs::create_dir_all(&data).unwrap();
    // A master (implicitly active) + a regular plugin we will activate.
    write_min_plugin(&data, "Skyrim.esm", true);
    write_min_plugin(&data, "MyMod.esp", false);

    // Seed the fixture Plugins.txt with MyMod.esp ALREADY active (asterisk format).
    // libloot 0.29.5's public Game API exposes order (set_load_order) and active-query
    // (is_plugin_active) but NO active-plugin setter, so active state enters via the
    // plugins.txt libloot loads (in real NexTwist this file is generated from the DB
    // plugin_state). load_canonical_order() (load_current_load_order_state + read the
    // resolved order) reads this active flag, and set_load_order preserves the active state
    // of already-loaded plugins, so save() re-writes the asterisk entry. (Documented as the
    // spike's API finding; the load + set split mirrors apply_load_order, debug
    // `loadorder-active-write`.)
    let prefix_root = tmp.path().join("pfx");
    testkit::fake_proton_prefix(&prefix_root, GAME_FOLDER, Some("*MyMod.esp\n")).unwrap();
    let appdata_local = appdata_local_path(&prefix_root, GAME_FOLDER);

    let mut game = open_game(SKYRIM_SE, &install, &appdata_local)
        .expect("open_game must succeed against the fixture prefix (A1/A3)");

    // Load the seeded active state (reads the asterisk flag), then persist the order.
    // Masters-first order (libloot enforces masters-first INTERNALLY; this order already
    // satisfies it).
    load_canonical_order(&mut game).expect("load_canonical_order must read the seeded state");
    set_order_and_save(&mut game, &["Skyrim.esm", "MyMod.esp"])
        .expect("set_order_and_save must persist the load order");

    // Sanity: libloot reports MyMod.esp active (read from the seeded asterisk entry).
    assert!(
        game.is_plugin_active("MyMod.esp"),
        "MyMod.esp should be active after loading the seeded asterisk Plugins.txt"
    );

    // libloot reports the exact file it writes; it must live under the fixture AppData.
    let active_file = game.active_plugins_file_path().clone();
    assert!(
        active_file.starts_with(&appdata_local),
        "plugins.txt path {active_file:?} must be bounded under the fixture AppData {appdata_local:?}"
    );
    assert_eq!(
        active_file.file_name().and_then(|s| s.to_str()),
        Some("Plugins.txt")
    );

    let contents = fs::read_to_string(&active_file).expect("Plugins.txt must exist after save");
    // SkyrimSE uses the asterisk-enabled format: an active non-implicit plugin is
    // written with a leading '*'. MyMod.esp was placed active in the order.
    assert!(
        contents.contains("*MyMod.esp"),
        "expected asterisk-format active entry for MyMod.esp, got:\n{contents}"
    );
}
