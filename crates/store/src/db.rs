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
        if let Some(parent) = db_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| StoreError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
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

        // All V1 + V2 tables must exist after a fresh open (runs V1 then V2).
        for table in [
            "managed_game",
            "deployed_file",
            "op_journal",
            "vanilla_backup",
            "managed_mod",
            "profile",
            "profile_mod",
            "plugin_state",
        ] {
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

    /// BLOCKING (T-02-01, D-16): the V2 migration must apply cleanly OVER a real
    /// Phase-1 (V1-only) DB and auto-create one active 'Default' profile per
    /// pre-existing managed_game — proving the data migration runs over existing rows
    /// and that V2 is additive (V1 tables + data survive).
    ///
    /// Test seam: refinery records applied migrations in its history table, so simply
    /// registering a game AFTER V2 has run would NOT retro-trigger the Default-profile
    /// INSERT. To exercise the upgrade path we deliberately reach a V1-ONLY state by
    /// running the refinery runner pinned to `Target::Version(1)`, seed a managed_game
    /// the way Phase-1 would have, then `Store::open` runs the full runner which sees
    /// V1 already applied and applies ONLY V2 (and its Default-profile INSERT) over the
    /// seeded V1 state — exactly the real-world Phase-1 → Phase-2 upgrade.
    #[test]
    fn v2_migrates_phase1_state() {
        use refinery::Target;
        use rusqlite::params;

        let dir = TempDir::new().unwrap();
        let db = dir.path().join("phase1.db");

        // --- Reach a genuine V1-only state via refinery (so its history is correct). ---
        {
            let mut conn = Connection::open(&db).unwrap();
            let _: String = conn
                .query_row("PRAGMA journal_mode=WAL;", [], |r| r.get(0))
                .unwrap();
            migrations::runner()
                .set_target(Target::Version(1))
                .run(&mut conn)
                .unwrap();

            // V2 tables must NOT exist yet (we are strictly at V1).
            let v2_count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master
                     WHERE type='table' AND name IN
                       ('managed_mod','profile','profile_mod','plugin_state')",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(v2_count, 0, "V2 tables must be absent in the V1-only state");

            // Seed a managed_game as Phase-1 would have persisted it.
            conn.execute(
                "INSERT INTO managed_game (appid, name, install_dir, prefix, staging_dir)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    489830,
                    "Skyrim Special Edition",
                    "/games/SkyrimSE",
                    "/games/compatdata/489830/pfx",
                    "/games/staging/489830"
                ],
            )
            .unwrap();
            // conn dropped here, closing the V1-only DB on disk.
        }

        // --- Now open via Store::open: refinery applies ONLY V2 over the V1 state. ---
        let store = Store::open(&db).unwrap();

        // (1) All four V2 tables now exist.
        for table in ["managed_mod", "profile", "profile_mod", "plugin_state"] {
            let count: i64 = store
                .conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "V2 table {table} should exist after upgrade");
        }

        // (2) Exactly one active 'Default' profile exists for the pre-existing game.
        let default = store.active_profile(489830).unwrap();
        let default = default.expect("Default profile should be created for the seeded game");
        assert_eq!(default.name, "Default");
        assert!(default.active);
        let profiles = store.list_profiles(489830).unwrap();
        assert_eq!(profiles.len(), 1, "exactly one Default profile per game");

        // (3) V2 is additive: the Phase-1 managed_game row survived untouched.
        assert_eq!(
            store.get_game(489830).unwrap().unwrap().name,
            "Skyrim Special Edition"
        );
    }

    /// V4 guard (NEXUS-03/06): reach a V3-only state, then apply V4 and confirm it ADDED
    /// `nexus_source` WITHOUT altering `managed_mod`'s columns (additive-only). Mirrors
    /// the `v2_migrates_phase1_state` test seam (refinery pinned to a target version).
    #[test]
    fn v4_adds_nexus_source_additively_over_v3() {
        use refinery::Target;

        let dir = TempDir::new().unwrap();
        let db = dir.path().join("v3.db");

        // Capture managed_mod's column set at V3 (before V4 runs).
        let cols_v3: Vec<String> = {
            let mut conn = Connection::open(&db).unwrap();
            let _: String = conn
                .query_row("PRAGMA journal_mode=WAL;", [], |r| r.get(0))
                .unwrap();
            migrations::runner()
                .set_target(Target::Version(3))
                .run(&mut conn)
                .unwrap();

            // nexus_source must NOT exist yet at V3.
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='nexus_source'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 0, "nexus_source must be absent at V3");

            let mut stmt = conn.prepare("PRAGMA table_info(managed_mod)").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .map(|c| c.unwrap())
                .collect::<Vec<_>>()
        };

        // Open via Store::open → refinery applies ONLY V4 over the V3 state.
        let store = Store::open(&db).unwrap();

        // V4 added nexus_source.
        let n: i64 = store
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='nexus_source'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "V4 must create nexus_source");

        // managed_mod's columns are UNCHANGED (V4 is additive, never ALTERs a safety table).
        let cols_v4: Vec<String> = {
            let mut stmt = store.conn.prepare("PRAGMA table_info(managed_mod)").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .map(|c| c.unwrap())
                .collect()
        };
        assert_eq!(cols_v3, cols_v4, "V4 must not alter managed_mod's columns");
    }
}
