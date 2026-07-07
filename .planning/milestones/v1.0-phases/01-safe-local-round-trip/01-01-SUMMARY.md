---
phase: 01-safe-local-round-trip
plan: 01
subsystem: persistence-foundation
status: complete
tags: [rust, cargo-workspace, sqlite, rusqlite, refinery, blake3, cargo-deny, toolchain]
dependency_graph:
  requires: []
  provides:
    - "crates/core domain types (Game, ManagedMod, FileEntry, DeployMethod, error enums)"
    - "crates/store persistence (registry/manifest/op_journal/vanilla_backup) over WAL SQLite"
    - "crates/testkit byte-for-byte pristine tree assertion (snapshot_tree + assert_trees_identical)"
    - "Cargo workspace + pinned toolchain + cargo-deny ban policy"
  affects:
    - "Plan 02 (steam): consumes core::Game + store registry"
    - "Plan 03 (extract): consumes core types + testkit"
    - "Plan 04 (deploy): consumes store op_journal/manifest/vanilla + testkit pristine assertion"
    - "Plan 05 (purge): consumes store manifest/vanilla"
    - "Plan 06 (tauri): extends workspace members to include src-tauri"
tech_stack:
  added:
    - "rusqlite 0.39 (bundled)"
    - "refinery 0.9.2 (rusqlite feature)"
    - "blake3 1.8"
    - "walkdir 2.5"
    - "serde 1 / thiserror 2"
    - "cargo-deny 0.19.9"
    - "Rust toolchain 1.96.0 (2024 edition)"
  patterns:
    - "Multi-crate headless workspace; safety engine has zero Tauri deps"
    - "thiserror in libs, anyhow reserved for app boundary"
    - "All SQL encapsulated in store; no rusqlite types in public API"
    - "Operation journal: intent-before-act ('pending' default) over WAL + synchronous=FULL"
    - "Content-addressed (blake3) vanilla backup ledger"
key_files:
  created:
    - rust-toolchain.toml
    - Cargo.toml
    - .gitignore
    - deny.toml
    - crates/core/Cargo.toml
    - crates/core/src/lib.rs
    - crates/core/src/model.rs
    - crates/core/src/error.rs
    - crates/store/Cargo.toml
    - crates/store/src/lib.rs
    - crates/store/src/db.rs
    - crates/store/src/migrations/V1__init.sql
    - crates/store/src/registry.rs
    - crates/store/src/manifest.rs
    - crates/store/src/journal.rs
    - crates/store/src/vanilla.rs
    - crates/testkit/Cargo.toml
    - crates/testkit/src/lib.rs
  modified: []
decisions:
  - "Pin rusqlite 0.39 (not 0.40) so refinery 0.9.2 resolves a single libsqlite3-sys"
  - "DeployMethod persisted as lowercase token; from_token guards corrupt DB values"
  - "op_journal.state DEFAULT 'pending'; mark_done idempotent; pending_ops ordered by id"
  - "vanilla_backup UNIQUE(appid,target_rel), hash-indexed for blob dedupe"
metrics:
  duration_min: 20
  tasks_completed: 3
  files_created: 18
  tests_passing: 21
  completed: 2026-06-20
---

# Phase 1 Plan 01: Persistence & Shared-Types Foundation Summary

Scaffolded the NexTwist multi-crate Rust workspace and built its persistence spine: a WAL SQLite store (rusqlite bundled + refinery V1 migration) holding the game registry, per-file deploy manifest, write-ahead operation journal, and content-addressed vanilla backup table — plus shared `core` domain types, a `testkit` byte-for-byte pristine-tree assertion, and a cargo-deny policy banning non-free UnRAR code.

## What Was Built

- **Task 1 — Workspace scaffold + supply-chain gate** (`92a26ad`): `rust-toolchain.toml` (stable >= 1.85, 2024 edition), virtual-workspace `Cargo.toml` (`members=["crates/*"]`, `resolver=2`, pinned `[workspace.dependencies]`), `.gitignore` (excludes `/target`, `node_modules`, `*.db`; keeps `Cargo.lock`), and `deny.toml` banning `unrar`/`unrar_sys` with a permissive license allow-list.
- **Task 2 — core + store** (`e24229e`): `crates/core` domain types (`Game`, `ManagedMod`, `FileEntry`, `DeployMethod`) with serde + thiserror error enums and no I/O deps; `crates/store` opens a WAL DB (`synchronous=FULL`), runs the embedded refinery `V1__init.sql` creating `managed_game`/`deployed_file`/`op_journal`/`vanilla_backup`, and exposes registry/manifest/journal/vanilla facades with no rusqlite types leaking into the public API.
- **Task 3 — testkit** (`7eb4ef8`): `fake_game_tree`/`fake_staged_mod` builders, `snapshot_tree` (walkdir + blake3), and `assert_trees_identical` producing an actionable MUTATED/ORPHAN/MISSING diff — the pristine-assertion primitive Plan 04's round-trip and crash-recovery tests build on.

## Interfaces Provided (contract for Plans 02–06)

- `core`: `Game { appid, name, install_dir, prefix, staging_dir }`, `ManagedMod`, `FileEntry { target_rel, source_mod, method, hash, pre_existing }`, `DeployMethod { Reflink|Hardlink|Symlink|Copy }`, `StoreError`/`CoreError`.
- `store::Store::open(path)` → WAL + `synchronous=FULL` + refinery V1; `add_managed_game`/`list_managed_games`/`get_game`; `record_deployed_file`/`list_deployed_files`/`remove_deployed_file`; `begin_op`/`mark_done`/`pending_ops` (`OpIntent`/`JournalRow`/`JournalId`); `record_vanilla`/`backup_key_exists`/`vanilla_for`.
- `testkit::{fake_game_tree, fake_staged_mod, snapshot_tree, assert_trees_identical, TreeSnapshot}`.

## Verification

- `cargo build --workspace` — clean (crates/* members).
- `cargo test --workspace` — **21 passed** (2 core + 13 store + 6 testkit), 0 failed.
- `cargo deny check bans` — `bans ok` (unrar ban active on the real workspace graph).
- `cargo clippy --workspace --all-targets` — 0 warnings.
- Encapsulation check: no `rusqlite`/`Connection`/`Row` type in any store public `pub fn` signature.
- `PRAGMA journal_mode` returns `wal` after `open()`; `op_journal.state` defaults to `pending`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking dependency conflict] Pinned rusqlite 0.39 instead of 0.40**
- **Found during:** Task 2 (`cargo build -p store`)
- **Issue:** RESEARCH.md's compatibility table assumed refinery 0.9.2 targets rusqlite 0.40, but refinery-core 0.9.2 (latest) caps its `rusqlite` feature at `>=0.23, <=0.39`. rusqlite 0.40 (→ libsqlite3-sys 0.38) and refinery's transitive rusqlite (→ libsqlite3-sys 0.18/0.37) both declare `links = "sqlite3"`, so Cargo refused to resolve two copies of the native SQLite library.
- **Fix:** Pinned `rusqlite = "0.39"` (→ libsqlite3-sys 0.37, which refinery 0.9.2 accepts) and `refinery = "0.9.2"` in `[workspace.dependencies]`. Both now resolve to a single libsqlite3-sys 0.37. This keeps the locked decision intact (rusqlite **bundled** + refinery migrations) — only the rusqlite point version moved by one minor; no package substitution. Documented inline in `Cargo.toml` with a revisit note for when refinery gains rusqlite 0.40 support. Verified via crates.io index (`refinery-core` 0.9.0/0.9.1/0.9.2 rusqlite caps are `<=0.37`/`<=0.38`/`<=0.39`).
- **Files modified:** Cargo.toml
- **Commit:** e24229e

### Pre-existing environment note (not a deviation)

Task 1's stated hard blocker — "Rust toolchain is NOT installed" — was already resolved on this machine: rustup 1.29.0 with cargo/rustc **1.96.0** (well above the 1.85 / 2024-edition requirement, including stable `io::ErrorKind::CrossesDevices`) was present under `~/.cargo/bin` but not on `PATH`. No install was needed; the toolchain was put on `PATH` for the build. `cargo-deny` 0.19.9 was the only tool that needed installing (`cargo install cargo-deny --locked`).

## Notes for Downstream Plans

- **Plan 06** must extend `Cargo.toml` `members` to `["crates/*", "src-tauri"]` when the Tauri shell is created (noted in a comment in `Cargo.toml`).
- **Plan 04** owns the journal *protocol* (intent-before-act ordering, idempotent replay/rollback on launch); `store` deliberately provides only the durable row primitives.
- The V1 schema is a contract: `op_journal` (DEPLOY-06 substrate), `vanilla_backup` content-addressed (DEPLOY-04 substrate), `deployed_file` manifest (DEPLOY-02 substrate). Schema changes must be additive via a new refinery `Vn__*.sql`, never by editing V1 (protects existing user DBs).

## Known Stubs

None. Every facade is fully implemented and unit-tested; no placeholder/empty-return code.

## Self-Check: PASSED

- All 18 created files verified present on disk.
- All 3 commits verified in git log (`92a26ad`, `e24229e`, `7eb4ef8`).
- Full workspace test suite green (21/21); cargo-deny bans pass; clippy clean.
