-- V2: multi-mod / profile / plugin substrate (Phase 2).
--
-- This migration is strictly ADDITIVE (T-02-01): it only CREATEs new tables and
-- indexes and performs ONE INSERT into the new `profile` table. It never ALTERs,
-- DROPs, or UPDATEs any V1 table (managed_game / deployed_file / op_journal /
-- vanilla_backup), so Phase-1 deployment state (manifest / journal / vanilla
-- backups) is preserved and the pristine-restore guarantee is unaffected.
--
-- Tables added (which requirement each serves):
--   * managed_mod  — D-01/D-13 multi-mod registry per game, with a rank that orders
--                    conflict winners (LOWER rank = HIGHER priority = wins).
--   * profile      — D-13/D-16 a lightweight reference set over the shared staging
--                    store; exactly one active profile per game.
--   * profile_mod  — D-13/D-14 per-profile membership: which mods are enabled and at
--                    what rank, independently per profile.
--   * plugin_state — D-07/D-13 per-profile plugin enable + load-order state.
--
-- Data migration (D-16): every existing managed_game gets one active 'Default'
-- profile so Phase-1 single-mod users land in a usable multi-mod world on upgrade.
-- Phase-1 had NO managed_mod rows — a single deployed mod existed only as
-- deployed_file rows. Folding deployed_file membership into a managed_mod/profile_mod
-- set is unnecessary: Phase-1 deployment is already on disk and reversible via the
-- existing manifest, so the Default profile starts empty and the live deployment is
-- untouched.

-- D-01/D-13: the mods NexTwist manages for a game. `rank` is 1-based; lower wins.
CREATE TABLE managed_mod (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    appid        INTEGER NOT NULL,
    name         TEXT NOT NULL,
    staging_root TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 0,
    rank         INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX idx_managed_mod_appid ON managed_mod (appid);

-- D-13/D-16: profiles. UNIQUE(appid, name) rejects duplicate names per game.
CREATE TABLE profile (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    appid  INTEGER NOT NULL,
    name   TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 0,
    UNIQUE (appid, name)
);

CREATE INDEX idx_profile_appid ON profile (appid);

-- D-13/D-14: per-profile membership + per-profile rank.
-- UNIQUE(profile_id, mod_id) enforces one membership row per (profile, mod).
CREATE TABLE profile_mod (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id INTEGER NOT NULL,
    mod_id     INTEGER NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 0,
    rank       INTEGER NOT NULL DEFAULT 1,
    UNIQUE (profile_id, mod_id)
);

CREATE INDEX idx_profile_mod_profile ON profile_mod (profile_id);

-- D-07/D-13: per-profile plugin enable + load-order state.
-- `kind` stores the PluginKind token (esm/esl/esp). order_index is 0-based.
-- UNIQUE(profile_id, plugin_name) enforces one row per plugin per profile.
CREATE TABLE plugin_state (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id  INTEGER NOT NULL,
    plugin_name TEXT NOT NULL,
    kind        TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 0,
    order_index INTEGER NOT NULL DEFAULT 0,
    UNIQUE (profile_id, plugin_name)
);

CREATE INDEX idx_plugin_state_profile ON plugin_state (profile_id);

-- D-16 data migration: one active 'Default' profile per existing managed game.
INSERT INTO profile (appid, name, active)
SELECT appid, 'Default', 1 FROM managed_game;
