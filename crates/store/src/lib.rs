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
//! The V4 migration (Phase 3) adds Nexus provenance additively:
//!   * **nexus provenance** (NEXUS-03/06) — [`Store::add_nexus_source`] / [`Store::get_nexus_source`]
//!
//! The V5 migration (Phase 4) adds the Collection acquisition substrate additively:
//!   * **collections** (COLL-01/02) — [`Store::add_collection`] / [`Store::get_collection`]
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
