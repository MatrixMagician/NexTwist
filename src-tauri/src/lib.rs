//! NexTwist Tauri shell library: app builder + startup crash-recovery wiring.
//!
//! The shell is deliberately thin. The safety core lives in the headless crates
//! (`steam`/`extract`/`deploy`/`store`); this library only:
//! 1. resolves the OS app-data dir and opens the [`AppState`],
//! 2. runs `deploy::recover_on_launch` for every managed game BEFORE the UI is served
//!    (the DEPLOY-06 startup half — an interrupted prior op is recovered first), and
//! 3. registers the thin command adapters.

pub mod auth;
pub mod commands;
pub mod keyring;
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

/// Report the result of the `nxm://` handler self-test (DIST-01 "self-test passes").
///
/// Strictly non-fatal (locked warn-and-continue decision, T-05-02): it consumes the
/// `Result` the deep-link plugin's `is_registered("nxm")` produces and logs PASS/WARN on
/// every arm, returning `()` regardless. It NEVER `?`-propagates, `unwrap`s, or `expect`s,
/// so a minimal distro lacking `xdg-mime` still lets the app open. Extracted from the
/// `setup` call site purely so it can be exercised headlessly (`tests/nxm_self_test.rs`)
/// without a live OS desktop session. Generic over the error so the test needs no
/// (non-constructible) plugin `Error`; the call site passes the plugin's own `Result<bool>`.
pub fn nxm_self_test<E: std::fmt::Display>(result: Result<bool, E>) {
    // Calls the plugin's own is_registered() upstream — do NOT hand-roll an xdg-mime query
    // (the plugin owns the desktop-file naming; reimplementing risks a filename mismatch).
    match result {
        Ok(true) => tracing::info!("nxm:// handler self-test: PASS"),
        Ok(false) => {
            tracing::warn!("nxm:// handler self-test: NexTwist is not the default handler")
        }
        Err(e) => tracing::warn!(error = %e, "nxm:// handler self-test: could not query xdg-mime"),
    }
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
        // OS-integration plugins (NXM-01). ORDER IS LOAD-BEARING: tauri-plugin-single-instance
        // MUST be registered BEFORE tauri-plugin-deep-link (RESEARCH Anti-Pattern) — on Linux,
        // with single-instance's `deep-link` feature, a second `nxm://` invocation while the app
        // is open is forwarded to the live instance and routed to `on_open_url` automatically
        // (never a duplicate window). Registering deep-link first would lose the forwarded URL.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second instance was launched (e.g. a browser `nxm://` click while we're open).
            // The forwarded URL reaches `on_open_url` via the deep-link feature; here we just
            // raise/focus the existing main window so the user sees the live instance react.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            let data_dir = resolve_data_dir(app);
            let app_state = AppState::init(data_dir)?;
            // Crash-recovery BEFORE the UI is served (DEPLOY-06).
            recover_all_on_launch(&app_state);
            app.manage(tokio::sync::Mutex::new(app_state));

            // Register the `nxm://` scheme + capture handler (NXM-01). On Linux this needs
            // `xdg-mime` + `update-desktop-database` on PATH for dev/installed-runtime
            // registration; in a shipped AppImage the plugin's `register_all()` reads
            // `$APPIMAGE` for a durable absolute `Exec=` path. Failures here are non-fatal —
            // the app still opens. The Phase-5 self-test below surfaces a WARN when NexTwist
            // is not the registered default (T-05-01), making a stale/hijacked handler observable.
            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(e) = app.deep_link().register_all() {
                    tracing::warn!(error = %e, "nxm:// deep-link registration failed (xdg-mime/update-desktop-database missing?)");
                }
                // Phase-5 self-test (DIST-01 "self-test passes"). Calls the plugin's own
                // is_registered() and reports PASS/WARN on every arm — strictly non-fatal so
                // a missing xdg-mime never aborts startup (locked warn-and-continue, T-05-02).
                nxm_self_test(app.deep_link().is_registered("nxm"));
                // Route every incoming `nxm://` URL through the thin shell router. ALL parsing
                // is in the headless `nexus::NxmLink::parse`; this closure only forwards. The
                // url is NEVER logged here (it may carry a key/expires/code — V7).
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        commands::nexus::handle_nxm_url(&handle, url.as_str());
                    }
                });
            }
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
            commands::profiles::list_profiles,
            commands::profiles::create_profile,
            commands::profiles::switch_profile,
            commands::profiles::delete_profile,
            commands::fomod::parse_fomod,
            commands::fomod::resolve_fomod,
            commands::fomod::apply_fomod,
            commands::nexus::login_with_api_key,
            commands::nexus::login_oauth_start,
            commands::nexus::logout,
            commands::nexus::account_info,
            commands::downloads::start_download,
            commands::downloads::cancel_download,
            commands::collections::resolve_collection,
            commands::collections::download_collection,
            commands::collections::deploy_collection,
            commands::collections::uninstall_collection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NexTwist");
}
