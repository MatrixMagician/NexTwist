//! `nextwist-store` — NexTwist persistence layer.
//!
//! A single SQLite database (rusqlite, bundled) under refinery migrations. The V1
//! tables are the reversible-deployment safety core; the V2 tables add the
//! multi-mod / profile / plugin substrate:
//!
//!   * **game registry**     — [`Store::add_managed_game`] / [`Store::list_managed_games`]
//!   * **deploy manifest** — [`Store::record_deployed_file`] / [`Store::list_deployed_files`]
//!   * **operation journal** — [`Store::begin_op`] / [`Store::mark_done`] / [`Store::pending_ops`]
//!   * **vanilla backup ledger** — [`Store::record_vanilla`] / [`Store::vanilla_for`]
//!   * **mod registry** — [`Store::add_mod`] / [`Store::list_mods`] / [`Store::set_mod_rank`]
//!   * **profiles + membership** — [`Store::create_profile`] / [`Store::set_active_profile`] / [`Store::set_profile_mod`]
//!   * **plugin state** — [`Store::set_plugin_state`] / [`Store::list_plugin_state`]
//!
//! The V4 migration adds Nexus provenance additively:
//!   * **nexus provenance** — [`Store::add_nexus_source`] / [`Store::get_nexus_source`]
//!
//! The V5 migration adds the Collection acquisition substrate additively:
//!   * **collections** — [`Store::add_collection`] / [`Store::get_collection`]
//!     / [`Store::add_collection_mod`] / [`Store::list_collection_mods`]
//!
//! Encapsulation invariant: NO `rusqlite` type appears in this crate's public API.
//! Downstream crates (`deploy`, `steam`, the Tauri shell) speak only in `core` types
//! and the small journal value types re-exported below. All SQL stays inside `store`.

mod collections;
mod db;
mod journal;
mod manifest;
mod mods;
mod nexus;
mod plugins;
mod profiles;
mod registry;
mod vanilla;

pub use db::Store;
pub use journal::{JournalId, JournalRow, OpIntent};
