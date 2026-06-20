//! Database open + migration. The single place that touches `PRAGMA`s and refinery.
//!
//! Crash-safety model (CONTEXT.md, RESEARCH.md A7):
//! * `journal_mode = WAL` — ACID *inside* the DB; concurrent reads during writes.
//! * `synchronous = FULL` — durability for the operation-journal commit path. FULL
//!   (not NORMAL) is chosen so a `pending` op_journal row is on stable storage before
//!   the filesystem syscall runs, which is what makes crash replay sound.
//! * `foreign_keys = ON` — enforce referential intent where declared.
//!
//! SQLite WAL alone cannot make a `link()` syscall and its recording row atomic — that
//! gap is closed by the op_journal protocol (Plan 04). This module only guarantees the
//! DB side is durable.

use std::path::Path;

use core::StoreError;
use refinery::embed_migrations;
use rusqlite::Connection;

// Embeds every `Vn__*.sql` under `src/migrations/` into a `migrations::runner()`.
embed_migrations!("src/migrations");

/// A handle to the NexTwist persistence layer.
///
/// Wraps a single rusqlite [`Connection`]. All SQL is encapsulated behind the
/// module facades (`registry`, `manifest`, `journal`, `vanilla`); callers never
/// see a `rusqlite` type in this crate's public API.
pub struct Store {
    pub(crate) conn: Connection,
}

impl Store {
    /// Open (creating if absent) the DB at `db_path`, set the durability PRAGMAs,
    /// and apply all pending refinery migrations.
    pub fn open(db_path: &Path) -> Result<Store, StoreError> {
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|source| StoreError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }

        let mut conn = Connection::open(db_path).map_err(|e| StoreError::Db(e.to_string()))?;

        // WAL must be set before migrations so they run under the chosen journal mode.
        // `query_row` because journal_mode returns the new mode as a row.
        let mode: String = conn
            .query_row("PRAGMA journal_mode=WAL;", [], |row| row.get(0))
            .map_err(|e| StoreError::Db(e.to_string()))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(StoreError::Db(format!(
                "expected WAL journal mode, got '{mode}'"
            )));
        }

        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(|e| StoreError::Db(e.to_string()))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| StoreError::Db(e.to_string()))?;

        migrations::runner()
            .run(&mut conn)
            .map_err(|e| StoreError::Migration(e.to_string()))?;

        Ok(Store { conn })
    }

    /// Return the active SQLite journal mode (e.g. `"wal"`). Test/diagnostic helper.
    pub fn journal_mode(&self) -> Result<String, StoreError> {
        self.conn
            .query_row("PRAGMA journal_mode;", [], |row| {
                row.get::<_, String>(0)
            })
            .map(|m| m.to_ascii_lowercase())
            .map_err(|e| StoreError::Db(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn open_creates_db_in_wal_mode_with_tables() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("nextwist.db");
        let store = Store::open(&db).unwrap();

        assert!(db.exists(), "db file should be created");
        assert_eq!(store.journal_mode().unwrap(), "wal");

        // All four V1 tables must exist.
        for table in ["managed_game", "deployed_file", "op_journal", "vanilla_backup"] {
            let count: i64 = store
                .conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "table {table} should exist");
        }
    }

    #[test]
    fn op_journal_state_defaults_to_pending() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        store
            .conn
            .execute(
                "INSERT INTO op_journal (appid, target_rel, kind) VALUES (1, 'a/b.esp', 'deploy')",
                [],
            )
            .unwrap();
        let state: String = store
            .conn
            .query_row("SELECT state FROM op_journal LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(state, "pending");
    }

    #[test]
    fn open_is_idempotent_across_reopen() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("d.db");
        let _ = Store::open(&db).unwrap();
        // Re-opening should re-run zero migrations and still report WAL.
        let store = Store::open(&db).unwrap();
        assert_eq!(store.journal_mode().unwrap(), "wal");
    }
}
