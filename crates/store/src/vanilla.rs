//! Content-addressed vanilla backup ledger (DEPLOY-04): the `vanilla_backup` facade.
//!
//! Backup-before-overwrite is the single most important safety mechanism. Before a
//! deploy overwrites any pre-existing game file, the deploy engine (Plan 04) copies
//! the original into a per-game original store (`<app_data>/originals/<appid>/<hash>`),
//! content-hashes it with blake3, and records the (appid, target_rel, hash) here.
//! Purge (Plan 05) restores from that store. The hash both keys the on-disk blob
//! and lets multiple targets sharing identical original content dedupe to one blob.

use std::path::Path;

use core::StoreError;
use rusqlite::params;

use crate::db::Store;

impl Store {
    /// Record a vanilla backup row for a (appid, target_rel), keyed by its blake3 hash.
    /// Upserts so re-recording the same target is a no-op-update rather than an error.
    pub fn record_vanilla(
        &self,
        appid: u32,
        target_rel: &Path,
        hash: &str,
    ) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO vanilla_backup (appid, target_rel, hash)
                 VALUES (?1, ?2, ?3)",
                params![appid, target_rel.to_string_lossy(), hash],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(())
    }

    /// True if any vanilla backup row references this blake3 hash. Used to decide
    /// whether the content-addressed blob already exists on disk (dedupe).
    pub fn backup_key_exists(&self, hash: &str) -> Result<bool, StoreError> {
        let n: i64 = self
            .conn
            .query_row(
                "SELECT count(*) FROM vanilla_backup WHERE hash = ?1",
                params![hash],
                |r| r.get(0),
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(n > 0)
    }

    /// The recorded blake3 hash of the vanilla original for a (appid, target_rel),
    /// if one was backed up.
    pub fn vanilla_for(
        &self,
        appid: u32,
        target_rel: &Path,
    ) -> Result<Option<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT hash FROM vanilla_backup WHERE appid = ?1 AND target_rel = ?2",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let mut rows = stmt
            .query_map(params![appid, target_rel.to_string_lossy()], |r| {
                r.get::<_, String>(0)
            })
            .map_err(|e| StoreError::Db(e.to_string()))?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| StoreError::Db(e.to_string()))?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // A realistic blake3 hex digest length (64 chars) — content is illustrative.
    const HASH: &str = "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";

    #[test]
    fn record_then_lookup_by_target_and_key() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let target = PathBuf::from("Data/textures/rock.dds");

        assert!(!store.backup_key_exists(HASH).unwrap());
        assert_eq!(store.vanilla_for(489830, &target).unwrap(), None);

        store.record_vanilla(489830, &target, HASH).unwrap();

        assert!(store.backup_key_exists(HASH).unwrap());
        assert_eq!(
            store.vanilla_for(489830, &target).unwrap(),
            Some(HASH.to_string())
        );
        // Different game / target does not match.
        assert_eq!(store.vanilla_for(377160, &target).unwrap(), None);
    }

    #[test]
    fn record_is_upsert_per_target() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let target = PathBuf::from("Data/a.esp");
        store.record_vanilla(1, &target, "hash_one").unwrap();
        store.record_vanilla(1, &target, "hash_two").unwrap();
        assert_eq!(
            store.vanilla_for(1, &target).unwrap(),
            Some("hash_two".to_string())
        );
        assert!(!store.backup_key_exists("hash_one").unwrap());
        assert!(store.backup_key_exists("hash_two").unwrap());
    }
}
