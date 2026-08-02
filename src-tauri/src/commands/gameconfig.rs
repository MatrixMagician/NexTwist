//! Game-config (StarfieldCustom.ini activation) adapters — delegate to the `deploy`
//! engine's reversible INI op wrappers (Plan 08-01).
//!
//! Zero safety logic lives here: the engine owns path resolution (via `steam::my_games_path`),
//! `drive_c` containment, foreign-symlink refusal, the surgical byte-fidelity merge, the
//! intent-before-act journal, three-valued provenance backup, and the typed conflict.
//! These adapters look up the managed game and forward one call.

use deploy::{IniActivationPreview, IniConflictResolution, IniOutcome};
use tauri::State;
use tokio::sync::Mutex;

use crate::commands::{boundary_err, require_game};
use crate::state::AppState;

/// Read-only preview of what loose-file activation WOULD do — writes nothing (SFINI-01).
#[tauri::command]
pub async fn preview_ini_activation(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
) -> Result<IniActivationPreview, String> {
    let game = require_game(&state, appid).await?;
    deploy::preview_ini_activation(&game).map_err(boundary_err)
}

/// Activate loose-file loading, resolving a pre-existing user value per `resolution` (SFINI-03).
#[tauri::command]
pub async fn apply_ini_activation(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    resolution: IniConflictResolution,
) -> Result<IniOutcome, String> {
    let game = require_game(&state, appid).await?;
    deploy::ensure_ini_active(&state.lock().await.store, &game, resolution).map_err(boundary_err)
}
