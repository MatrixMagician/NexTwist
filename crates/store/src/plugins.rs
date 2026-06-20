//! Per-profile plugin state (D-07/D-13): the `plugin_state` table facade.
//!
//! One row per plugin known to a profile: its [`core::PluginKind`], whether it is
//! enabled, and its load-order position. The `kind` column stores the PluginKind
//! token (esm/esl/esp); a corrupt token surfaces [`StoreError::Corrupt`] via the
//! double-Result row mapper (the same pattern as `manifest.rs`), never a silent wrong
//! value (T-02-02). No `rusqlite` type leaks publicly.

use core::{Plugin, PluginKind, StoreError};
use rusqlite::params;

use crate::db::Store;

impl Store {
    /// Upsert one plugin's state for a profile. Keyed by `UNIQUE(profile_id, plugin_name)`.
    pub fn set_plugin_state(&self, profile_id: i64, plugin: &Plugin) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO plugin_state (profile_id, plugin_name, kind, enabled, order_index)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (profile_id, plugin_name)
                 DO UPDATE SET kind = excluded.kind,
                               enabled = excluded.enabled,
                               order_index = excluded.order_index",
                params![
                    profile_id,
                    plugin.name,
                    plugin.kind.as_str(),
                    plugin.enabled as i64,
                    plugin.order,
                ],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(())
    }

    /// List a profile's plugin state, ordered by load-order position.
    pub fn list_plugin_state(&self, profile_id: i64) -> Result<Vec<Plugin>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT plugin_name, kind, enabled, order_index
                 FROM plugin_state WHERE profile_id = ?1 ORDER BY order_index, plugin_name",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let rows = stmt
            .query_map(params![profile_id], row_to_plugin)
            .map_err(|e| StoreError::Db(e.to_string()))?;

        let mut out = Vec::new();
        for r in rows {
            // Outer Result = rusqlite row error; inner Result = domain decode error.
            let plugin = r.map_err(|e| StoreError::Db(e.to_string()))??;
            out.push(plugin);
        }
        Ok(out)
    }
}

fn row_to_plugin(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Plugin, StoreError>> {
    let kind_tok: String = row.get(1)?;
    let kind = match PluginKind::from_token(&kind_tok) {
        Some(k) => k,
        None => {
            return Ok(Err(StoreError::Corrupt(format!(
                "unknown plugin kind '{kind_tok}'"
            ))));
        }
    };
    Ok(Ok(Plugin {
        name: row.get(0)?,
        kind,
        enabled: row.get::<_, i64>(2)? != 0,
        order: row.get(3)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn plugin(name: &str, kind: PluginKind, order: u32) -> Plugin {
        Plugin {
            name: name.into(),
            kind,
            enabled: true,
            order,
        }
    }

    #[test]
    fn set_list_round_trips_in_order() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        store
            .set_plugin_state(1, &plugin("Patch.esp", PluginKind::Esp, 2))
            .unwrap();
        store
            .set_plugin_state(1, &plugin("Skyrim.esm", PluginKind::Esm, 0))
            .unwrap();
        store
            .set_plugin_state(1, &plugin("Light.esl", PluginKind::Esl, 1))
            .unwrap();

        let plugins = store.list_plugin_state(1).unwrap();
        let names: Vec<&str> = plugins.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Skyrim.esm", "Light.esl", "Patch.esp"]);
        assert_eq!(plugins[0].kind, PluginKind::Esm);
    }

    #[test]
    fn upsert_replaces_state() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        store
            .set_plugin_state(1, &plugin("A.esp", PluginKind::Esp, 0))
            .unwrap();
        let mut updated = plugin("A.esp", PluginKind::Esm, 5);
        updated.enabled = false;
        store.set_plugin_state(1, &updated).unwrap();

        let plugins = store.list_plugin_state(1).unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].kind, PluginKind::Esm);
        assert_eq!(plugins[0].order, 5);
        assert!(!plugins[0].enabled);
    }

    #[test]
    fn scoped_per_profile() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        store
            .set_plugin_state(1, &plugin("A.esp", PluginKind::Esp, 0))
            .unwrap();
        store
            .set_plugin_state(2, &plugin("B.esp", PluginKind::Esp, 0))
            .unwrap();
        assert_eq!(store.list_plugin_state(1).unwrap().len(), 1);
        assert_eq!(store.list_plugin_state(2).unwrap().len(), 1);
    }

    #[test]
    fn corrupt_kind_token_surfaces_error() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        store
            .conn
            .execute(
                "INSERT INTO plugin_state (profile_id, plugin_name, kind, enabled, order_index)
                 VALUES (1, 'X.esp', 'bogus', 0, 0)",
                [],
            )
            .unwrap();
        let err = store.list_plugin_state(1).unwrap_err();
        assert!(matches!(err, StoreError::Corrupt(_)));
    }
}
