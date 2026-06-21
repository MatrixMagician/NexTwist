-- V4: Nexus provenance (additive — mirrors V2's header discipline).
--
-- This migration is strictly ADDITIVE: it only CREATEs one new table + one index. It
-- never modifies any Phase-1/2 table (managed_game / deployed_file / op_journal /
-- vanilla_backup / managed_mod / profile / profile_mod / plugin_state) in place — no
-- in-place schema change and no destructive statement against an existing table — so the
-- reversible-deployment safety core and the pristine-restore guarantee are wholly
-- unaffected. V3 is the prior highest migration; this is V4.
--
-- One row per managed mod that came from NexusMods, recording where it was acquired
-- (mod id, file id, version, display name). The FK to managed_mod(id) is ON DELETE
-- CASCADE so deleting the mod sheds its provenance row automatically (like profile_mod
-- in V3). UNIQUE(mod_id) makes the row 1:1 with its managed_mod and lets the store use
-- an idempotent upsert.
CREATE TABLE nexus_source (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    mod_id       INTEGER NOT NULL,
    nexus_mod_id INTEGER NOT NULL,
    file_id      INTEGER NOT NULL,
    version      TEXT NOT NULL,
    display_name TEXT NOT NULL,
    UNIQUE (mod_id),
    FOREIGN KEY (mod_id) REFERENCES managed_mod (id) ON DELETE CASCADE
);

CREATE INDEX idx_nexus_source_mod ON nexus_source (mod_id);
