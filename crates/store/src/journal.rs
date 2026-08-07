//! Write-ahead operation journal: the `op_journal` table facade.
//!
//! This module provides ONLY the durable row primitives. The intent-before-act
//! protocol and idempotent replay/rollback live in the deploy crate:
//!
//! 1. `begin_op(intent)` — insert a `pending` row, returns its id (COMMIT it).
//! 2. perform the idempotent filesystem syscall.
//! 3. `mark_done(id)` — flip the row to `done`.
//!
//! On launch, the deploy engine queries `pending_ops()` and rolls each forward or
//! back. Because file ops are idempotent, replaying a half-done op is always safe.

use std::path::PathBuf;

use core::{DeployMethod, StoreError};
use rusqlite::params;

use crate::db::Store;

/// Opaque journal-row identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalId(pub i64);

/// The declared intent of a single filesystem operation, recorded before it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpIntent {
    /// Game this op belongs to.
    pub appid: u32,
    /// Target path relative to the deploy root.
    pub target_rel: PathBuf,
    /// Planned deploy method (None for pure-delete ops such as purge).
    pub method: Option<DeployMethod>,
    /// blake3 hash of the source content (None where not applicable).
    pub source_hash: Option<String>,
    /// Operation kind, e.g. `"deploy"` or `"purge"`.
    pub kind: String,
    /// Root of the staged tree this file came from (deploy ops only).
    ///
    /// Recovery reconstructs the staged source as `staging_root/target_rel`, so the root
    /// the caller actually used has to be durable — a mod staged in a per-mod subdir
    /// (what production does) is not findable from the game's staging dir alone.
    pub staging_root: Option<PathBuf>,
}

/// A persisted journal row read back from the DB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalRow {
    /// Row id.
    pub id: JournalId,
    /// Game this op belongs to.
    pub appid: u32,
    /// Target path relative to the deploy root.
    pub target_rel: PathBuf,
    /// Recorded deploy method, if any.
    pub method: Option<DeployMethod>,
    /// Recorded source hash, if any.
    pub source_hash: Option<String>,
    /// Operation kind.
    pub kind: String,
    /// Current state (`"pending"` or `"done"`).
    pub state: String,
    /// Recorded staging root, if the op carried one. `None` for purge/INI ops and for
    /// deploy rows written before the column existed.
    pub staging_root: Option<PathBuf>,
}

impl Store {
    /// Record an operation intent as a `pending` row and return its id.
    pub fn begin_op(&self, intent: &OpIntent) -> Result<JournalId, StoreError> {
        self.conn
            .execute(
                "INSERT INTO op_journal
                     (appid, target_rel, method, source_hash, kind, state, staging_root)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
                params![
                    intent.appid,
                    intent.target_rel.to_string_lossy(),
                    intent.method.map(|m| m.as_str()),
                    intent.source_hash,
                    intent.kind,
                    intent
                        .staging_root
                        .as_ref()
                        .map(|p| p.to_string_lossy().into_owned()),
                ],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(JournalId(self.conn.last_insert_rowid()))
    }

    /// Flip a journal row to `done`. Idempotent: marking an already-done or missing
    /// row simply affects zero rows.
    pub fn mark_done(&self, id: JournalId) -> Result<(), StoreError> {
        self.conn
            .execute(
                "UPDATE op_journal SET state = 'done' WHERE id = ?1",
                params![id.0],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(())
    }

    /// All journal rows not yet `done`, oldest first — the crash-recovery work list.
    pub fn pending_ops(&self) -> Result<Vec<JournalRow>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, appid, target_rel, method, source_hash, kind, state, staging_root
                 FROM op_journal WHERE state != 'done' ORDER BY id",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let rows = stmt
            .query_map([], row_to_journal)
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| StoreError::Db(e.to_string()))?);
        }
        Ok(out)
    }
}

fn row_to_journal(row: &rusqlite::Row<'_>) -> rusqlite::Result<JournalRow> {
    let method_tok: Option<String> = row.get(3)?;
    Ok(JournalRow {
        id: JournalId(row.get(0)?),
        appid: row.get(1)?,
        target_rel: PathBuf::from(row.get::<_, String>(2)?),
        method: method_tok.and_then(|t| DeployMethod::from_token(&t)),
        source_hash: row.get(4)?,
        kind: row.get(5)?,
        state: row.get(6)?,
        staging_root: row.get::<_, Option<String>>(7)?.map(PathBuf::from),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn intent() -> OpIntent {
        OpIntent {
            appid: 489830,
            target_rel: PathBuf::from("Data/skse.esp"),
            method: Some(DeployMethod::Reflink),
            source_hash: Some("deadbeef".into()),
            kind: "deploy".into(),
            staging_root: Some(PathBuf::from("/app/staging/489830/SomeMod")),
        }
    }

    #[test]
    fn begin_then_pending_then_done() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();

        let id = store.begin_op(&intent()).unwrap();

        let pending = store.pending_ops().unwrap();
        assert_eq!(pending.len(), 1);
        let row = &pending[0];
        assert_eq!(row.id, id);
        assert_eq!(row.state, "pending");
        assert_eq!(row.method, Some(DeployMethod::Reflink));
        assert_eq!(row.source_hash.as_deref(), Some("deadbeef"));
        assert_eq!(row.kind, "deploy");
        assert_eq!(
            row.staging_root.as_deref(),
            Some(Path::new("/app/staging/489830/SomeMod")),
            "the staging root must round-trip, or recovery cannot locate the staged source"
        );

        store.mark_done(id).unwrap();
        assert!(store.pending_ops().unwrap().is_empty());
    }

    #[test]
    fn mark_done_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let id = store.begin_op(&intent()).unwrap();
        store.mark_done(id).unwrap();
        // Marking again, or a non-existent id, must not error.
        store.mark_done(id).unwrap();
        store.mark_done(JournalId(99999)).unwrap();
        assert!(store.pending_ops().unwrap().is_empty());
    }

    #[test]
    fn pending_ops_preserves_insertion_order() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let a = store.begin_op(&intent()).unwrap();
        let mut second = intent();
        second.target_rel = PathBuf::from("Data/other.esp");
        let b = store.begin_op(&second).unwrap();
        let pending = store.pending_ops().unwrap();
        assert_eq!(pending[0].id, a);
        assert_eq!(pending[1].id, b);
    }
}
