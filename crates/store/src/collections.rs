//! Collection persistence store facade (COLL-01/02): the V5 `collection`,
//! `collection_mod`, and `fomod_choice` tables.
//!
//! Records a pinned NexusMods Collection revision, every mod it pins (with the mod's
//! Nexus source identity + install phase + conflict rank + a link to the local
//! `managed_mod` it stages into), and the per-mod replayed FOMOD `choices` JSON. Mirrors
//! `nexus.rs`: `core` types in/out, all SQL inside `store`, NO `rusqlite` type in any
//! public signature. `(appid, slug, revision)` is UNIQUE so a re-resolve UPSERTs the same
//! collection row; the FKs CASCADE so deleting a collection sheds its `collection_mod`
//! rows and each of those sheds its `fomod_choice` row.

use core::{Collection, CollectionMod, StoreError};

use rusqlite::params;

use crate::db::Store;

impl Store {
    /// Upsert a Collection revision; returns its `collection` row id.
    ///
    /// Idempotent on `(appid, slug, revision)` (UNIQUE): re-resolving the same revision
    /// UPDATEs the existing row (name/profile_id refreshed) and returns the SAME id rather
    /// than erroring or duplicating. The single-statement `RETURNING id` form (WR-05) yields
    /// the affected row's id for BOTH the INSERT and the DO UPDATE branch atomically.
    pub fn add_collection(&self, c: &Collection) -> Result<i64, StoreError> {
        let id: i64 = self
            .conn
            .query_row(
                "INSERT INTO collection (appid, slug, revision, name, profile_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (appid, slug, revision) DO UPDATE SET
                     name       = excluded.name,
                     profile_id = excluded.profile_id
                 RETURNING id",
                params![c.appid, c.slug, c.revision, c.name, c.profile_id],
                |r| r.get(0),
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(id)
    }

    /// Fetch a Collection by its row id, if present.
    pub fn get_collection(&self, id: i64) -> Result<Option<Collection>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, appid, slug, revision, name, profile_id
                 FROM collection WHERE id = ?1",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let mut rows = stmt
            .query_map(params![id], row_to_collection)
            .map_err(|e| StoreError::Db(e.to_string()))?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| StoreError::Db(e.to_string()))?)),
            None => Ok(None),
        }
    }

    /// Remove a Collection by row id. Idempotent: a missing row returns `false`. The FK
    /// CASCADE sheds its `collection_mod` rows (and their `fomod_choice` rows) automatically.
    pub fn remove_collection(&self, id: i64) -> Result<bool, StoreError> {
        let n = self
            .conn
            .execute("DELETE FROM collection WHERE id = ?1", params![id])
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(n > 0)
    }

    /// Upsert one pinned mod into a Collection; returns its `collection_mod` row id.
    ///
    /// Idempotent on `(collection_id, mod_id)` (UNIQUE): re-recording the same membership
    /// UPDATEs its source identity / phase / rank and returns the SAME id. When the
    /// [`CollectionMod::choices_json`] is `Some`, the replayed FOMOD choices are upserted into
    /// `fomod_choice`; when `None`, any existing choices row for this membership is removed
    /// (so clearing a mod's choices is reflected).
    pub fn add_collection_mod(
        &self,
        collection_id: i64,
        cm: &CollectionMod,
    ) -> Result<i64, StoreError> {
        let row_id: i64 = self
            .conn
            .query_row(
                "INSERT INTO collection_mod
                     (collection_id, mod_id, nexus_mod_id, file_id, md5, phase, rank)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (collection_id, mod_id) DO UPDATE SET
                     nexus_mod_id = excluded.nexus_mod_id,
                     file_id      = excluded.file_id,
                     md5          = excluded.md5,
                     phase        = excluded.phase,
                     rank         = excluded.rank
                 RETURNING id",
                params![
                    collection_id,
                    cm.mod_id,
                    cm.nexus_mod_id as i64,
                    cm.file_id as i64,
                    cm.md5,
                    cm.phase,
                    cm.rank,
                ],
                |r| r.get(0),
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;

        match &cm.choices_json {
            Some(choices) => {
                self.conn
                    .execute(
                        "INSERT INTO fomod_choice (collection_mod_id, choices_json)
                         VALUES (?1, ?2)
                         ON CONFLICT (collection_mod_id) DO UPDATE SET
                             choices_json = excluded.choices_json",
                        params![row_id, choices],
                    )
                    .map_err(|e| StoreError::Db(e.to_string()))?;
            }
            None => {
                self.conn
                    .execute(
                        "DELETE FROM fomod_choice WHERE collection_mod_id = ?1",
                        params![row_id],
                    )
                    .map_err(|e| StoreError::Db(e.to_string()))?;
            }
        }

        Ok(row_id)
    }

    /// List the pinned mods of a Collection, ordered by install phase then rank.
    ///
    /// Each row carries its replayed FOMOD `choices_json` (via a LEFT JOIN on
    /// `fomod_choice`), `None` when the mod pins no scripted-installer choices.
    pub fn list_collection_mods(
        &self,
        collection_id: i64,
    ) -> Result<Vec<CollectionMod>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT cm.mod_id, cm.nexus_mod_id, cm.file_id, cm.md5, cm.phase, cm.rank,
                        fc.choices_json
                 FROM collection_mod cm
                 LEFT JOIN fomod_choice fc ON fc.collection_mod_id = cm.id
                 WHERE cm.collection_id = ?1
                 ORDER BY cm.phase, cm.rank, cm.id",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let rows = stmt
            .query_map(params![collection_id], row_to_collection_mod)
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| StoreError::Db(e.to_string()))?);
        }
        Ok(out)
    }
}

fn row_to_collection(row: &rusqlite::Row<'_>) -> rusqlite::Result<Collection> {
    Ok(Collection {
        id: row.get(0)?,
        appid: row.get::<_, i64>(1)? as u32,
        slug: row.get(2)?,
        revision: row.get::<_, i64>(3)? as u32,
        name: row.get(4)?,
        profile_id: row.get(5)?,
    })
}

fn row_to_collection_mod(row: &rusqlite::Row<'_>) -> rusqlite::Result<CollectionMod> {
    Ok(CollectionMod {
        mod_id: row.get(0)?,
        nexus_mod_id: row.get::<_, i64>(1)? as u64,
        file_id: row.get::<_, i64>(2)? as u64,
        md5: row.get(3)?,
        phase: row.get::<_, i64>(4)? as u32,
        rank: row.get::<_, i64>(5)? as u32,
        choices_json: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ManagedMod;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn store() -> (TempDir, Store) {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        (dir, store)
    }

    fn a_mod(name: &str) -> ManagedMod {
        ManagedMod {
            id: 0,
            name: name.into(),
            staging_root: PathBuf::from(format!("/staging/{name}")),
            enabled: true,
            rank: 1,
        }
    }

    fn a_collection() -> Collection {
        Collection {
            id: 0,
            appid: 489830,
            slug: "skyrim-essentials".into(),
            revision: 7,
            name: "Skyrim Essentials".into(),
            profile_id: None,
        }
    }

    fn a_collection_mod(mod_id: i64) -> CollectionMod {
        CollectionMod {
            mod_id,
            nexus_mod_id: 12604,
            file_id: 120063,
            md5: Some("d41d8cd98f00b204e9800998ecf8427e".into()),
            phase: 0,
            rank: 2,
            choices_json: None,
        }
    }

    #[test]
    fn add_get_collection_round_trips() {
        let (_d, store) = store();
        let id = store.add_collection(&a_collection()).unwrap();
        assert!(id > 0);

        let got = store.get_collection(id).unwrap().unwrap();
        assert_eq!(got.id, id);
        assert_eq!(got.appid, 489830);
        assert_eq!(got.slug, "skyrim-essentials");
        assert_eq!(got.revision, 7);
        assert_eq!(got.name, "Skyrim Essentials");
        assert_eq!(got.profile_id, None);

        assert_eq!(store.get_collection(99999).unwrap(), None);
    }

    #[test]
    fn collection_with_profile_link_round_trips() {
        let (_d, store) = store();
        let appid = 489830;
        // A real profile to link, so the FK validates.
        let profile_id = store.create_profile(appid, "Collection: Essentials").unwrap();
        let mut c = a_collection();
        c.profile_id = Some(profile_id);
        let id = store.add_collection(&c).unwrap();
        assert_eq!(store.get_collection(id).unwrap().unwrap().profile_id, Some(profile_id));
    }

    /// Upsert idempotency on the UNIQUE (appid, slug, revision) key: same id, updated fields.
    #[test]
    fn add_collection_is_idempotent_on_unique_key() {
        let (_d, store) = store();
        let id1 = store.add_collection(&a_collection()).unwrap();

        let mut renamed = a_collection();
        renamed.name = "Skyrim Essentials (revised)".into();
        let id2 = store.add_collection(&renamed).unwrap();
        assert_eq!(id1, id2, "re-resolving the same revision must reuse the row");
        assert_eq!(
            store.get_collection(id1).unwrap().unwrap().name,
            "Skyrim Essentials (revised)"
        );
        // A different revision is a NEW row.
        let mut newer = a_collection();
        newer.revision = 8;
        let id3 = store.add_collection(&newer).unwrap();
        assert_ne!(id1, id3, "a different revision is a distinct collection");
    }

    #[test]
    fn collection_mod_round_trips_with_and_without_choices() {
        let (_d, store) = store();
        let appid = 489830;
        let cid = store.add_collection(&a_collection()).unwrap();
        let mod_a = store.add_mod(appid, &a_mod("SkyUI")).unwrap();
        let mod_b = store.add_mod(appid, &a_mod("SKSE64")).unwrap();

        // Mod A pins FOMOD choices; mod B pins none.
        let mut cm_a = a_collection_mod(mod_a);
        cm_a.choices_json = Some(
            r#"{"type":"fomod","options":[{"name":"Main","groups":[{"name":"UI","choices":[{"name":"Full","idx":0}]}]}]}"#.into(),
        );
        cm_a.phase = 1;
        cm_a.rank = 5;
        let cm_b = a_collection_mod(mod_b);

        store.add_collection_mod(cid, &cm_a).unwrap();
        store.add_collection_mod(cid, &cm_b).unwrap();

        let mods = store.list_collection_mods(cid).unwrap();
        // Ordered by phase then rank: B (phase 0) before A (phase 1).
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].mod_id, mod_b);
        assert_eq!(mods[0].choices_json, None);
        assert_eq!(mods[1].mod_id, mod_a);
        assert_eq!(mods[1].phase, 1);
        assert_eq!(mods[1].rank, 5);
        assert!(mods[1].choices_json.as_ref().unwrap().contains("fomod"));
    }

    /// Re-recording a membership is idempotent on (collection_id, mod_id): same id,
    /// updated fields, and clearing choices_json removes the fomod_choice row.
    #[test]
    fn collection_mod_upsert_is_idempotent_and_clears_choices() {
        let (_d, store) = store();
        let appid = 489830;
        let cid = store.add_collection(&a_collection()).unwrap();
        let mod_a = store.add_mod(appid, &a_mod("SkyUI")).unwrap();

        let mut cm = a_collection_mod(mod_a);
        cm.choices_json = Some(r#"{"type":"fomod","options":[]}"#.into());
        let id1 = store.add_collection_mod(cid, &cm).unwrap();
        assert!(store.list_collection_mods(cid).unwrap()[0].choices_json.is_some());

        // Re-record with the choices cleared and a new rank.
        cm.choices_json = None;
        cm.rank = 9;
        let id2 = store.add_collection_mod(cid, &cm).unwrap();
        assert_eq!(id1, id2, "same membership must reuse the row");
        let mods = store.list_collection_mods(cid).unwrap();
        assert_eq!(mods[0].rank, 9);
        assert_eq!(mods[0].choices_json, None, "clearing choices drops the fomod_choice row");
    }

    /// CASCADE: deleting a collection removes its collection_mod and fomod_choice rows.
    #[test]
    fn deleting_collection_cascades_mods_and_choices() {
        let (_d, store) = store();
        let appid = 489830;
        let cid = store.add_collection(&a_collection()).unwrap();
        let mod_a = store.add_mod(appid, &a_mod("SkyUI")).unwrap();
        let mut cm = a_collection_mod(mod_a);
        cm.choices_json = Some(r#"{"type":"fomod","options":[]}"#.into());
        store.add_collection_mod(cid, &cm).unwrap();
        assert_eq!(store.list_collection_mods(cid).unwrap().len(), 1);

        assert!(store.remove_collection(cid).unwrap());
        assert_eq!(store.get_collection(cid).unwrap(), None);
        // Membership + its fomod_choice are gone (CASCADE).
        assert_eq!(store.list_collection_mods(cid).unwrap().len(), 0);
        let choice_rows: i64 = store
            .conn
            .query_row("SELECT count(*) FROM fomod_choice", [], |r| r.get(0))
            .unwrap();
        assert_eq!(choice_rows, 0, "fomod_choice must CASCADE-delete with its collection_mod");
        // Removing again is a no-op.
        assert!(!store.remove_collection(cid).unwrap());
    }

    /// CASCADE: deleting the managed_mod removes its collection_mod link (and choices).
    #[test]
    fn deleting_managed_mod_cascades_collection_link() {
        let (_d, store) = store();
        let appid = 489830;
        let cid = store.add_collection(&a_collection()).unwrap();
        let mod_a = store.add_mod(appid, &a_mod("SkyUI")).unwrap();
        let mut cm = a_collection_mod(mod_a);
        cm.choices_json = Some(r#"{"type":"fomod","options":[]}"#.into());
        store.add_collection_mod(cid, &cm).unwrap();
        assert_eq!(store.list_collection_mods(cid).unwrap().len(), 1);

        assert!(store.remove_mod(mod_a).unwrap());
        assert_eq!(
            store.list_collection_mods(cid).unwrap().len(),
            0,
            "FK CASCADE must remove the collection_mod link when the managed_mod is deleted"
        );
        let choice_rows: i64 = store
            .conn
            .query_row("SELECT count(*) FROM fomod_choice", [], |r| r.get(0))
            .unwrap();
        assert_eq!(choice_rows, 0, "the orphaned fomod_choice must CASCADE too");
    }

    /// Dropping the linked profile NULLs collection.profile_id (ON DELETE SET NULL) —
    /// the collection record survives a profile teardown (Plan 04 uninstall keeps the
    /// resolve history).
    #[test]
    fn dropping_profile_nulls_collection_link_not_the_collection() {
        let (_d, store) = store();
        let appid = 489830;
        let profile_id = store.create_profile(appid, "Collection: Essentials").unwrap();
        let mut c = a_collection();
        c.profile_id = Some(profile_id);
        let cid = store.add_collection(&c).unwrap();

        assert!(store.delete_profile(profile_id).unwrap());
        let got = store.get_collection(cid).unwrap().unwrap();
        assert_eq!(got.profile_id, None, "the profile FK is SET NULL, collection survives");
    }

    /// Migration-reach (mirrors db.rs's v4 guard): reach a genuine V4-only state, then
    /// `Store::open` applies ONLY V5 over it, ADDING the three collection tables without
    /// altering any prior table's columns.
    #[test]
    fn v5_adds_collection_tables_additively_over_v4() {
        use crate::db::_migrations_reexport as migrations;
        use refinery::Target;
        use rusqlite::Connection;

        let dir = TempDir::new().unwrap();
        let db = dir.path().join("v4.db");

        // managed_mod's column set at V4 (before V5 runs), and prove V5 tables are absent.
        let cols_v4: Vec<String> = {
            let mut conn = Connection::open(&db).unwrap();
            let _: String = conn
                .query_row("PRAGMA journal_mode=WAL;", [], |r| r.get(0))
                .unwrap();
            migrations::runner()
                .set_target(Target::Version(4))
                .run(&mut conn)
                .unwrap();

            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN
                       ('collection','collection_mod','fomod_choice')",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 0, "V5 tables must be absent at V4");

            let mut stmt = conn.prepare("PRAGMA table_info(managed_mod)").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .map(|c| c.unwrap())
                .collect()
        };

        // Open via Store::open → refinery applies ONLY V5 over the V4 state.
        let store = Store::open(&db).unwrap();

        for table in ["collection", "collection_mod", "fomod_choice"] {
            let n: i64 = store
                .conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "V5 must create {table}");
        }

        // managed_mod's columns are UNCHANGED (V5 is additive, never ALTERs a prior table).
        let cols_v5: Vec<String> = {
            let mut stmt = store.conn.prepare("PRAGMA table_info(managed_mod)").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .map(|c| c.unwrap())
                .collect()
        };
        assert_eq!(cols_v4, cols_v5, "V5 must not alter managed_mod's columns");
    }
}
