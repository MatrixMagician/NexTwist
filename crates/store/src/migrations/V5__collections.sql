-- V5: Collection acquisition substrate (COLL-01/02 — Phase 4).
--
-- This migration is strictly ADDITIVE (mirrors V4's header discipline): it only
-- CREATEs new tables + indexes. It NEVER ALTERs, DROPs, or UPDATEs any prior
-- Phase-1/2/3 table (managed_game / deployed_file / op_journal / vanilla_backup /
-- managed_mod / profile / profile_mod / plugin_state / nexus_source) in place — no
-- in-place schema change and no destructive statement against an existing table — so
-- the reversible-deployment safety core and the byte-for-byte pristine-restore
-- guarantee are wholly unaffected. V4 is the prior highest migration; this is V5.
--
-- Tables added (which requirement each serves):
--   * collection      — COLL-01 a NexusMods Collection revision pinned for a game:
--                        (appid, slug, revision) identify the revision; name is the
--                        display label; profile_id optionally links the dedicated
--                        Phase-2 profile a deployed Collection lives in (Plan 04).
--                        UNIQUE(appid, slug, revision) makes the row idempotent on
--                        re-resolve (the store upserts on that key).
--   * collection_mod  — COLL-02 one pinned mod inside a collection, carrying its Nexus
--                        source identity (nexus_mod_id/file_id/md5), install `phase`,
--                        conflict `rank`, and a link to the local managed_mod it
--                        stages into. Both FKs are ON DELETE CASCADE so deleting the
--                        collection OR the managed_mod sheds the link. UNIQUE(collection_id,
--                        mod_id) is one membership row per (collection, mod).
--   * fomod_choice    — COLL-03 substrate: the replayed FOMOD IChoices for a collection
--                        mod, stored as the manifest's `choices` JSON (TEXT). FK to
--                        collection_mod ON DELETE CASCADE so it sheds with its mod.
--
-- A deleted collection therefore CASCADE-removes its collection_mod rows, and each of
-- those CASCADE-removes its fomod_choice row.

-- COLL-01: a pinned Collection revision for a game. profile_id is NULLable: it is set
-- only once the Collection is materialised into its dedicated profile (Plan 04). The
-- FK to profile(id) is ON DELETE SET NULL so dropping that profile does not delete the
-- collection record (the resolve report + pinned-mod list survive a profile teardown).
CREATE TABLE collection (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    appid      INTEGER NOT NULL,
    slug       TEXT NOT NULL,
    revision   INTEGER NOT NULL,
    name       TEXT NOT NULL,
    profile_id INTEGER,
    UNIQUE (appid, slug, revision),
    FOREIGN KEY (profile_id) REFERENCES profile (id) ON DELETE SET NULL
);

CREATE INDEX idx_collection_appid ON collection (appid);
CREATE INDEX idx_collection_profile ON collection (profile_id);

-- COLL-02: one pinned mod inside a collection. `mod_id` links the local managed_mod the
-- mod stages into. Both FKs CASCADE so the link sheds when either parent is deleted.
CREATE TABLE collection_mod (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    collection_id INTEGER NOT NULL,
    mod_id        INTEGER NOT NULL,
    nexus_mod_id  INTEGER NOT NULL,
    file_id       INTEGER NOT NULL,
    md5           TEXT,
    phase         INTEGER NOT NULL DEFAULT 0,
    rank          INTEGER NOT NULL DEFAULT 1,
    UNIQUE (collection_id, mod_id),
    FOREIGN KEY (collection_id) REFERENCES collection (id) ON DELETE CASCADE,
    FOREIGN KEY (mod_id) REFERENCES managed_mod (id) ON DELETE CASCADE
);

CREATE INDEX idx_collection_mod_collection ON collection_mod (collection_id);
CREATE INDEX idx_collection_mod_mod ON collection_mod (mod_id);

-- COLL-03: the replayed FOMOD choices for a collection mod, stored as the manifest's
-- `choices` JSON verbatim (TEXT). 1:1 with its collection_mod (UNIQUE) and CASCADEs.
CREATE TABLE fomod_choice (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    collection_mod_id INTEGER NOT NULL,
    choices_json      TEXT NOT NULL,
    UNIQUE (collection_mod_id),
    FOREIGN KEY (collection_mod_id) REFERENCES collection_mod (id) ON DELETE CASCADE
);

CREATE INDEX idx_fomod_choice_collection_mod ON fomod_choice (collection_mod_id);
