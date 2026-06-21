//! Nexus provenance store facade (NEXUS-03/06): the `nexus_source` table.
//!
//! Records where a managed mod was acquired from NexusMods (mod id, file id, version,
//! display name) so a Nexus-sourced mod is otherwise indistinguishable from a
//! local-archive mod. Mirrors `mods.rs`: `core` types in/out, all SQL inside `store`,
//! NO `rusqlite` type in any public signature. The row is 1:1 with its `managed_mod`
//! (UNIQUE(mod_id)) and the FK CASCADEs, so deleting the mod sheds its provenance.

use core::{NexusSource, StoreError};

use rusqlite::params;

use crate::db::Store;

impl Store {
    /// Upsert the Nexus provenance for a managed mod; returns its `nexus_source` row id.
    ///
    /// Idempotent on `mod_id` (UNIQUE): re-recording provenance for the same mod UPDATEs
    /// the existing row (e.g. a re-download at a newer version) rather than erroring.
    pub fn add_nexus_source(&self, src: &NexusSource) -> Result<i64, StoreError> {
        self.conn
            .execute(
                "INSERT INTO nexus_source (mod_id, nexus_mod_id, file_id, version, display_name)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (mod_id) DO UPDATE SET
                     nexus_mod_id = excluded.nexus_mod_id,
                     file_id      = excluded.file_id,
                     version      = excluded.version,
                     display_name = excluded.display_name",
                params![
                    src.mod_id,
                    src.nexus_mod_id as i64,
                    src.file_id as i64,
                    src.version,
                    src.display_name,
                ],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;

        // last_insert_rowid is the inserted id on INSERT; on an UPDATE branch, look the
        // row up explicitly so the caller always gets the stable nexus_source id.
        let id: i64 = self
            .conn
            .query_row(
                "SELECT id FROM nexus_source WHERE mod_id = ?1",
                params![src.mod_id],
                |r| r.get(0),
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(id)
    }

    /// Fetch the Nexus provenance for a managed mod, if it came from NexusMods.
    pub fn get_nexus_source(&self, mod_id: i64) -> Result<Option<NexusSource>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT mod_id, nexus_mod_id, file_id, version, display_name
                 FROM nexus_source WHERE mod_id = ?1",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let mut rows = stmt
            .query_map(params![mod_id], row_to_nexus_source)
            .map_err(|e| StoreError::Db(e.to_string()))?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| StoreError::Db(e.to_string()))?)),
            None => Ok(None),
        }
    }
}

fn row_to_nexus_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<NexusSource> {
    Ok(NexusSource {
        mod_id: row.get(0)?,
        nexus_mod_id: row.get::<_, i64>(1)? as u64,
        file_id: row.get::<_, i64>(2)? as u64,
        version: row.get(3)?,
        display_name: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ManagedMod;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn a_mod(name: &str) -> ManagedMod {
        ManagedMod {
            id: 0,
            name: name.into(),
            staging_root: PathBuf::from(format!("/staging/{name}")),
            enabled: true,
            rank: 1,
        }
    }

    fn a_source(mod_id: i64) -> NexusSource {
        NexusSource {
            mod_id,
            nexus_mod_id: 12604,
            file_id: 120063,
            version: "1.6.3".into(),
            display_name: "SKSE64".into(),
        }
    }

    #[test]
    fn add_get_round_trips() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let mod_id = store.add_mod(489830, &a_mod("SKSE64")).unwrap();

        let row_id = store.add_nexus_source(&a_source(mod_id)).unwrap();
        assert!(row_id > 0);

        let got = store.get_nexus_source(mod_id).unwrap().unwrap();
        assert_eq!(got.nexus_mod_id, 12604);
        assert_eq!(got.file_id, 120063);
        assert_eq!(got.version, "1.6.3");
        assert_eq!(got.display_name, "SKSE64");

        // A mod with no Nexus provenance returns None.
        assert_eq!(store.get_nexus_source(99999).unwrap(), None);
    }

    #[test]
    fn upsert_is_idempotent_on_mod_id() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let mod_id = store.add_mod(489830, &a_mod("SKSE64")).unwrap();

        let id1 = store.add_nexus_source(&a_source(mod_id)).unwrap();
        // Re-record at a newer version: UPDATEs the same row, no duplicate.
        let mut newer = a_source(mod_id);
        newer.version = "2.0.0".into();
        let id2 = store.add_nexus_source(&newer).unwrap();
        assert_eq!(id1, id2, "upsert must reuse the same row");
        assert_eq!(store.get_nexus_source(mod_id).unwrap().unwrap().version, "2.0.0");
    }

    /// FK CASCADE: deleting the managed_mod sheds its nexus_source row.
    #[test]
    fn cascade_delete_removes_provenance() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let mod_id = store.add_mod(489830, &a_mod("SKSE64")).unwrap();
        store.add_nexus_source(&a_source(mod_id)).unwrap();
        assert!(store.get_nexus_source(mod_id).unwrap().is_some());

        assert!(store.remove_mod(mod_id).unwrap());
        assert_eq!(
            store.get_nexus_source(mod_id).unwrap(),
            None,
            "FK CASCADE must remove the provenance row when the mod is deleted"
        );
    }
}
