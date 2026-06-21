//! `nextwist-loadorder` — headless plugin / load-order management via libloot.
//!
//! This crate is the **Linux seam** for plugin management. libloot (the LOOT
//! project's pure-Rust crate) cannot derive the AppData/Local plugins.txt location
//! on Linux — on non-Windows it returns `NoLocalAppData` for the Bethesda games that
//! need an AppData folder (Pitfall 1). The real location lives inside the Proton
//! prefix, which only NexTwist knows. So this crate **ALWAYS** constructs the game
//! with `Game::with_local_path`, supplying
//! `<prefix>/drive_c/users/steamuser/AppData/Local/<GameName>` — never `Game::new`.
//!
//! Tauri-free and headless: it compiles and unit/spike-tests in CI without a webview.
//! The full plugin manager (plugin scan, masterlist fetch, "Sort with LOOT", profile
//! apply) builds on this wrapper in Plan 04 — this plan (02-02) only de-risks the
//! `with_local_path → load → set_load_order → save` round-trip (RESEARCH A1/A3).

pub mod error;
pub mod loot;
pub mod masterlist;
pub mod scan;

pub use error::LoadOrderError;
pub use loot::{apply_load_order, masters_first_order};
pub use masterlist::ensure_masterlist;
pub use scan::{esplugin_game_id, scan_plugins, scan_plugins_for};
