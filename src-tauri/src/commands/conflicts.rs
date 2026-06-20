//! Conflict-slice adapters (CONF-01/02/03) — delegate to the headless `deploy::conflict`
//! resolver and the unchanged safe engine. Zero business logic: each command looks up
//! the game, reads the enabled mod set, and forwards ONE resolve / deploy call.
//!
//! The resolver is a pure fold (`deploy::conflict::resolve`); the winner-set deploy goes
//! through `deploy::deploy_winners`, which reuses the same journaled per-file primitive
//! as Phase-1 `deploy` (the safe engine is never bypassed). `set_mod_rank` only persists
//! the new priority — it does NOT deploy (D-04: rank changes are pending until Deploy).

use deploy::{conflict, DeployReport, ModInput};
use nextwist_core::{FileConflict, ManagedMod};
use tauri::State;
use tokio::sync::Mutex;

use crate::commands::{boundary_err, require_game};
use crate::state::AppState;

/// Build the enabled-mod [`ModInput`] set for a game from the store (shared by the
/// list-conflicts and deploy-winner-set adapters). Not business logic — a single
/// `list_mods` read mapped to the resolver's input shape.
async fn enabled_mod_inputs(
    state: &State<'_, Mutex<AppState>>,
    appid: u32,
) -> Result<Vec<ModInput>, String> {
    let mods = state.lock().await.store.list_mods(appid).map_err(boundary_err)?;
    Ok(mods
        .into_iter()
        .filter(|m| m.enabled)
        .map(|m| ModInput { mod_id: m.id, staging_root: m.staging_root, rank: m.rank })
        .collect())
}

/// List a game's managed mods in priority (rank-ascending) order — the data source for
/// the Conflict view's priority list (UI-SPEC §A.1) and for mapping winner/provider mod
/// ids to names in the conflict table. A single `list_mods` read.
#[tauri::command]
pub async fn list_mods(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
) -> Result<Vec<ManagedMod>, String> {
    require_game(&state, appid).await?;
    state.lock().await.store.list_mods(appid).map_err(boundary_err)
}

/// List the file-level conflicts among a game's ENABLED mods (CONF-01): one entry per
/// contested `target_rel`, naming every provider and the priority winner.
#[tauri::command]
pub async fn list_conflicts(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
) -> Result<Vec<FileConflict>, String> {
    require_game(&state, appid).await?;
    let inputs = enabled_mod_inputs(&state, appid).await?;
    let (_winners, conflicts) = conflict::resolve(&inputs).map_err(boundary_err)?;
    Ok(conflicts)
}

/// Set a mod's deployment rank (CONF-02). Persists only — the change is PENDING until
/// the user explicitly deploys (D-04); this command never touches disk.
#[tauri::command]
pub async fn set_mod_rank(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    mod_id: i64,
    rank: u32,
) -> Result<bool, String> {
    require_game(&state, appid).await?;
    state.lock().await.store.set_mod_rank(mod_id, rank).map_err(boundary_err)
}

/// Resolve the enabled-mod winner set and deploy it through the safe engine (CONF-03):
/// the deterministic, deduped (one owner per path) winners are applied via
/// `deploy::deploy_winners`.
#[tauri::command]
pub async fn deploy_winner_set(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
) -> Result<DeployReport, String> {
    let game = require_game(&state, appid).await?;
    let inputs = enabled_mod_inputs(&state, appid).await?;
    let (winners, _conflicts) = conflict::resolve(&inputs).map_err(boundary_err)?;
    deploy::deploy_winners(&state.lock().await.store, &game, &winners).map_err(boundary_err)
}
