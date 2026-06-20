//! Thin Tauri command adapters — the ONLY job of this layer is to cross the IPC
//! boundary and delegate to the headless safety core.
//!
//! Anti-Pattern 4 (RESEARCH.md / ARCHITECTURE.md): no business logic, no file loops,
//! no path resolution lives here. Every `#[tauri::command]` below: locks the shared
//! state, calls exactly one headless-crate function, maps the typed error to a
//! `String` at the boundary (the webview only speaks JSON/strings), and returns. All
//! validation and safety logic lives in the (tested) `steam`/`extract`/`deploy` crates.

pub mod conflicts;
pub mod deploy;
pub mod games;
pub mod mods;

use tauri::State;
use tokio::sync::Mutex;

use crate::state::AppState;

/// Map any headless-crate error into the `String` the IPC boundary returns. Keeping
/// this in one place ensures every adapter maps errors identically (and trivially).
pub(crate) fn boundary_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Look up a managed game by AppID (a single `store.get_game` call) or surface a clear
/// "not managed" boundary error. Shared by the mods/deploy adapters so neither inlines
/// a store lookup; this is not business logic, just the registry read every op needs.
pub(crate) async fn require_game(
    state: &State<'_, Mutex<AppState>>,
    appid: u32,
) -> Result<nextwist_core::Game, String> {
    state
        .lock()
        .await
        .store
        .get_game(appid)
        .map_err(boundary_err)?
        .ok_or_else(|| format!("game {appid} is not managed"))
}
