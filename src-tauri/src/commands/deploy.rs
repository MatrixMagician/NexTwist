//! Deploy / purge / verify adapters — delegate to the `deploy` engine (CROWN JEWEL).
//!
//! Zero safety logic lives here: the engine owns the probe, the method ladder, the
//! intent-before-act journal, backup-before-overwrite, casing normalization, and the
//! pristine round-trip. These adapters look up the managed game and forward one call.

use deploy::{DeployReport, PurgeReport, StagedFiles, VerifyReport};
use extract::StagedMod;
use tauri::State;
use tokio::sync::Mutex;

use crate::commands::{boundary_err, require_game};
use crate::state::AppState;

/// Deploy a previously-staged mod into the managed game's `Data/` tree.
#[tauri::command]
pub async fn deploy(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    staged: StagedMod,
) -> Result<DeployReport, String> {
    let game = require_game(&state, appid).await?;
    let work = StagedFiles { staging_root: staged.staging_root, files: staged.files };
    deploy::deploy(&state.lock().await.store, &game, &work).map_err(boundary_err)
}

/// Purge the managed game back to byte-for-byte pristine (manifest-driven).
#[tauri::command]
pub async fn purge(state: State<'_, Mutex<AppState>>, appid: u32) -> Result<PurgeReport, String> {
    let game = require_game(&state, appid).await?;
    deploy::purge(&state.lock().await.store, &game).map_err(boundary_err)
}

/// Verify the managed game's deployment against the manifest (read-only drift report).
#[tauri::command]
pub async fn verify(state: State<'_, Mutex<AppState>>, appid: u32) -> Result<VerifyReport, String> {
    let game = require_game(&state, appid).await?;
    deploy::verify(&state.lock().await.store, &game).map_err(boundary_err)
}
