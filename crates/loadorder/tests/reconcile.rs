//! SFLO-04 reconciliation unit tests — the pure `reconcile_plugins_txt` verdict.
//!
//! No prefix / libloot needed: the function is pure, so these use synthetic recorded slices
//! and hand-written `plugins.txt` strings. They lock BOTH the expected-vs-drift classification
//! and the serde wire shape the frontend types against (`"InSync" | { Drift: string[] }`).

use std::collections::HashSet;

use loadorder::{reconcile_plugins_txt, ReconcileState};
use nextwist_core::{Plugin, PluginKind};

fn plugin(name: &str, kind: PluginKind, enabled: bool) -> Plugin {
    Plugin { name: name.into(), kind, enabled, order: 0 }
}

fn protected(names: &[&str]) -> HashSet<String> {
    names.iter().map(|s| (*s).to_string()).collect()
}

/// InSync: on-disk differs only by the game's EXPECTED on-launch rewrite — it adds a `.ccc`
/// entry (protected), re-adds a blueprint master (protected), and strips an implicit master
/// (a protected recorded plugin absent from disk). None of those are drift.
#[test]
fn reconcile_expected_deltas() {
    let recorded = vec![
        plugin("Starfield.esm", PluginKind::Esm, true), // implicit master (stripped on disk)
        plugin("BlueprintShips-Starfield.esm", PluginKind::Esm, true), // re-added on launch
        plugin("MyMod.esp", PluginKind::Esp, true),     // the real user intent
    ];
    let prot = protected(&["Starfield.esm", "BlueprintShips-Starfield.esm", "Constellation.ccc"]);
    // On disk: user mod present; blueprint re-added; a `.ccc` added; the implicit master
    // Starfield.esm stripped (absent). All deltas involve protected names → expected.
    let on_disk = "*MyMod.esp\n*BlueprintShips-Starfield.esm\n*Constellation.ccc\n";

    assert_eq!(
        reconcile_plugins_txt(&recorded, on_disk, &prot),
        ReconcileState::InSync
    );
}

/// Drift: a recorded non-protected user `.esp` vanished from disk, AND an unexpected
/// non-protected plugin appeared — both are real drift, listed sorted + unique.
#[test]
fn reconcile_real_drift() {
    let recorded = vec![
        plugin("Starfield.esm", PluginKind::Esm, true),
        plugin("MyMod.esp", PluginKind::Esp, true),
        plugin("OtherMod.esp", PluginKind::Esp, true), // this one vanished on disk
    ];
    let prot = protected(&["Starfield.esm"]);
    // MyMod present; OtherMod gone; Unknown.esp appeared (not recorded, not protected).
    let on_disk = "*MyMod.esp\n*Unknown.esp\n";

    assert_eq!(
        reconcile_plugins_txt(&recorded, on_disk, &prot),
        ReconcileState::Drift(vec!["OtherMod.esp".into(), "Unknown.esp".into()])
    );
}

/// Drift: a non-protected user plugin reordered relative to the recorded intent is real drift
/// (the game rewrites protected/implicit entries, never the relative order of user mods).
#[test]
fn reconcile_reorder_is_drift() {
    let recorded = vec![
        plugin("A.esp", PluginKind::Esp, true),
        plugin("B.esp", PluginKind::Esp, true),
    ];
    let on_disk = "*B.esp\n*A.esp\n"; // swapped
    assert_eq!(
        reconcile_plugins_txt(&recorded, on_disk, &HashSet::new()),
        ReconcileState::Drift(vec!["A.esp".into(), "B.esp".into()])
    );
}

/// InSync floors: an on-disk file byte-matching the recorded active set, and an empty file
/// with no recorded actives, are both in sync.
#[test]
fn reconcile_identical_and_empty_are_insync() {
    let recorded = vec![
        plugin("A.esp", PluginKind::Esp, true),
        plugin("B.esp", PluginKind::Esp, true),
    ];
    assert_eq!(
        reconcile_plugins_txt(&recorded, "*A.esp\n*B.esp\n", &HashSet::new()),
        ReconcileState::InSync
    );

    // No recorded actives (only a disabled plugin) + empty on-disk → InSync.
    let none_active = vec![plugin("Disabled.esp", PluginKind::Esp, false)];
    assert_eq!(
        reconcile_plugins_txt(&none_active, "", &HashSet::new()),
        ReconcileState::InSync
    );

    // Comment / blank lines are ignored (still InSync against a single recorded active).
    let one = vec![plugin("A.esp", PluginKind::Esp, true)];
    assert_eq!(
        reconcile_plugins_txt(&one, "# header\n\n*A.esp\n", &HashSet::new()),
        ReconcileState::InSync
    );
}

/// IN-03: a pure CASE difference between the recorded name and the on-disk `plugins.txt` name
/// (Wine case-folding) is NOT drift — same plugins, same order, only casing differs → InSync.
#[test]
fn reconcile_case_only_difference_is_insync() {
    let recorded = vec![
        plugin("MyMod.esp", PluginKind::Esp, true),
        plugin("Second.esp", PluginKind::Esp, true),
    ];
    // On disk the game rewrote the names lowercased, but same set and same relative order.
    let on_disk = "*mymod.esp\n*second.esp\n";
    assert_eq!(
        reconcile_plugins_txt(&recorded, on_disk, &HashSet::new()),
        ReconcileState::InSync
    );
}

/// The serde wire shape the frontend depends on: `InSync` is the bare string `"InSync"`,
/// `Drift(names)` is `{"Drift":[...]}` (default external tagging — NOT `{"InSync":null}`).
#[test]
fn reconcile_state_serde_wire_shape() {
    assert_eq!(
        serde_json::to_string(&ReconcileState::InSync).unwrap(),
        "\"InSync\""
    );
    assert_eq!(
        serde_json::to_string(&ReconcileState::Drift(vec!["A.esp".into()])).unwrap(),
        "{\"Drift\":[\"A.esp\"]}"
    );
    // Round-trips back to the same value.
    let s = serde_json::to_string(&ReconcileState::InSync).unwrap();
    assert_eq!(
        serde_json::from_str::<ReconcileState>(&s).unwrap(),
        ReconcileState::InSync
    );
}
