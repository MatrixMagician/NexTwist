//! Per-file deploy manifest (DEPLOY-02): the `deployed_file` table facade.
//!
//! Each [`core::FileEntry`] row records one file NexTwist placed into a game's
//! deploy tree — what it is, how it was placed, its content hash, and whether it
//! overwrote a pre-existing vanilla file (which would then have a `vanilla_backup`
//! row). Purge (Plan 05) reads this manifest to remove exactly what was deployed.

use std::path::{Path, PathBuf};

use core::{DeployMethod, FileEntry, StoreError};
use rusqlite::params;

use crate::db::Store;

impl Store {
    /// Record (or replace) a deployed file for a game. Keyed by (appid, target_rel).
    pub fn record_deployed_file(&self, appid: u32, entry: &FileEntry) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO deployed_file
                   (appid, target_rel, source_mod, method, hash, pre_existing)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    appid,
                    entry.target_rel.to_string_lossy(),
                    entry.source_mod,
                    entry.method.as_str(),
                    entry.hash,
                    entry.pre_existing as i64,
                ],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(())
    }

    /// List every deployed file for a game, ordered by target path.
    pub fn list_deployed_files(&self, appid: u32) -> Result<Vec<FileEntry>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT target_rel, source_mod, method, hash, pre_existing
                 FROM deployed_file WHERE appid = ?1 ORDER BY target_rel",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let rows = stmt
            .query_map(params![appid], row_to_entry)
            .map_err(|e| StoreError::Db(e.to_string()))?;

        let mut out = Vec::new();
        for r in rows {
            // Outer Result = rusqlite row error; inner Result = domain decode error.
            let entry = r.map_err(map_row_err)??;
            out.push(entry);
        }
        Ok(out)
    }

    /// Remove a deployed-file row by (appid, target_rel). Idempotent: removing a
    /// missing row is a no-op (returns `false`), matching the idempotent-op model.
    pub fn remove_deployed_file(
        &self,
        appid: u32,
        target_rel: &Path,
    ) -> Result<bool, StoreError> {
        let n = self
            .conn
            .execute(
                "DELETE FROM deployed_file WHERE appid = ?1 AND target_rel = ?2",
                params![appid, target_rel.to_string_lossy()],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(n > 0)
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<FileEntry, StoreError>> {
    let method_tok: String = row.get(2)?;
    let method = match DeployMethod::from_token(&method_tok) {
        Some(m) => m,
        None => {
            return Ok(Err(StoreError::Corrupt(format!(
                "unknown deploy method '{method_tok}'"
            ))));
        }
    };
    Ok(Ok(FileEntry {
        target_rel: PathBuf::from(row.get::<_, String>(0)?),
        source_mod: row.get(1)?,
        method,
        hash: row.get(3)?,
        pre_existing: row.get::<_, i64>(4)? != 0,
    }))
}

/// Flatten a `rusqlite::Result<Result<FileEntry, StoreError>>` row into a `StoreError`.
fn map_row_err(e: rusqlite::Error) -> StoreError {
    StoreError::Db(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn entry(rel: &str) -> FileEntry {
        FileEntry {
            target_rel: PathBuf::from(rel),
            source_mod: 7,
            method: DeployMethod::Hardlink,
            hash: "abc123".into(),
            pre_existing: true,
        }
    }

    #[test]
    fn record_list_remove_cycle() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();

        store.record_deployed_file(489830, &entry("Data/a.esp")).unwrap();
        store.record_deployed_file(489830, &entry("Data/b.esp")).unwrap();

        let files = store.list_deployed_files(489830).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].target_rel, PathBuf::from("Data/a.esp"));
        assert_eq!(files[0].method, DeployMethod::Hardlink);
        assert!(files[0].pre_existing);

        assert!(store.remove_deployed_file(489830, Path::new("Data/a.esp")).unwrap());
        // Removing again is a no-op.
        assert!(!store.remove_deployed_file(489830, Path::new("Data/a.esp")).unwrap());

        let files = store.list_deployed_files(489830).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].target_rel, PathBuf::from("Data/b.esp"));
    }

    #[test]
    fn list_is_scoped_per_game() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        store.record_deployed_file(489830, &entry("Data/x.esp")).unwrap();
        store.record_deployed_file(377160, &entry("Data/y.esp")).unwrap();
        assert_eq!(store.list_deployed_files(489830).unwrap().len(), 1);
        assert_eq!(store.list_deployed_files(377160).unwrap().len(), 1);
    }

    #[test]
    fn corrupt_method_token_surfaces_error() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        store
            .conn
            .execute(
                "INSERT INTO deployed_file (appid, target_rel, source_mod, method, hash, pre_existing)
                 VALUES (1, 'Data/z.esp', 1, 'bogus', 'h', 0)",
                [],
            )
            .unwrap();
        let err = store.list_deployed_files(1).unwrap_err();
        assert!(matches!(err, StoreError::Corrupt(_)));
    }
}
