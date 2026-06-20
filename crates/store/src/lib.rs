//! `nextwist-store` — NexTwist persistence layer.
//!
//! A single SQLite database (rusqlite, bundled) under refinery migrations, holding
//! the four load-bearing tables for the reversible-deployment safety core:
//!
//!   * **game registry**  (ENV-03)    — [`Store::add_managed_game`] / [`Store::list_managed_games`]
//!   * **deploy manifest** (DEPLOY-02) — [`Store::record_deployed_file`] / [`Store::list_deployed_files`]
//!   * **operation journal** (DEPLOY-06) — [`Store::begin_op`] / [`Store::mark_done`] / [`Store::pending_ops`]
//!   * **vanilla backup ledger** (DEPLOY-04) — [`Store::record_vanilla`] / [`Store::vanilla_for`]
//!
//! Encapsulation invariant: NO `rusqlite` type appears in this crate's public API.
//! Downstream crates (`deploy`, `steam`, the Tauri shell) speak only in `core` types
//! and the small journal value types re-exported below. All SQL stays inside `store`.

mod db;
mod journal;
mod manifest;
mod registry;
mod vanilla;

pub use db::Store;
pub use journal::{JournalId, JournalRow, OpIntent};
