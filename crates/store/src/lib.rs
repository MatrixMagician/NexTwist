//! `nextwist-store` — NexTwist persistence layer.
//!
//! A single SQLite database (rusqlite, bundled) under refinery migrations. The V1
//! tables are the reversible-deployment safety core; the V2 tables (Phase 2) add the
//! multi-mod / profile / plugin substrate:
//!
//!   * **game registry**  (ENV-03)    — [`Store::add_managed_game`] / [`Store::list_managed_games`]
//!   * **deploy manifest** (DEPLOY-02) — [`Store::record_deployed_file`] / [`Store::list_deployed_files`]
//!   * **operation journal** (DEPLOY-06) — [`Store::begin_op`] / [`Store::mark_done`] / [`Store::pending_ops`]
//!   * **vanilla backup ledger** (DEPLOY-04) — [`Store::record_vanilla`] / [`Store::vanilla_for`]
//!   * **mod registry** (D-01/D-13) — [`Store::add_mod`] / [`Store::list_mods`] / [`Store::set_mod_rank`]
//!   * **profiles + membership** (D-13/D-14) — [`Store::create_profile`] / [`Store::set_active_profile`] / [`Store::set_profile_mod`]
//!   * **plugin state** (D-07/D-13) — [`Store::set_plugin_state`] / [`Store::list_plugin_state`]
//!
//! Encapsulation invariant: NO `rusqlite` type appears in this crate's public API.
//! Downstream crates (`deploy`, `steam`, the Tauri shell) speak only in `core` types
//! and the small journal value types re-exported below. All SQL stays inside `store`.

mod db;
mod journal;
mod manifest;
mod mods;
mod plugins;
mod profiles;
mod registry;
mod vanilla;

pub use db::Store;
pub use journal::{JournalId, JournalRow, OpIntent};
