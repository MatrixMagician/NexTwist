//! NexTwist Tauri shell library: app builder + startup crash-recovery wiring.
//!
//! The shell is deliberately thin. The safety core lives in the headless crates
//! (`steam`/`extract`/`deploy`/`store`); this library only:
//! 1. resolves the OS app-data dir and opens the [`AppState`],
//! 2. runs `deploy::recover_on_launch` for every managed game BEFORE the UI is served
//!    (the DEPLOY-06 startup half — an interrupted prior op is recovered first), and
//! 3. registers the thin command adapters.

pub mod commands;
pub mod state;

use std::path::PathBuf;

use state::AppState;
use tauri::Manager;

/// Resolve the OS app-data directory NexTwist owns, falling back to a hidden home dir.
fn resolve_data_dir(app: &tauri::App) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from(".nextwist"))
}

/// Run `recover_on_launch` for every managed game so any interrupted prior operation is
/// replayed to a consistent state BEFORE the window is shown (DEPLOY-06 startup half).
///
/// This is intentionally NOT a `#[tauri::command]`: it is startup wiring, not UI-driven.
/// Recovery failures are logged, never fatal — the app still opens so the user can act.
fn recover_all_on_launch(state: &AppState) {
    let games = match state.store.list_managed_games() {
        Ok(games) => games,
        Err(e) => {
            tracing::error!(error = %e, "could not list managed games for startup recovery");
            return;
        }
    };
    for game in &games {
        match deploy::recover_on_launch(&state.store, game) {
            Ok(report) => tracing::info!(
                appid = game.appid,
                replayed = report.replayed,
                pristine = report.drift.pristine,
                "recover_on_launch complete"
            ),
            Err(e) => tracing::error!(appid = game.appid, error = %e, "recover_on_launch failed"),
        }
    }
}

/// Build and run the NexTwist desktop app.
pub fn run() {
    // Plain fmt subscriber (no env-filter feature needed); ignore a double-init in tests.
    let _ = tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).try_init();

    tauri::Builder::default()
        .setup(|app| {
            let data_dir = resolve_data_dir(app);
            let app_state = AppState::init(data_dir)?;
            // Crash-recovery BEFORE the UI is served (DEPLOY-06).
            recover_all_on_launch(&app_state);
            app.manage(tokio::sync::Mutex::new(app_state));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::games::detect_games,
            commands::games::add_game,
            commands::games::add_game_by_folder,
            commands::games::list_games,
            commands::mods::install_archive,
            commands::deploy::deploy,
            commands::deploy::purge,
            commands::deploy::verify,
            commands::conflicts::list_mods,
            commands::conflicts::list_conflicts,
            commands::conflicts::set_mod_rank,
            commands::conflicts::deploy_winner_set,
            commands::plugins::list_plugins,
            commands::plugins::set_plugin_enabled,
            commands::plugins::save_plugin_order,
            commands::plugins::sort_with_loot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NexTwist");
}
