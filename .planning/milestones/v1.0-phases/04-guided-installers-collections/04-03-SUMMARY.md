---
phase: 04-guided-installers-collections
plan: 03
subsystem: collections-acquisition
status: complete
tags: [collections, nexus, store, migration, resolve-gate, coll-01, coll-02]
dependency_graph:
  requires: ["04-01"]
  provides:
    - "store::add_collection / get_collection / remove_collection / add_collection_mod / list_collection_mods (V5 facade)"
    - "core::Collection / core::CollectionMod domain types"
    - "nexus::collection::Collection parser (ICollection / collection.json)"
    - "nexus::resolve::resolve_collection -> ResolveReport (resolve-before-download gate)"
    - "nexus::client::file_availability / FileAvailability"
  affects:
    - "04-04 (consumes Collection, ResolveReport, and the store facade to drive install/deploy/uninstall)"
tech_stack:
  added: []
  patterns:
    - "Additive V5 migration mirroring V4 discipline (AUTOINCREMENT PK, FK ON DELETE CASCADE, UNIQUE idempotent-upsert, covering indexes)"
    - "Store facade: core types in/out, single-statement RETURNING-id upsert, no rusqlite in public API"
    - "Pure serde_json parser for untrusted manifest (namespace of nexus.rs pure-parse discipline)"
    - "Resolve-before-download gate: metadata reads only, off-Nexus classified from type alone (no request)"
key_files:
  created:
    - crates/store/src/migrations/V5__collections.sql
    - crates/store/src/collections.rs
    - crates/nexus/src/collection.rs
    - crates/nexus/src/resolve.rs
    - crates/nexus/tests/collection_mock.rs
    - crates/nexus/tests/fixtures/collection.json
  modified:
    - crates/core/src/model.rs
    - crates/core/src/lib.rs
    - crates/store/src/lib.rs
    - crates/store/src/db.rs
    - crates/nexus/src/client.rs
    - crates/nexus/src/lib.rs
decisions:
  - "collection.profile_id FK is ON DELETE SET NULL (not CASCADE): dropping the dedicated profile in a Plan-04 uninstall keeps the collection + resolve history, only NULLs the link."
  - "nexus file availability is read from the existing v1 file-info endpoint (proven path): 404 => Unavailable, category_name == ARCHIVED => Archived, else Available — added client.file_availability rather than overloading mod_file_metadata so the resolver path is download-free by construction."
  - "Off-Nexus sources (direct/browse/manual) are classified Manual from source.type ALONE — resolver issues NO network request for them, so the off-Nexus url is never contacted (T-04-08 SSRF mitigation)."
  - "fomod_choice stores the manifest choices JSON verbatim as TEXT (1:1 with collection_mod, CASCADE); choices_json carried on core::CollectionMod and cleared on upsert when None."
  - "Stale modRule references are parsed and retained but never assumed to match a resolved mod (Pitfall 4); rank-mapping that consumes them lands in Plan 04."
metrics:
  duration_min: 7
  completed: 2026-06-21
  tasks: 2
  files_created: 6
  files_modified: 6
---

# Phase 04 Plan 03: Collection Acquisition Foundation Summary

Built the Collection acquisition foundation (COLL-01/COLL-02): a pure `collection.json`
parser into a typed `Collection`, an availability resolver that classifies every pinned
mod BEFORE any download or disk write, and an additive V5 store migration persisting
collections + per-mod FOMOD choices through a rusqlite-free facade — the resolve-before-
download safety gate is provable (zero downloads, off-Nexus never fetched).

## What Was Built

### Task 1 — V5 migration + store facade + core types (commit cbf1e2e)
- **`V5__collections.sql`** (additive): three tables — `collection`
  (`UNIQUE(appid, slug, revision)`, `profile_id` FK→profile `ON DELETE SET NULL`),
  `collection_mod` (FK→collection + FK→managed_mod, both `ON DELETE CASCADE`,
  `UNIQUE(collection_id, mod_id)`), `fomod_choice` (FK→collection_mod `ON DELETE CASCADE`,
  `UNIQUE(collection_mod_id)`). AUTOINCREMENT PKs, covering indexes on every FK column.
  Header asserts strict additivity (only CREATE TABLE/INDEX; never touches a prior table).
- **`core::Collection` / `core::CollectionMod`**: pure domain types (re-exported from
  `core/lib.rs`) with serde round-trip tests.
- **`store/collections.rs`**: `add_collection` / `get_collection` / `remove_collection` /
  `add_collection_mod` / `list_collection_mods` — single-statement `RETURNING id` upserts
  (WR-05), `core` types in/out, NO `rusqlite` type in any public signature (grep-proven).
  `add_collection_mod` upserts the FOMOD `choices_json` into `fomod_choice` (or deletes the
  row when `None`). Tests: round-trip, idempotent-upsert (both UNIQUE keys), CASCADE
  (collection→mods→choices; managed_mod→link), profile SET NULL, V5 migration-reach.

### Task 2 — collection.json parser + availability resolver (commit 66c091f)
- **`nexus/collection.rs`**: pure `serde_json` parser for the Vortex `ICollection` shape
  → typed `Collection { info, mods, mod_rules }`. Models `SourceInfo`/`SourceType`
  (nexus/bundle/direct/browse/manual), `modId`/`fileId`/`md5`/`fileSize`, the `IChoices`
  FOMOD replay (`Choices`/`ChoiceStep`/`ChoiceGroup`/`ChoiceOption`), `phase`,
  `fileOverrides`, `patches`, and `modRules[]` over `ModReference`. Malformed input flattens
  to a `NexusError` (never panics — untrusted input, T-04-08/T-04-11).
- **`nexus/resolve.rs`**: `resolve_collection(client, game_domain, &Collection)` →
  `ResolveReport` (`ModStatus { Available, Archived, Unavailable, Manual }`). `nexus` →
  a single rate-limited `client.file_availability` metadata read; `bundle` → Available;
  `direct`/`browse`/`manual` → Manual with **no request issued**. `ResolveReport::all_available`
  / `manual_steps` helpers.
- **`client.file_availability` + `FileAvailability`**: a download-free v1 file-info read —
  404 ⇒ Unavailable, `category_name == ARCHIVED` ⇒ Archived, else Available; `until_ready()`
  first, `note_headers` reactive. Extended `V1FileInfo` with `category_name`.
- **Fixture + `collection_mock.rs`**: a real-shaped `collection.json` (7 mods spanning every
  source type + a phantom modRule) and four tests — fixture parse, classify-all-source-types
  with an `expect(0)` download-link guard mock (proving zero downloads), shared-limiter
  `until_ready` via a 429, and stale-modRule-not-fatal.

## Deviations from Plan

### Auto-fixed Issues
None — plan executed as written.

### Notes (within plan scope, Claude's-discretion items the plan delegated)
- **`client.file_availability` added** rather than reusing `mod_file_metadata` for the nexus
  availability check. The plan's `key_links` named `mod_file_metadata|download_link` as the
  pattern; a dedicated metadata-only classifier keeps the resolver download-free *by
  construction* (it has no access to a download path) and surfaces Archived/Unavailable
  cleanly from the file `category_name`/404. Same endpoint, same limiter gating — a
  refinement of the named pattern, not a departure.
- **`collection.profile_id` FK is `ON DELETE SET NULL`** (the plan said the collection links
  an optional `profile_id`; it did not specify the on-delete action). SET NULL preserves the
  collection + resolve history across a Plan-04 profile teardown, which is the safer choice
  for the reversible-uninstall flow.

## Threat Model Adherence
- **T-04-08 (SSRF, off-Nexus auto-fetch)** — mitigated: `direct`/`browse`/`manual` classified
  `Manual` from `source.type` alone; the resolver issues no request for them (proven by the
  off-Nexus-only test making zero network calls).
- **T-04-09 (stale modRule)** — mitigated: phantom references parse and are retained but never
  assumed to match a resolved mod (`stale_mod_rule_reference_is_not_fatal`).
- **T-04-10 (resolve gate bypass)** — mitigated: resolve issues only metadata reads; the
  `expect(0)` download-link guard mock asserts zero download requests.
- **T-04-11 (unbounded manifest)** — accepted: `serde_json` bounds allocation; per-mod reads
  are rate-limited via the shared governor.
- **T-04-SC (package installs)** — mitigated: NO new dependency added (`serde_json` already a
  nexus dep); `cargo deny check advisories bans licenses sources` is green.

## Verification

| Check | Result |
|-------|--------|
| `cargo test -p nextwist-store` | 44 passed (8 new collection tests) |
| `cargo test -p nextwist-nexus` | 21 passed (3 collection unit + 4 collection_mock) |
| `cargo test -p nextwist-core` | 5 passed (2 new Collection serde round-trips) |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo deny check advisories bans licenses sources` | advisories/bans/licenses/sources ok |
| grep: no `rusqlite` in `collections.rs` public signatures | NONE (clean) |

## Success Criteria
- [x] A real Collection revision manifest parses into a typed Collection (COLL-01).
- [x] Every pinned mod is classified into a resolve report with zero downloads before
  acceptance (COLL-02, success criterion #2).
- [x] Off-Nexus sources are Manual and never fetched (COLL-02).
- [x] The V5 store layer persists collections + FOMOD choices additively and CASCADE-deletes
  cleanly.

## Known Stubs
None. No hardcoded empty data flows to UI; the rank-from-modRules mapping and the bulk
download/install/deploy are explicitly Plan-04 scope (consumers of this foundation), not stubs.

## Handoff to Plan 04
Plan 04 consumes: `nexus::Collection` (parsed manifest), `nexus::resolve_collection`/
`ResolveReport` (the accepted gate), and the `store` collection facade. Still to do in Plan 04:
map `modRules` (`after`/`before`/`conflicts`/`fileOverrides`) → mod rank + load order;
bulk-download the Available set (Premium gate via `UserInfo.is_premium`); replay each mod's
`choices` headlessly through `crates/fomod`; create the dedicated profile; deploy via
`switch_profile`; reversible uninstall via `purge`.

## Self-Check: PASSED
- FOUND: crates/store/src/migrations/V5__collections.sql
- FOUND: crates/store/src/collections.rs
- FOUND: crates/nexus/src/collection.rs
- FOUND: crates/nexus/src/resolve.rs
- FOUND: crates/nexus/tests/collection_mock.rs
- FOUND: crates/nexus/tests/fixtures/collection.json
- FOUND commit: cbf1e2e (Task 1)
- FOUND commit: 66c091f (Task 2)
