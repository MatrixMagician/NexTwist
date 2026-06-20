-- V1: NexTwist persistence schema.
--
-- This schema is a CONTRACT consumed by Plans 04/05 (deploy/purge engine). The
-- four tables below are the substrate for the core safety guarantees:
--   * managed_game   — ENV-03 game registry (resolved install/prefix/staging paths)
--   * deployed_file  — DEPLOY-02 per-file manifest (what we placed, how, and its hash)
--   * op_journal     — DEPLOY-06 write-ahead operation journal (intent-before-act,
--                      'pending' before the syscall, flipped 'done' after; crash
--                      recovery replays/rolls back any non-'done' rows on launch)
--   * vanilla_backup — DEPLOY-04 content-addressed backup-before-overwrite ledger
--                      (keyed by blake3 hash so the original is restorable on purge)

-- ENV-03: the games NexTwist manages and their resolved paths.
CREATE TABLE managed_game (
    appid       INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    install_dir TEXT NOT NULL,
    prefix      TEXT NOT NULL,
    staging_dir TEXT NOT NULL
);

-- DEPLOY-02: every file deployed into a game's Data/ tree.
-- UNIQUE(appid, target_rel) enforces one owner per deployed path.
CREATE TABLE deployed_file (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    appid        INTEGER NOT NULL,
    target_rel   TEXT NOT NULL,
    source_mod   INTEGER NOT NULL,
    method       TEXT NOT NULL,
    hash         TEXT NOT NULL,
    pre_existing INTEGER NOT NULL DEFAULT 0,
    UNIQUE (appid, target_rel)
);

CREATE INDEX idx_deployed_file_appid ON deployed_file (appid);

-- DEPLOY-06: write-ahead operation journal.
-- `state` defaults to 'pending'; the deploy engine (Plan 04) flips it to 'done'
-- after the idempotent syscall succeeds. On launch, any row not 'done' is replayed.
CREATE TABLE op_journal (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    appid       INTEGER NOT NULL,
    target_rel  TEXT NOT NULL,
    method      TEXT,
    source_hash TEXT,
    kind        TEXT NOT NULL,
    state       TEXT NOT NULL DEFAULT 'pending',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_op_journal_state ON op_journal (state);

-- DEPLOY-04: content-addressed vanilla backup ledger.
-- One backup per (appid, target_rel); `hash` is the blake3 hex of the original
-- file, which is what keys the on-disk backup blob (<app_data>/originals/<appid>/<hash>).
CREATE TABLE vanilla_backup (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    appid      INTEGER NOT NULL,
    target_rel TEXT NOT NULL,
    hash       TEXT NOT NULL,
    UNIQUE (appid, target_rel)
);

CREATE INDEX idx_vanilla_backup_hash ON vanilla_backup (hash);
