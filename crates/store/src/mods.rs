//! Multi-mod registry (D-01/D-13): the `managed_mod` table facade.
//!
//! Phase 2 makes mods first-class: many [`core::ManagedMod`] rows coexist per game,
//! each carrying a `rank` (lower = higher priority) that orders file-conflict winners
//! (CONF-02 substrate). No `rusqlite` type appears in this module's public surface.

use std::path::PathBuf;

use core::{ManagedMod, StoreError};
use rusqlite::params;

use crate::db::Store;

impl Store {
    /// Insert a managed mod for a game; returns its assigned row id.
    ///
    /// The `id` field of the passed `ManagedMod` is ignored (the store assigns it).
    pub fn add_mod(&self, appid: u32, m: &ManagedMod) -> Result<i64, StoreError> {
        self.conn
            .execute(
                "INSERT INTO managed_mod (appid, name, staging_root, enabled, rank)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    appid,
                    m.name,
                    m.staging_root.to_string_lossy(),
                    m.enabled as i64,
                    m.rank,
                ],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// List all mods for a game, ordered by rank (ascending = highest priority first).
    pub fn list_mods(&self, appid: u32) -> Result<Vec<ManagedMod>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, name, staging_root, enabled, rank
                 FROM managed_mod WHERE appid = ?1 ORDER BY rank, id",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let rows = stmt
            .query_map(params![appid], row_to_mod)
            .map_err(|e| StoreError::Db(e.to_string()))?;
        collect_mods(rows)
    }

    /// Fetch a single mod by row id, if present.
    pub fn get_mod(&self, id: i64) -> Result<Option<ManagedMod>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, name, staging_root, enabled, rank
                 FROM managed_mod WHERE id = ?1",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let mut rows = stmt
            .query_map(params![id], row_to_mod)
            .map_err(|e| StoreError::Db(e.to_string()))?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| StoreError::Db(e.to_string()))?)),
            None => Ok(None),
        }
    }

    /// Set the deployment rank of a mod. Idempotent: a missing id is a no-op (`false`).
    pub fn set_mod_rank(&self, id: i64, rank: u32) -> Result<bool, StoreError> {
        let n = self
            .conn
            .execute(
                "UPDATE managed_mod SET rank = ?2 WHERE id = ?1",
                params![id, rank],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(n > 0)
    }

    /// Remove a mod by row id. Idempotent: removing a missing row returns `false`.
    pub fn remove_mod(&self, id: i64) -> Result<bool, StoreError> {
        let n = self
            .conn
            .execute("DELETE FROM managed_mod WHERE id = ?1", params![id])
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(n > 0)
    }
}

fn row_to_mod(row: &rusqlite::Row<'_>) -> rusqlite::Result<ManagedMod> {
    Ok(ManagedMod {
        id: row.get(0)?,
        name: row.get(1)?,
        staging_root: PathBuf::from(row.get::<_, String>(2)?),
        enabled: row.get::<_, i64>(3)? != 0,
        rank: row.get(4)?,
    })
}

fn collect_mods(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<ManagedMod>>,
) -> Result<Vec<ManagedMod>, StoreError> {
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| StoreError::Db(e.to_string()))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn a_mod(name: &str, rank: u32) -> ManagedMod {
        ManagedMod {
            id: 0,
            name: name.into(),
            staging_root: PathBuf::from(format!("/staging/{name}")),
            enabled: true,
            rank,
        }
    }

    #[test]
    fn add_get_remove_round_trips() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();

        let id = store.add_mod(489830, &a_mod("SkyUI", 1)).unwrap();
        let got = store.get_mod(id).unwrap().unwrap();
        assert_eq!(got.name, "SkyUI");
        assert_eq!(got.rank, 1);
        assert!(got.enabled);
        assert_eq!(got.id, id);

        assert!(store.remove_mod(id).unwrap());
        // Removing again is a no-op.
        assert!(!store.remove_mod(id).unwrap());
        assert_eq!(store.get_mod(id).unwrap(), None);
    }

    /// CONF-02 substrate: list_mods returns mods in ascending rank order.
    #[test]
    fn rank_orders_list() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        store.add_mod(489830, &a_mod("low_prio", 5)).unwrap();
        store.add_mod(489830, &a_mod("high_prio", 1)).unwrap();
        store.add_mod(489830, &a_mod("mid_prio", 3)).unwrap();

        let mods = store.list_mods(489830).unwrap();
        let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["high_prio", "mid_prio", "low_prio"]);
    }

    #[test]
    fn set_rank_updates_ordering() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let a = store.add_mod(1, &a_mod("a", 1)).unwrap();
        let b = store.add_mod(1, &a_mod("b", 2)).unwrap();

        assert!(store.set_mod_rank(a, 9).unwrap());
        let mods = store.list_mods(1).unwrap();
        assert_eq!(mods[0].id, b); // b (rank 2) now ahead of a (rank 9)
        // Setting rank on a missing id is a no-op.
        assert!(!store.set_mod_rank(99999, 1).unwrap());
    }

    #[test]
    fn list_is_scoped_per_game() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        store.add_mod(489830, &a_mod("x", 1)).unwrap();
        store.add_mod(377160, &a_mod("y", 1)).unwrap();
        assert_eq!(store.list_mods(489830).unwrap().len(), 1);
        assert_eq!(store.list_mods(377160).unwrap().len(), 1);
    }
}
