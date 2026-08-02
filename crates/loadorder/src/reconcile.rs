//! SFLO-04 on-launch `plugins.txt` reconciliation — a PURE compare-and-classify verdict.
//!
//! Starfield rewrites its `plugins.txt` on launch: it adds Creation-Club `.ccc` entries,
//! strips implicitly-active masters (they don't belong in the file), and re-adds blueprint
//! masters. Treating that expected rewrite as corruption would surface a scary raw diff every
//! launch. So this module derives the user's INTENT from the recorded plugin state
//! (`store::list_plugin_state`, already `core::Plugin`) and classifies each on-disk delta:
//! a delta involving a protected / implicitly-active plugin is EXPECTED; anything else — a
//! user mod that vanished, an unexpected regular `.esp`, a reordered regular plugin — is REAL
//! [`ReconcileState::Drift`].
//!
//! This mirrors `deploy::verify`'s "recorded-is-truth, classify deltas, never mutate disk"
//! shape, but the source of truth is the store's plugin state (not the file manifest) and the
//! result is a CLASSIFIED verdict, never a raw text diff. It lives OUTSIDE `deploy::verify`'s
//! `Data/`-hash walk because `plugins.txt` is in the prefix AppData, not the deploy root.
//!
//! Path safety: on-disk lines are parsed as OPAQUE filenames — a name is
//! never joined as a path. The function is pure (no filesystem / libloot I/O), so it is
//! unit-testable with hand-written `plugins.txt` strings and synthetic recorded slices.

use std::collections::HashSet;

use nextwist_core::Plugin;

/// The SFLO-04 reconciliation verdict.
///
/// Serde repr is the DEFAULT external tagging the frontend depends on: the unit variant
/// [`ReconcileState::InSync`] serializes as the bare JSON string `"InSync"` (NOT
/// `{"InSync":null}`), and [`ReconcileState::Drift`] as `{"Drift":[...]}`. The frontend MUST
/// type this `"InSync" | { Drift: string[] }` — a `{InSync:null}` type would make the calm
/// in-sync branch silently never match at runtime.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReconcileState {
    /// On-disk `plugins.txt` differs from the recorded intent only by the game's expected
    /// on-launch rewrite (protected `.ccc` / stripped-implicit / re-added blueprint deltas).
    InSync,
    /// Beyond-expected deltas — the sorted, de-duplicated plugin names the user should see.
    Drift(Vec<String>),
}

/// Reconcile the on-disk `plugins.txt` against the recorded plugin state.
///
/// `recorded` is the store's per-profile plugin state (the user's INTENT). `on_disk_txt` is
/// the raw asterisk-format file the game may have rewritten. `protected` is the
/// libloot-derived implicitly-active set (from `loot::protected_plugins`) — the data-driven
/// EXPECTED set, so this self-corrects against the real game's implicit plugins rather than a
/// fixed name list (07-RESEARCH Assumptions A1/A2).
///
/// Returns [`ReconcileState::InSync`] when every delta is an expected on-launch rewrite of a
/// protected plugin, else [`ReconcileState::Drift`] with the sorted, unique offending names.
/// Errs toward classifying a protected/implicit delta as EXPECTED (avoid false alarms) while
/// still surfacing any user-mod delta as real drift.
pub fn reconcile_plugins_txt(
    recorded: &[Plugin],
    on_disk_txt: &str,
    protected: &HashSet<String>,
) -> ReconcileState {
    // On-disk ACTIVE plugins = the `*`-prefixed lines (opaque filenames), in file order.
    // All recorded-vs-on-disk matching is done on an ASCII-lowercased key (the
    // plugins.txt convention) so a pure CASE difference under Wine case-folding (`MyMod.esp`
    // vs `mymod.esp`) is NOT reported as drift; the original-cased name is kept for output.
    let on_disk: Vec<String> = parse_active_lines(on_disk_txt);
    let on_disk_set: HashSet<String> = on_disk.iter().map(|n| n.to_ascii_lowercase()).collect();

    // The user-intended active set = recorded enabled, EXCLUDING protected names (NexTwist
    // never writes implicit/protected plugins to the file — they are implicitly active).
    let recorded_active: Vec<&str> = recorded
        .iter()
        .filter(|p| p.enabled && !protected.contains(&p.name))
        .map(|p| p.name.as_str())
        .collect();
    let recorded_active_set: HashSet<String> = recorded_active
        .iter()
        .map(|n| n.to_ascii_lowercase())
        .collect();

    let mut drift: Vec<String> = Vec::new();

    // (1) On disk but NOT user-intended-active: expected iff protected (a `.ccc` entry, a
    //     re-added blueprint master); otherwise an unexpected plugin → real drift.
    for name in &on_disk {
        if recorded_active_set.contains(&name.to_ascii_lowercase()) {
            continue;
        }
        if !protected.contains(name) {
            drift.push(name.clone());
        }
    }

    // (2) User-intended-active but ABSENT from disk: since protected names were already
    //     excluded from `recorded_active`, any missing one is a user mod that vanished → drift.
    for name in &recorded_active {
        if !on_disk_set.contains(&name.to_ascii_lowercase()) {
            drift.push((*name).to_string());
        }
    }

    // (3) Relative-order divergence among the non-protected plugins present in BOTH: a
    //     reordered user `.esp` is real drift (the game does not reorder user mods). Compared
    //     case-insensitively so a case-only difference is not a false reorder.
    let disk_common: Vec<String> = on_disk
        .iter()
        .filter(|n| recorded_active_set.contains(&n.to_ascii_lowercase()))
        .map(|n| n.to_ascii_lowercase())
        .collect();
    let recorded_common: Vec<String> = recorded_active
        .iter()
        .filter(|n| on_disk_set.contains(&n.to_ascii_lowercase()))
        .map(|n| n.to_ascii_lowercase())
        .collect();
    if disk_common != recorded_common {
        for n in &recorded_active {
            if on_disk_set.contains(&n.to_ascii_lowercase())
                && !drift.iter().any(|d| d.eq_ignore_ascii_case(n))
            {
                drift.push((*n).to_string());
            }
        }
    }

    if drift.is_empty() {
        ReconcileState::InSync
    } else {
        drift.sort();
        drift.dedup();
        ReconcileState::Drift(drift)
    }
}

/// Parse the ACTIVE (`*`-prefixed) plugin names from an asterisk-format `plugins.txt` body.
///
/// Blank and `#`-comment lines are ignored; a leading `*` marks an active plugin and is
/// stripped. Non-`*` lines (present-but-inactive) are NOT active, so they are dropped. Each
/// name is an OPAQUE filename — never joined as a path.
fn parse_active_lines(txt: &str) -> Vec<String> {
    txt.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix('*'))
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}
