//! Plugin / LOOT adapters (PLUGIN-01/02/03) — delegate to the headless `loadorder` crate.
//!
//! Zero safety/format logic lives here: `loadorder` owns the scan, the asterisk
//! plugins.txt write, the masterlist fetch, and the LOOT sort (all via libloot). These
//! adapters look up the managed game + active profile, read the enabled mods' staging
//! roots, and forward exactly one headless call, mapping the typed error to a `String` at
//! the IPC boundary.
//!
//! The active-state/order source of truth is the per-profile `plugin_state` store table
//! (D-07/D-13); `plugins.txt` in the Proton prefix is DERIVED from it (regenerable; not
//! the pristine invariant — RESEARCH OQ3). `list_plugins` MERGES the on-disk scan
//! (filenames + ESM/ESL/ESP badges) with the stored enable/order per profile.

use nextwist_core::Plugin;
use tauri::State;
use tokio::sync::{Mutex, MutexGuard};

use crate::commands::{boundary_err, require_game};
use crate::state::AppState;

/// Build the merged plugin view for a game using an ALREADY-HELD state guard (WR-03).
///
/// Identical merge to [`merged_plugins`] but it performs every store read (active profile,
/// scan roots, stored state) and the on-disk scan under the SAME lock the caller holds, so
/// a read-modify-write (e.g. [`set_plugin_enabled`]) is atomic against concurrent plugin
/// ops — no lost update / kind-order disagreement from a released-then-reacquired lock.
fn merged_plugins_locked(
    guard: &MutexGuard<'_, AppState>,
    appid: u32,
) -> Result<Vec<Plugin>, String> {
    let game = guard
        .store
        .get_game(appid)
        .map_err(boundary_err)?
        .ok_or_else(|| format!("game {appid} is not managed"))?;
    let game_id =
        loadorder::esplugin_game_id(appid).ok_or_else(|| format!("game {appid} is not supported"))?;
    let roots: Vec<std::path::PathBuf> = guard
        .store
        .list_mods(appid)
        .map_err(boundary_err)?
        .into_iter()
        .filter(|m| m.enabled)
        .map(|m| m.staging_root)
        .collect();
    let data_dir = game.install_dir.join("Data");
    let scanned =
        loadorder::scan_plugins_for(game_id, &roots, &data_dir).map_err(boundary_err)?;

    let profile_id = guard
        .store
        .active_profile(appid)
        .map_err(boundary_err)?
        .map(|p| p.id)
        .ok_or_else(|| format!("game {appid} has no active profile"))?;
    let stored = guard.store.list_plugin_state(profile_id).map_err(boundary_err)?;

    let mut merged: Vec<Plugin> = scanned
        .into_iter()
        .map(|mut p| {
            if let Some(s) = stored.iter().find(|s| s.name == p.name) {
                p.enabled = s.enabled;
                p.order = s.order;
            }
            p
        })
        .collect();
    merged.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.name.cmp(&b.name)));
    Ok(merged)
}

/// Resolve the active profile id for a game, or a clear boundary error if none is set.
/// Plugin enable/order is per-profile, so every plugin op needs the active profile.
async fn active_profile_id(
    state: &State<'_, Mutex<AppState>>,
    appid: u32,
) -> Result<i64, String> {
    state
        .lock()
        .await
        .store
        .active_profile(appid)
        .map_err(boundary_err)?
        .map(|p| p.id)
        .ok_or_else(|| format!("game {appid} has no active profile"))
}

/// Merge the on-disk plugin scan (filenames + ESM/ESL/ESP badges) with the per-profile
/// stored enable/order. The scan owns the type badge and which plugins exist; the store
/// owns enabled/order. A scanned plugin with no stored row defaults to disabled/order 0;
/// the result is ordered by stored order then name for a stable list.
async fn merged_plugins(
    state: &State<'_, Mutex<AppState>>,
    appid: u32,
) -> Result<Vec<Plugin>, String> {
    // WR-03: acquire the state lock once and build the entire merged view (all store reads
    // + the scan) under it, so the scan and the stored-state read it is merged with are a
    // consistent snapshot rather than two separately-locked reads with a scan between.
    let guard = state.lock().await;
    merged_plugins_locked(&guard, appid)
}

/// List a game's plugins (PLUGIN-01 discovery): the enabled mods' + game `Data/` plugins,
/// ESM/ESL/ESP-badged, merged with the active profile's stored enable/order.
#[tauri::command]
pub async fn list_plugins(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
) -> Result<Vec<Plugin>, String> {
    merged_plugins(&state, appid).await
}

/// Enable/disable a single plugin (PLUGIN-01). Persists to the active profile's
/// `plugin_state` only — writing `plugins.txt` happens on `save_plugin_order` (the UI sends
/// the full desired list there). The plugin's kind/order are taken from the current merged
/// list so the stored row stays consistent with the scan.
#[tauri::command]
pub async fn set_plugin_enabled(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    name: String,
    enabled: bool,
) -> Result<(), String> {
    // WR-03: hold the state lock for the WHOLE read-modify-write (resolve active profile,
    // build the merged view, toggle, persist) so the row written cannot be stale relative
    // to a concurrent plugin op. The on-disk scan inside the merge runs under the lock too
    // — on a single-user desktop app the brief extra hold is worth the atomicity.
    let guard = state.lock().await;
    let profile_id = guard
        .store
        .active_profile(appid)
        .map_err(boundary_err)?
        .map(|p| p.id)
        .ok_or_else(|| format!("game {appid} has no active profile"))?;
    let merged = merged_plugins_locked(&guard, appid)?;
    let mut plugin = merged
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("plugin '{name}' not found for game {appid}"))?;
    plugin.enabled = enabled;
    guard.store.set_plugin_state(profile_id, &plugin).map_err(boundary_err)
}

/// Persist a plugin load order (PLUGIN-02) and write the asterisk `plugins.txt` at the
/// Proton-prefix AppData location via libloot (masters-first enforced internally).
///
/// `order` is the full desired plugin list (name/kind/enabled/order) in the user's chosen
/// order; the index in the vector becomes the stored order. Writes `plugins.txt` FIRST,
/// then persists every row to `plugin_state` only after the file write succeeds. On a
/// write failure the libloot reason is surfaced verbatim for the UI-SPEC plugins.txt error
/// copy.
///
/// WR-05: the file write precedes the DB persist so a libloot/IO failure leaves the DB
/// UNTOUCHED — matching the user's "nothing was saved" mental model when the command
/// returns an error. (Writing the DB first would record the new order while the on-disk
/// `plugins.txt` was never written, leaving the persisted state and the prefix disagreeing
/// after a failure.)
#[tauri::command]
pub async fn save_plugin_order(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    order: Vec<Plugin>,
) -> Result<std::path::PathBuf, String> {
    let game = require_game(&state, appid).await?;
    let profile_id = active_profile_id(&state, appid).await?;

    // 1. Write plugins.txt at the prefix AppData via libloot (masters-first; D-08). Doing
    //    this FIRST means a failure here leaves the DB untouched (WR-05: nothing saved).
    let folder = loadorder::appdata_folder_name(appid)
        .ok_or_else(|| format!("game {appid} is not supported"))?;
    let appdata_local = loadorder::appdata_local_path(&game.prefix, folder);
    let written = loadorder::apply_load_order(appid, &game.install_dir, &appdata_local, &order)
        .map_err(boundary_err)?;

    // 2. Only AFTER the file write succeeded, persist each plugin's enable/order to the
    //    active profile (the index = order), under one lock for a consistent snapshot.
    {
        let guard = state.lock().await;
        for (idx, p) in order.iter().enumerate() {
            let row = Plugin {
                name: p.name.clone(),
                kind: p.kind,
                enabled: p.enabled,
                order: idx as u32,
            };
            guard.store.set_plugin_state(profile_id, &row).map_err(boundary_err)?;
        }
    }

    Ok(written)
}

/// Propose a LOOT-sorted order (PLUGIN-03, D-12) — returns the proposed order + critical
/// warnings WITHOUT writing. The UI reviews it and calls `save_plugin_order` only on
/// confirm (no silent apply).
#[tauri::command]
pub async fn sort_with_loot(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
) -> Result<loadorder::SortProposal, String> {
    let game = require_game(&state, appid).await?;
    let app_data = state.lock().await.data_dir.clone();
    let plugins = merged_plugins(&state, appid).await?;
    let folder = loadorder::appdata_folder_name(appid)
        .ok_or_else(|| format!("game {appid} is not supported"))?;
    let appdata_local = loadorder::appdata_local_path(&game.prefix, folder);
    loadorder::propose_sort(appid, &game.install_dir, &appdata_local, &app_data, &plugins)
        .map_err(boundary_err)
}
