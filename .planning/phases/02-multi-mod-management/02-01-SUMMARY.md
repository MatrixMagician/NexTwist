---
phase: 02-multi-mod-management
plan: 01
subsystem: persistence + core types
tags: [schema, migration, profiles, multi-mod, plugins, conflict, msrv, cargo-deny]
status: complete
requires:
  - "Phase-1 store (registry/manifest/journal/vanilla) + refinery V1 migration"
  - "Phase-1 core model (Game, ManagedMod, FileEntry, DeployMethod)"
provides:
  - "V2 schema: managed_mod, profile, profile_mod, plugin_state tables"
  - "Default-profile data migration (one active Default per managed_game)"
  - "core types: ManagedMod.rank, Profile, Plugin, PluginKind, FileConflict"
  - "Store CRUD: mods (rank), profiles (membership + active), plugin_state"
  - "MSRV 1.89 + cargo-deny libloot GPL-3.0 allowance (pre-positions Plan 04)"
affects:
  - "Plan 03 (conflict) — consumes managed_mod.rank + FileConflict + list_mods"
  - "Plan 04 (plugin/LOOT) — consumes plugin_state CRUD + PluginKind + libloot allowance"
  - "Plan 05 (profile switch) — consumes profile/profile_mod CRUD + set_active_profile"
tech-stack:
  added: []
  patterns:
    - "Additive-only refinery migration (CREATE + one INSERT; never ALTER/DROP/UPDATE V1)"
    - "Store-module facade: impl Store, no rusqlite in public signatures, row_to_X mapper"
    - "Double-Result corrupt-token row mapper -> StoreError::Corrupt (manifest.rs pattern)"
    - "Transactional one-active-flag invariant (set_active_profile clears then sets)"
    - "Refinery Target::Version(N) to reach a genuine V1-only state in the migration test"
key-files:
  created:
    - crates/store/src/migrations/V2__multi_mod.sql
    - crates/store/src/mods.rs
    - crates/store/src/profiles.rs
    - crates/store/src/plugins.rs
  modified:
    - crates/core/src/model.rs
    - crates/core/src/lib.rs
    - crates/store/src/db.rs
    - crates/store/src/lib.rs
    - Cargo.toml
    - rust-toolchain.toml
    - deny.toml
decisions:
  - "Phase-1 deployed_file membership is NOT folded into managed_mod/profile_mod; the live deployment stays on disk + reversible via the existing manifest, so the Default profile starts empty (D-16)."
  - "set_profile_mod / set_plugin_state use ON CONFLICT DO UPDATE (upsert) keyed by their UNIQUE constraints, matching the manifest INSERT OR REPLACE idempotency model."
  - "Migration test reaches a real V1-only state with refinery Target::Version(1) rather than hand-applying V1 SQL, so refinery's own history is correct and V2 applies as the genuine upgrade seam."
metrics:
  duration_min: 7
  tasks: 3
  files: 11
  tests_added: 19
  completed: 2026-06-21
---

# Phase 2 Plan 01: Multi-Mod Persistence + Type Substrate Summary

Additive V2 schema (managed_mod / profile / profile_mod / plugin_state) with an auto-Default-profile data migration, three typed Store CRUD modules, an extended core model (rank + Profile/Plugin/PluginKind/FileConflict), MSRV bumped to 1.89, and a cargo-deny libloot GPL-3.0 allowance — the stable persistence + type contract every other Phase-2 slice builds on.

## What Was Built

- **MSRV 1.89** (`Cargo.toml [workspace.package]`) for libloot (Plan 04). Subsumes the 1.85 EXDEV/`CrossesDevices` floor; `rust-toolchain.toml` channel stays `stable` (installed 1.96).
- **cargo-deny** documents + allows the `GPL-3.0-or-later` libloot family (libloot/libloadorder/esplugin, added Plan 04). No dependency added this plan.
- **Core model** extended additively: `ManagedMod.rank: u32`; new `Profile`, `Plugin`, `PluginKind {Esm,Esl,Esp}` (token round-trip), `FileConflict`. Headless invariant intact (no rusqlite/tauri/reqwest in `core`).
- **V2 migration**: four tables with UNIQUE invariants + indexes, strictly additive, plus `INSERT INTO profile … SELECT … FROM managed_game` creating one active Default profile per game.
- **Three Store modules** (`mods`, `profiles`, `plugins`) — typed CRUD, no rusqlite in public signatures.

## Store Method Signatures (the contract Plans 03/04/05 call)

```rust
// mods.rs
fn add_mod(&self, appid: u32, m: &ManagedMod) -> Result<i64, StoreError>;      // returns row id
fn list_mods(&self, appid: u32) -> Result<Vec<ManagedMod>, StoreError>;        // ORDER BY rank, id
fn get_mod(&self, id: i64) -> Result<Option<ManagedMod>, StoreError>;
fn set_mod_rank(&self, id: i64, rank: u32) -> Result<bool, StoreError>;        // idempotent
fn remove_mod(&self, id: i64) -> Result<bool, StoreError>;                     // idempotent

// profiles.rs
fn create_profile(&self, appid: u32, name: &str) -> Result<i64, StoreError>;   // inactive; UNIQUE(appid,name)
fn list_profiles(&self, appid: u32) -> Result<Vec<Profile>, StoreError>;       // ORDER BY id
fn active_profile(&self, appid: u32) -> Result<Option<Profile>, StoreError>;
fn set_active_profile(&self, appid: u32, profile_id: i64) -> Result<bool, StoreError>; // txn, exactly-one-active
fn set_profile_mod(&self, profile_id: i64, mod_id: i64, enabled: bool, rank: u32) -> Result<(), StoreError>; // upsert
fn list_profile_mods(&self, profile_id: i64) -> Result<Vec<(i64, bool, u32)>, StoreError>; // (mod_id, enabled, rank) ORDER BY rank
fn delete_profile(&self, profile_id: i64) -> Result<bool, StoreError>;         // cascades profile_mod + plugin_state

// plugins.rs
fn set_plugin_state(&self, profile_id: i64, plugin: &Plugin) -> Result<(), StoreError>; // upsert
fn list_plugin_state(&self, profile_id: i64) -> Result<Vec<Plugin>, StoreError>; // ORDER BY order_index; corrupt kind -> StoreError::Corrupt
```

## Core Type Shapes

```rust
struct ManagedMod { id: i64, name: String, staging_root: PathBuf, enabled: bool, rank: u32 } // rank added
struct Profile    { id: i64, appid: u32, name: String, active: bool }
enum   PluginKind { Esm, Esl, Esp }   // as_str() / from_token(); serde lowercase; Esm+Esl = master group
struct Plugin     { name: String, kind: PluginKind, enabled: bool, order: u32 }
struct FileConflict { target_rel: PathBuf, providers: Vec<i64>, winner: i64 } // ids are ManagedMod rows
```

## V2 Migration Is Additive-Only (confirmed)

`V2__multi_mod.sql` contains only `CREATE TABLE` / `CREATE INDEX` and a single `INSERT INTO profile … SELECT … FROM managed_game`. It performs **no** `ALTER`/`DROP`/`UPDATE`/`DELETE` against any V1 table, so Phase-1 deploy state (manifest / journal / vanilla backups) is preserved and the pristine-restore guarantee is unaffected (T-02-01 mitigated).

## BLOCKING Migration Test

`crates/store/src/db.rs::v2_migrates_phase1_state` reaches a genuine **V1-only** state via `migrations::runner().set_target(Target::Version(1))`, asserts V2 tables are absent, seeds a `managed_game` as Phase-1 would, then `Store::open` applies **only V2** over it and asserts: (1) all four V2 tables exist, (2) exactly one active `Default` profile exists for the seeded appid, (3) the V1 `managed_game` row survived. This exercises the real Phase-1 → Phase-2 upgrade seam, not a fresh-DB shortcut.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking gate] Pre-existing `clippy::collapsible_if` across Phase-1 crates**
- **Found during:** Task 3 verification (`cargo clippy --workspace --all-targets -- -D warnings`).
- **Issue:** The 1.96 clippy toolchain flags nested `if` / `if let` blocks (now collapsible into let-chains, stable since 1.88) as errors under `-D warnings`. These blocks were authored in Phase-1 (commit `e24229e` and the steam/deploy crates) and were untouched by this plan, but the plan's completion gate requires a clean workspace clippy run, so they blocked completion.
- **Fix:** Mechanical collapse to let-chains, no behavior change. Files: `crates/store/src/db.rs` (Store::open), `crates/deploy/src/engine.rs`, `crates/deploy/src/lib.rs`, `crates/deploy/src/verify.rs` (x2), `crates/deploy/tests/fs_probe.rs`, `crates/deploy/tests/method_ladder.rs`, `crates/steam/src/discover.rs`, `crates/steam/src/resolve.rs`.
- **Commits:** db.rs collapse landed in the Task-3 commit `4edf99b`; the remaining Phase-1 crate collapses landed in `4a44859`.

No other deviations — the schema/type/store work followed the plan exactly.

## Verification Results

- `cargo test -p nextwist-store` — 27 passed (incl. BLOCKING `v2_migrates_phase1_state`).
- `cargo test -p nextwist-core model::` — 7 passed (token + serde round-trips).
- `cargo test --workspace` — **101 passed** (Phase-1 baseline 82 + 19 new; additive, no regressions).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok (GPL-3.0-or-later allowed).
- `grep -q 'rust-version = "1.89"' Cargo.toml` — true.

## Notes for Downstream Plans

- `set_active_profile` and `delete_profile` run in `unchecked_transaction()`; the connection is single-threaded (`Store` wraps one `Connection`), so this is safe and keeps the one-active-flag invariant atomic.
- `list_profile_mods` returns a `(mod_id, enabled, rank)` tuple, not a struct — Plan 05 should join against `list_mods` to materialize full `ManagedMod`s for a profile.
- The Default profile is created **empty** (no membership rows). Plan 05's first profile-switch over a fresh upgrade will deploy nothing until mods are added — by design (D-16).

## Self-Check: PASSED

- FOUND: crates/store/src/migrations/V2__multi_mod.sql
- FOUND: crates/store/src/mods.rs
- FOUND: crates/store/src/profiles.rs
- FOUND: crates/store/src/plugins.rs
- FOUND commit d0cfc29 (Task 1), b0201d1 (Task 2), 4edf99b (Task 3), 4a44859 (deviation)
