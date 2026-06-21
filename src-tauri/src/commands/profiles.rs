//! Profile-slice adapters (PROF-01/02/03) — delegate to the store (create/list/delete)
//! and the headless `deploy::switch_profile` reconcile (switch). Zero business logic:
//! each command looks up the game and forwards exactly ONE store/headless call, mapping
//! the typed error to a `String` at the IPC boundary.
//!
//! A profile owns no files — it is a lightweight reference set over the shared staging
//! store (D-13/D-14). `switch_profile` reconciles the on-disk deployment through the
//! UNCHANGED safe engine (purge old → deploy new winner set → write new plugins.txt →
//! mark active); it ALREADY writes the target profile's `plugins.txt` internally (the
//! Task-1 wiring is deploy → loadorder direct), so this adapter does not re-apply it.
//! `delete_profile` removes only the profile + its selections; staged mod files are kept
//! (D-14, shared staging).

use deploy::SwitchReport;
use nextwist_core::Profile;
use tauri::State;
use tokio::sync::Mutex;

use crate::commands::{boundary_err, require_game};
use crate::state::AppState;

/// List all profiles for a game (PROF-01) — the data source for the §D profile selector.
/// A single `store.list_profiles` read.
#[tauri::command]
pub async fn list_profiles(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
) -> Result<Vec<Profile>, String> {
    require_game(&state, appid).await?;
    state.lock().await.store.list_profiles(appid).map_err(boundary_err)
}

/// Create a new (inactive) profile for a game and return it (PROF-01). The store assigns
/// the row id; a duplicate name per game surfaces the store's UNIQUE error verbatim.
#[tauri::command]
pub async fn create_profile(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    name: String,
) -> Result<Profile, String> {
    require_game(&state, appid).await?;
    let guard = state.lock().await;
    let id = guard.store.create_profile(appid, &name).map_err(boundary_err)?;
    Ok(Profile { id, appid, name, active: false })
}

/// Switch the active profile (PROF-02), reconciling the deployment through the safe
/// engine: `deploy::switch_profile` purges the current deployment to pristine, deploys
/// the target profile's winner set, writes its `plugins.txt`, and marks it active. This
/// is the disk-mutating action the UI gates behind a confirmation modal (UI-SPEC §D.2).
#[tauri::command]
pub async fn switch_profile(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    profile_id: i64,
) -> Result<SwitchReport, String> {
    let game = require_game(&state, appid).await?;
    deploy::switch_profile(&state.lock().await.store, &game, profile_id).map_err(boundary_err)
}

/// Delete a profile and its mod/plugin selections (PROF-01). Staged mod files are KEPT
/// (D-14: only the profile + its references are removed). Idempotent: a missing id
/// returns `false`.
#[tauri::command]
pub async fn delete_profile(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    profile_id: i64,
) -> Result<bool, String> {
    require_game(&state, appid).await?;
    state.lock().await.store.delete_profile(profile_id).map_err(boundary_err)
}
