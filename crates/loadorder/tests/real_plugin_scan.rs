//! Plugin discovery against a copy of a REAL Skyrim SE install.
//!
//! The scan classifies plugins from their on-disk TES4 header flags. Every other test
//! feeds it hand-built 24-byte headers; this one runs it over the genuine 80-plugin Skyrim
//! SE set — the base masters, the Creation Club `.esl` content, and the real mixed-case
//! filenames Bethesda ships — where a header the parser mishandles shows up immediately.
//!
//! Skipped unless the sandbox exists (populate with `scripts/realtest-setup.sh`, which only
//! ever READS the real install), so CI and other machines are unaffected.

use std::path::PathBuf;

use esplugin::GameId;
use loadorder::{scan_plugin_views_for, view_to_plugin};
use nextwist_core::PluginKind;

fn sandbox_data() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/realtest/game/Data")
        .canonicalize()
        .ok()?;
    p.is_dir().then_some(p)
}

#[test]
fn scan_classifies_the_real_skyrim_plugin_set() {
    let Some(data) = sandbox_data() else {
        eprintln!("SKIP: no target/realtest/game sandbox (run scripts/realtest-setup.sh)");
        return;
    };

    let plugins: Vec<_> = scan_plugin_views_for(GameId::SkyrimSE, &[], &data)
        .expect("scanning a real game Data dir must not error")
        .into_iter()
        .map(view_to_plugin)
        .collect();

    eprintln!("scanned {} real plugins", plugins.len());
    assert!(
        plugins.len() >= 50,
        "expected the real plugin set, found {}",
        plugins.len()
    );

    // Skyrim.esm is the game master and must classify as a master.
    let skyrim = plugins
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("Skyrim.esm"))
        .expect("Skyrim.esm must be discovered in a real install");
    assert_eq!(
        skyrim.kind,
        PluginKind::Esm,
        "the game master must classify as a master"
    );

    // The real set mixes masters and light plugins; both arms of the classifier should
    // fire on genuine data rather than only on synthetic headers.
    let masters = plugins.iter().filter(|p| p.kind == PluginKind::Esm).count();
    let lights = plugins.iter().filter(|p| p.kind == PluginKind::Esl).count();
    eprintln!(
        "masters={masters} light={lights} regular={}",
        plugins.len() - masters - lights
    );
    assert!(masters > 0, "a real install has master files");

    // Discovery is de-duplicated and deterministic: no repeated names, sorted
    // case-insensitively.
    let mut names: Vec<String> = plugins.iter().map(|p| p.name.to_lowercase()).collect();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len(), "scan must de-duplicate by filename");
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(
        names, sorted,
        "scan must return a deterministic sorted order"
    );

    // Every discovered plugin defaults to disabled/order 0 — the store owns real state.
    assert!(
        plugins.iter().all(|p| !p.enabled && p.order == 0),
        "discovery must not invent enable/order state"
    );
}
