//! Mod-install adapter — delegate to `extract` (validated staging) over the managed
//! game's staging dir. All archive validation (zip-slip / `..` / symlink rejection,
//! format detection, read-only locking) lives in `crates/extract`; this adapter only
//! looks up the game's staging dir and forwards the archive path.

use std::path::PathBuf;

use extract::StagedMod;
use tauri::State;
use tokio::sync::Mutex;

use crate::commands::{boundary_err, require_game};
use crate::state::AppState;

/// Install a local archive into the managed game's staging tree, returning the
/// validated [`StagedMod`] the UI hands back to `deploy`.
#[tauri::command]
pub async fn install_archive(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    archive: PathBuf,
) -> Result<StagedMod, String> {
    let game = require_game(&state, appid).await?;
    extract::install_archive(&archive, &game.staging_dir).map_err(boundary_err)
}
