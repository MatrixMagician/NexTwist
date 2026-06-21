//! Profile + membership facade (D-13/D-14/D-16): the `profile` and `profile_mod`
//! table facades.
//!
//! A profile is a lightweight reference set over the shared staging store — it owns
//! no files, only which mods are enabled and at what rank, independently per profile.
//! Exactly one profile is active per game. No `rusqlite` type leaks publicly.

use core::{Profile, StoreError};
use rusqlite::{params, OptionalExtension};

use crate::db::Store;

impl Store {
    /// Create a profile for a game; returns its assigned row id.
    /// The new profile is created inactive — use [`Store::set_active_profile`].
    /// `UNIQUE(appid, name)` surfaces a [`StoreError::Db`] on a duplicate name.
    pub fn create_profile(&self, appid: u32, name: &str) -> Result<i64, StoreError> {
        self.conn
            .execute(
                "INSERT INTO profile (appid, name, active) VALUES (?1, ?2, 0)",
                params![appid, name],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// List all profiles for a game, ordered by id (creation order) for determinism.
    pub fn list_profiles(&self, appid: u32) -> Result<Vec<Profile>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, appid, name, active FROM profile WHERE appid = ?1 ORDER BY id",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let rows = stmt
            .query_map(params![appid], row_to_profile)
            .map_err(|e| StoreError::Db(e.to_string()))?;
        collect_profiles(rows)
    }

    /// The active profile for a game, if one is set.
    pub fn active_profile(&self, appid: u32) -> Result<Option<Profile>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, appid, name, active
                 FROM profile WHERE appid = ?1 AND active = 1 LIMIT 1",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let mut rows = stmt
            .query_map(params![appid], row_to_profile)
            .map_err(|e| StoreError::Db(e.to_string()))?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| StoreError::Db(e.to_string()))?)),
            None => Ok(None),
        }
    }

    /// Make `profile_id` the single active profile for `appid`.
    ///
    /// Runs in a transaction so exactly one profile is active per game: it clears the
    /// active flag on every profile for the game, then sets it on the target. Returns
    /// `false` if the target id does not belong to that game (nothing activated).
    pub fn set_active_profile(&self, appid: u32, profile_id: i64) -> Result<bool, StoreError> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| StoreError::Db(e.to_string()))?;
        tx.execute(
            "UPDATE profile SET active = 0 WHERE appid = ?1",
            params![appid],
        )
        .map_err(|e| StoreError::Db(e.to_string()))?;
        let n = tx
            .execute(
                "UPDATE profile SET active = 1 WHERE id = ?1 AND appid = ?2",
                params![profile_id, appid],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        tx.commit().map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(n > 0)
    }

    /// Clear the active flag on every profile for a game, leaving NO active profile.
    ///
    /// Used to recover from a mid-switch failure (WR-02): if `switch_profile` purges the
    /// old deployment to pristine but a later step (deploy / plugins write) fails, the
    /// store must not keep the OLD profile flagged active — its deployment no longer exists
    /// on disk. Clearing the flag makes the persisted "no active profile" state honest
    /// (the game is pristine), so the UI can prompt a fresh switch instead of showing a
    /// profile whose deployment is gone. Idempotent.
    pub fn clear_active_profile(&self, appid: u32) -> Result<(), StoreError> {
        self.conn
            .execute(
                "UPDATE profile SET active = 0 WHERE appid = ?1",
                params![appid],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(())
    }

    /// Upsert a (profile, mod) membership row with its enabled flag and per-profile rank.
    /// Keyed by `UNIQUE(profile_id, mod_id)`.
    pub fn set_profile_mod(
        &self,
        profile_id: i64,
        mod_id: i64,
        enabled: bool,
        rank: u32,
    ) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO profile_mod (profile_id, mod_id, enabled, rank)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (profile_id, mod_id)
                 DO UPDATE SET enabled = excluded.enabled, rank = excluded.rank",
                params![profile_id, mod_id, enabled as i64, rank],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(())
    }

    /// List a profile's membership as `(mod_id, enabled, rank)`, ordered by rank.
    pub fn list_profile_mods(
        &self,
        profile_id: i64,
    ) -> Result<Vec<(i64, bool, u32)>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT mod_id, enabled, rank
                 FROM profile_mod WHERE profile_id = ?1 ORDER BY rank, mod_id",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let rows = stmt
            .query_map(params![profile_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)? != 0,
                    row.get::<_, u32>(2)?,
                ))
            })
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| StoreError::Db(e.to_string()))?);
        }
        Ok(out)
    }

    /// Delete a profile and its membership rows. Idempotent: a missing id returns `false`.
    ///
    /// REFUSES to delete the currently-active profile (CR-02 safety invariant): the active
    /// profile may have a live on-disk deployment, and deleting it here would orphan those
    /// files (the profile flow no longer triggers a purge) AND leave the game with zero
    /// active profiles, violating "exactly one active per game". The caller must switch to
    /// another profile first — `switch_profile` purges the outgoing deployment to pristine
    /// — and only then delete the now-inactive profile.
    pub fn delete_profile(&self, profile_id: i64) -> Result<bool, StoreError> {
        // Reject deleting an active profile; the caller must switch away (which purges
        // the deployment to pristine) before this profile can be safely removed.
        let is_active: bool = self
            .conn
            .query_row(
                "SELECT active FROM profile WHERE id = ?1",
                params![profile_id],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(|e| StoreError::Db(e.to_string()))?
            .map(|a| a != 0)
            .unwrap_or(false);
        if is_active {
            return Err(StoreError::Db(
                "cannot delete the active profile; switch to another profile first".into(),
            ));
        }
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| StoreError::Db(e.to_string()))?;
        tx.execute(
            "DELETE FROM profile_mod WHERE profile_id = ?1",
            params![profile_id],
        )
        .map_err(|e| StoreError::Db(e.to_string()))?;
        tx.execute(
            "DELETE FROM plugin_state WHERE profile_id = ?1",
            params![profile_id],
        )
        .map_err(|e| StoreError::Db(e.to_string()))?;
        let n = tx
            .execute("DELETE FROM profile WHERE id = ?1", params![profile_id])
            .map_err(|e| StoreError::Db(e.to_string()))?;
        tx.commit().map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(n > 0)
    }
}

fn row_to_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<Profile> {
    Ok(Profile {
        id: row.get(0)?,
        appid: row.get(1)?,
        name: row.get(2)?,
        active: row.get::<_, i64>(3)? != 0,
    })
}

fn collect_profiles(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<Profile>>,
) -> Result<Vec<Profile>, StoreError> {
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| StoreError::Db(e.to_string()))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ManagedMod;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Insert a real managed mod for a game and return its id (WR-06: `profile_mod.mod_id`
    /// now FK-references `managed_mod(id)`, so tests must use real mod rows).
    fn add_test_mod(store: &Store, appid: u32, name: &str) -> i64 {
        store
            .add_mod(
                appid,
                &ManagedMod {
                    id: 0,
                    name: name.into(),
                    staging_root: PathBuf::from(format!("/staging/{name}")),
                    enabled: true,
                    rank: 1,
                },
            )
            .unwrap()
    }

    /// PROF-01: a game can hold multiple profiles, all listed.
    #[test]
    fn create_multiple() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        store.create_profile(489830, "Default").unwrap();
        store.create_profile(489830, "Heavy Modlist").unwrap();

        let profiles = store.list_profiles(489830).unwrap();
        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Default", "Heavy Modlist"]);
    }

    #[test]
    fn duplicate_name_per_game_rejected() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        store.create_profile(489830, "Default").unwrap();
        let err = store.create_profile(489830, "Default").unwrap_err();
        assert!(matches!(err, StoreError::Db(_)));
    }

    #[test]
    fn exactly_one_active_per_game() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let a = store.create_profile(1, "A").unwrap();
        let b = store.create_profile(1, "B").unwrap();

        assert_eq!(store.active_profile(1).unwrap(), None);
        assert!(store.set_active_profile(1, a).unwrap());
        assert_eq!(store.active_profile(1).unwrap().unwrap().id, a);
        // Switching active flips exactly one row.
        assert!(store.set_active_profile(1, b).unwrap());
        assert_eq!(store.active_profile(1).unwrap().unwrap().id, b);
        let actives = store
            .list_profiles(1)
            .unwrap()
            .into_iter()
            .filter(|p| p.active)
            .count();
        assert_eq!(actives, 1);
        // Activating a profile from another game is a no-op.
        assert!(!store.set_active_profile(1, 99999).unwrap());
    }

    /// PROF-03: two profiles hold independent enabled-mod sets + ranks.
    #[test]
    fn preserve_membership() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let p1 = store.create_profile(1, "P1").unwrap();
        let p2 = store.create_profile(1, "P2").unwrap();
        // Real mod rows (WR-06 FK): m10 and m20 stand in for the former hardcoded 10/20.
        let m10 = add_test_mod(&store, 1, "m10");
        let m20 = add_test_mod(&store, 1, "m20");

        // P1: m10 enabled rank 1, m20 disabled rank 2.
        store.set_profile_mod(p1, m10, true, 1).unwrap();
        store.set_profile_mod(p1, m20, false, 2).unwrap();
        // P2: only m20, enabled, rank 1.
        store.set_profile_mod(p2, m20, true, 1).unwrap();

        let m1 = store.list_profile_mods(p1).unwrap();
        assert_eq!(m1, vec![(m10, true, 1), (m20, false, 2)]);
        let m2 = store.list_profile_mods(p2).unwrap();
        assert_eq!(m2, vec![(m20, true, 1)]);

        // Upsert changes one profile without touching the other.
        store.set_profile_mod(p1, m10, false, 9).unwrap();
        assert_eq!(store.list_profile_mods(p1).unwrap(), vec![(m20, false, 2), (m10, false, 9)]);
        assert_eq!(store.list_profile_mods(p2).unwrap(), vec![(m20, true, 1)]);
    }

    #[test]
    fn delete_removes_profile_and_membership() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let p = store.create_profile(1, "Doomed").unwrap();
        let m = add_test_mod(&store, 1, "m1");
        store.set_profile_mod(p, m, true, 1).unwrap();

        assert!(store.delete_profile(p).unwrap());
        assert!(!store.delete_profile(p).unwrap());
        assert!(store.list_profiles(1).unwrap().is_empty());
        assert!(store.list_profile_mods(p).unwrap().is_empty());
    }

    /// WR-06: a membership row referencing a non-existent profile or mod is now REJECTED
    /// by the foreign keys (it was silently inserted as a dangling row under V2).
    #[test]
    fn dangling_membership_rejected_by_fk() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let p = store.create_profile(1, "P").unwrap();
        let m = add_test_mod(&store, 1, "real");

        // Dangling mod_id is rejected.
        assert!(matches!(store.set_profile_mod(p, 9999, true, 1), Err(StoreError::Db(_))));
        // Dangling profile_id is rejected.
        assert!(matches!(store.set_profile_mod(9999, m, true, 1), Err(StoreError::Db(_))));
        // A fully-valid membership row still works.
        store.set_profile_mod(p, m, true, 1).unwrap();
        assert_eq!(store.list_profile_mods(p).unwrap(), vec![(m, true, 1)]);
    }

    /// WR-06: `ON DELETE CASCADE` sheds a mod's membership rows when the mod is removed.
    #[test]
    fn remove_mod_cascades_membership() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let p = store.create_profile(1, "P").unwrap();
        let m = add_test_mod(&store, 1, "doomed");
        store.set_profile_mod(p, m, true, 1).unwrap();
        assert_eq!(store.list_profile_mods(p).unwrap().len(), 1);

        assert!(store.remove_mod(m).unwrap());
        assert!(store.list_profile_mods(p).unwrap().is_empty(), "membership cascaded away");
    }

    /// CR-02: deleting the ACTIVE profile is refused (it may have a live deployment and
    /// would leave the game with zero active profiles). The caller must switch away first.
    #[test]
    fn delete_active_profile_is_refused() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let a = store.create_profile(1, "A").unwrap();
        let b = store.create_profile(1, "B").unwrap();
        store.set_active_profile(1, a).unwrap();

        // The active profile cannot be deleted.
        let err = store.delete_profile(a).unwrap_err();
        assert!(matches!(err, StoreError::Db(_)));
        // It is still present and still active — the invariant holds.
        assert_eq!(store.active_profile(1).unwrap().unwrap().id, a);
        assert_eq!(store.list_profiles(1).unwrap().len(), 2);

        // An inactive profile is still deletable.
        assert!(store.delete_profile(b).unwrap());

        // After switching away, the (now inactive) former-active profile can be deleted.
        let c = store.create_profile(1, "C").unwrap();
        store.set_active_profile(1, c).unwrap();
        assert!(store.delete_profile(a).unwrap());
        // Exactly one active profile remains.
        assert_eq!(store.active_profile(1).unwrap().unwrap().id, c);
    }
}
