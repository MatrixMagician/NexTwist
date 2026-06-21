-- V3: enforce referential integrity on per-profile membership / plugin state (WR-06).
--
-- V2 created `profile_mod` and `plugin_state` WITHOUT foreign keys, so
-- `set_profile_mod` / `set_plugin_state` could INSERT membership rows for non-existent
-- profiles or mods (a dangling profile_id was never caught, and dangling rows could
-- accumulate). `db.rs` already sets `PRAGMA foreign_keys=ON`, so declaring the FKs makes
-- the DB reject dangling rows AND lets `delete_profile` / `remove_mod` rely on
-- `ON DELETE CASCADE` instead of manual child-row deletes.
--
-- SQLite cannot add a foreign key to an existing table with `ALTER TABLE`; the supported
-- path is a table REBUILD (create new with the FKs, copy data, drop old, rename, recreate
-- indexes). This is done here in V3 — V2 is left untouched (T-02-01 additive) so any
-- already-migrated user DB upgrades cleanly.
--
-- Both `profile_mod` and `plugin_state` are LEAF tables (nothing references them), so the
-- drop+rename is safe under the foreign_keys pragma. The migration runs inside refinery's
-- transaction; toggling the foreign_keys pragma there is a no-op, so instead we first
-- delete any pre-existing dangling rows so the new constraints validate against clean data.

-- 1. Shed any dangling rows accumulated under V2's unconstrained tables, so the rebuilt
--    tables (which the FKs would otherwise reject) start referentially clean.
DELETE FROM profile_mod
 WHERE profile_id NOT IN (SELECT id FROM profile)
    OR mod_id     NOT IN (SELECT id FROM managed_mod);

DELETE FROM plugin_state
 WHERE profile_id NOT IN (SELECT id FROM profile);

-- 2. Rebuild profile_mod with FKs to profile(id) and managed_mod(id), both ON DELETE
--    CASCADE (so deleting a profile or a mod sheds its membership rows automatically).
CREATE TABLE profile_mod_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id INTEGER NOT NULL,
    mod_id     INTEGER NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 0,
    rank       INTEGER NOT NULL DEFAULT 1,
    UNIQUE (profile_id, mod_id),
    FOREIGN KEY (profile_id) REFERENCES profile (id) ON DELETE CASCADE,
    FOREIGN KEY (mod_id)     REFERENCES managed_mod (id) ON DELETE CASCADE
);

INSERT INTO profile_mod_new (id, profile_id, mod_id, enabled, rank)
SELECT id, profile_id, mod_id, enabled, rank FROM profile_mod;

DROP TABLE profile_mod;
ALTER TABLE profile_mod_new RENAME TO profile_mod;
CREATE INDEX idx_profile_mod_profile ON profile_mod (profile_id);

-- 3. Rebuild plugin_state with an FK to profile(id) ON DELETE CASCADE.
CREATE TABLE plugin_state_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id  INTEGER NOT NULL,
    plugin_name TEXT NOT NULL,
    kind        TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 0,
    order_index INTEGER NOT NULL DEFAULT 0,
    UNIQUE (profile_id, plugin_name),
    FOREIGN KEY (profile_id) REFERENCES profile (id) ON DELETE CASCADE
);

INSERT INTO plugin_state_new (id, profile_id, plugin_name, kind, enabled, order_index)
SELECT id, profile_id, plugin_name, kind, enabled, order_index FROM plugin_state;

DROP TABLE plugin_state;
ALTER TABLE plugin_state_new RENAME TO plugin_state;
CREATE INDEX idx_plugin_state_profile ON plugin_state (profile_id);
