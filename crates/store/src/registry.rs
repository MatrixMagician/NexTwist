//! Game registry (ENV-03): the managed-game table facade.
//!
//! No `rusqlite` type appears in this module's public surface — callers work in
//! terms of [`core::Game`] only.

use std::path::PathBuf;

use core::{Game, StoreError};
use rusqlite::params;

use crate::db::Store;

impl Store {
    /// Insert (or replace) a managed game keyed by its AppID.
    pub fn add_managed_game(&self, game: &Game) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO managed_game (appid, name, install_dir, prefix, staging_dir)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    game.appid,
                    game.name,
                    path_str(&game.install_dir),
                    path_str(&game.prefix),
                    path_str(&game.staging_dir),
                ],
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        Ok(())
    }

    /// List all managed games, ordered by AppID for determinism.
    pub fn list_managed_games(&self) -> Result<Vec<Game>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT appid, name, install_dir, prefix, staging_dir
                 FROM managed_game ORDER BY appid",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let rows = stmt
            .query_map([], row_to_game)
            .map_err(|e| StoreError::Db(e.to_string()))?;
        collect(rows)
    }

    /// Fetch a single managed game by AppID, if present.
    pub fn get_game(&self, appid: u32) -> Result<Option<Game>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT appid, name, install_dir, prefix, staging_dir
                 FROM managed_game WHERE appid = ?1",
            )
            .map_err(|e| StoreError::Db(e.to_string()))?;
        let mut rows = stmt
            .query_map(params![appid], row_to_game)
            .map_err(|e| StoreError::Db(e.to_string()))?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| StoreError::Db(e.to_string()))?)),
            None => Ok(None),
        }
    }
}

fn row_to_game(row: &rusqlite::Row<'_>) -> rusqlite::Result<Game> {
    Ok(Game {
        appid: row.get(0)?,
        name: row.get(1)?,
        install_dir: PathBuf::from(row.get::<_, String>(2)?),
        prefix: PathBuf::from(row.get::<_, String>(3)?),
        staging_dir: PathBuf::from(row.get::<_, String>(4)?),
    })
}

/// Lossy-free path → string. Paths from the OS are arbitrary bytes; on Linux they
/// are nearly always valid UTF-8. We persist the lossy form and accept that an
/// invalid-UTF-8 path component would not round-trip byte-identically (acceptable
/// for game install paths, which are user-chosen and effectively always UTF-8).
fn path_str(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

fn collect(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<Game>>,
) -> Result<Vec<Game>, StoreError> {
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

    fn skyrim() -> Game {
        Game {
            appid: 489830,
            name: "Skyrim Special Edition".into(),
            install_dir: PathBuf::from("/games/common/Skyrim Special Edition"),
            prefix: PathBuf::from("/games/compatdata/489830/pfx"),
            staging_dir: PathBuf::from("/games/staging/489830"),
        }
    }

    #[test]
    fn add_then_list_round_trips_game() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        let g = skyrim();
        store.add_managed_game(&g).unwrap();

        let games = store.list_managed_games().unwrap();
        assert_eq!(games, vec![g.clone()]);
        assert_eq!(store.get_game(489830).unwrap(), Some(g));
        assert_eq!(store.get_game(1).unwrap(), None);
    }

    #[test]
    fn add_is_upsert_by_appid() {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("d.db")).unwrap();
        store.add_managed_game(&skyrim()).unwrap();
        let mut renamed = skyrim();
        renamed.name = "Skyrim SE (renamed)".into();
        store.add_managed_game(&renamed).unwrap();

        let games = store.list_managed_games().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Skyrim SE (renamed)");
    }
}
