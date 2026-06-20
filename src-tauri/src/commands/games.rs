//! Game detection / add / list adapters — delegate to `steam` (resolution) and
//! `store` (persistence). No Steam/Proton-layout knowledge lives here; that is all in
//! `crates/steam`. These adapters only forward and persist the resolved `core::Game`.

use std::path::PathBuf;

use nextwist_core::Game;
use steam::DetectedGame;
use tauri::State;
use tokio::sync::Mutex;

use crate::commands::boundary_err;
use crate::state::AppState;

/// Auto-detect installed, supported Steam games (Skyrim SE / Fallout 4).
#[tauri::command]
pub async fn detect_games() -> Result<Vec<DetectedGame>, String> {
    steam::detect_games().map_err(boundary_err)
}

/// Resolve a detected game by AppID and persist it as managed.
#[tauri::command]
pub async fn add_game(state: State<'_, Mutex<AppState>>, appid: u32) -> Result<Game, String> {
    let game = steam::resolve_game(appid).map_err(boundary_err)?.into_game();
    state.lock().await.store.add_managed_game(&game).map_err(boundary_err)?;
    Ok(game)
}

/// Resolve a manually-picked game folder, validate its markers, and persist it.
#[tauri::command]
pub async fn add_game_by_folder(
    state: State<'_, Mutex<AppState>>,
    path: PathBuf,
    appid: u32,
) -> Result<Game, String> {
    let game = steam::add_game_by_folder(&path, appid).map_err(boundary_err)?.into_game();
    state.lock().await.store.add_managed_game(&game).map_err(boundary_err)?;
    Ok(game)
}

/// List every managed game from the registry.
#[tauri::command]
pub async fn list_games(state: State<'_, Mutex<AppState>>) -> Result<Vec<Game>, String> {
    state.lock().await.store.list_managed_games().map_err(boundary_err)
}
