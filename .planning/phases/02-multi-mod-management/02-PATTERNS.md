# Phase 2: Multi-Mod Management - Pattern Map

**Mapped:** 2026-06-20
**Files analyzed:** 16 (new/modified)
**Analogs found:** 14 / 16 (2 genuinely new: libloot wrapper, masterlist fetch)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/core/src/model.rs` (EXTEND) | model | transform | itself (same file) — `Game`/`ManagedMod`/`FileEntry` | exact (additive) |
| `crates/store/src/migrations/V2__multi_mod.sql` | migration | CRUD | `crates/store/src/migrations/V1__init.sql` | exact |
| `crates/store/src/mods.rs` | store query module | CRUD | `crates/store/src/registry.rs` | exact |
| `crates/store/src/profiles.rs` | store query module | CRUD | `crates/store/src/registry.rs` + `manifest.rs` | exact |
| `crates/store/src/plugins.rs` | store query module | CRUD | `crates/store/src/manifest.rs` | exact |
| `crates/deploy/src/conflict.rs` | service (resolver) | transform | `crates/deploy/src/engine.rs` (consumes `StagedFiles`) | role-match |
| `crates/loadorder/src/lib.rs` | crate root | — | `crates/deploy/src/lib.rs` / `crates/steam/src/lib.rs` | role-match |
| `crates/loadorder/src/scan.rs` | service | file-I/O | `crates/deploy/src/engine.rs` (walkdir over staged) | role-match |
| `crates/loadorder/src/loot.rs` | service (FFI/lib wrapper) | transform | NONE (libloot crate) — see No Analog | none |
| `crates/loadorder/src/masterlist.rs` | service | file-I/O + network | NONE (reqwest cache) — see No Analog | none |
| `crates/loadorder/src/error.rs` | error type | — | `crates/deploy/src/error.rs` | exact |
| `crates/loadorder/Cargo.toml` | config | — | `crates/deploy/Cargo.toml` | exact |
| `src-tauri/src/commands/conflicts.rs` | command adapter | request-response | `src-tauri/src/commands/deploy.rs` | exact |
| `src-tauri/src/commands/plugins.rs` | command adapter | request-response | `src-tauri/src/commands/deploy.rs` / `mods.rs` | exact |
| `src-tauri/src/commands/profiles.rs` | command adapter | request-response | `src-tauri/src/commands/deploy.rs` | exact |
| `frontend/src/routes/*` (conflict/plugin/profile views) + `frontend/src/lib/api.ts` (EXTEND) | component | request-response | `frontend/src/routes/+page.svelte` + `frontend/src/lib/api.ts` | exact |
| `crates/deploy/tests/profile_switch.rs` (+ conflict redeploy) | test | — | `crates/deploy/tests/crash_recovery.rs` | exact |

## Pattern Assignments

### `crates/core/src/model.rs` (model, additive extend)

**Analog:** itself. The header is an explicit contract: "field tweaks are allowed but the shapes are stable" and `ManagedMod.id`/`enabled` were "scaffolded in Phase 1 to pre-position Phase 2." Add new structs in the SAME style, do not restructure existing ones.

**Struct + serde pattern** (lines 18-44, 70-104): pure data, `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]`, doc comment per field, NO rusqlite/tauri/reqwest deps. For an enum with DB token persistence, copy `DeployMethod` exactly (lines 70-104): `#[serde(rename_all = "lowercase")]`, plus `as_str()` / `from_token()` for the store layer, plus a `*_token_round_trips` unit test (lines 110-121).

New types to add (CONTEXT/RESEARCH §Structure): add `priority`/`rank: u32` to `ManagedMod`; add `Profile`, `Plugin`, `PluginKind` (enum: Esm/Esl/Esp — model on `DeployMethod` for the token round-trip), `FileConflict { target_rel, providers: Vec<i64>, winner: i64 }`.

---

### `crates/store/src/migrations/V2__multi_mod.sql` (migration, CRUD)

**Analog:** `crates/store/src/migrations/V1__init.sql` (whole file).

**Migration conventions to copy:**
- Header comment block stating which Plan/requirement each table serves (V1 lines 1-11).
- `id INTEGER PRIMARY KEY AUTOINCREMENT` for row tables; natural PK where one exists (`managed_game.appid`, V1 line 15).
- `UNIQUE(...)` to enforce invariants — mirror `deployed_file UNIQUE(appid, target_rel)` (V1 lines 24-33). The per-profile membership table needs `UNIQUE(profile_id, mod_id)`; per-profile plugin state needs `UNIQUE(profile_id, plugin_name)`.
- `CREATE INDEX idx_<table>_<col>` after each table for the common lookup column (V1 lines 35, 51, 64).
- Booleans stored as `INTEGER NOT NULL DEFAULT 0` (V1 line 31 `pre_existing`).
- Auto-discovered by `embed_migrations!("src/migrations")` in `db.rs` (line 21) — no wiring needed beyond the filename `V2__*.sql`.

New tables (RESEARCH lines 145-149, 265-266): `managed_mod` (with rank/priority), `profile`, `profile_mod` (membership + per-profile rank), `plugin_state` (per-profile enable + order index). Include a data-migration step folding Phase-1 single-mod/deployed state into an auto-created "Default" profile per game.

**Migration test:** the existing test pattern is in `db.rs` lines 86-107 (`open_creates_db_in_wal_mode_with_tables` queries `sqlite_master`). Add a `migrations::v2_migrates_phase1_state` integration test that opens a V1 DB, reopens (runs V2), and asserts new tables + Default-profile rows exist.

---

### `crates/store/src/mods.rs` / `profiles.rs` / `plugins.rs` (store query modules, CRUD)

**Analog:** `crates/store/src/registry.rs` (whole file) and `crates/store/src/manifest.rs` (whole file).

**Locked store-module conventions (copy exactly):**
- `impl Store { ... }` — methods hang off the shared handle; NO `rusqlite` type in the public signature (registry.rs lines 1-13, manifest.rs lines 1-13). Callers speak `core::` types only.
- Insert/upsert: `INSERT OR REPLACE ... VALUES (?1, ...)` with `params![...]`, `.map_err(|e| StoreError::Db(e.to_string()))` (registry.rs lines 15-30, manifest.rs lines 17-34).
- List: `conn.prepare(...)` → `stmt.query_map([], row_to_X)` → `collect(rows)`, always `ORDER BY` a deterministic column (registry.rs lines 33-45 + `collect` helper lines 84-92).
- Single fetch returns `Option`: `match rows.next()` → `Ok(Some/None)` (registry.rs lines 48-63).
- Idempotent delete returns `bool` (`Ok(n > 0)`) — copy `remove_deployed_file` (manifest.rs lines 60-73).
- Row mapper is a free `fn row_to_X(row: &rusqlite::Row<'_>) -> rusqlite::Result<X>` (registry.rs lines 66-74); use `PathBuf::from(row.get::<_, String>(i)?)` for paths.
- Path→string persistence via the `path_str` lossy helper (registry.rs lines 80-82).
- For a token column that can be corrupt (e.g. `PluginKind`), use the double-Result row mapper + `StoreError::Corrupt` pattern from `manifest.rs` lines 76-98 and its `corrupt_method_token_surfaces_error` test (lines 148-162).

**Tests:** in-module `#[cfg(test)]` with `TempDir` + `Store::open(&dir.path().join("d.db"))`, asserting round-trip, upsert-by-key, and per-game scoping (registry.rs lines 94-135, manifest.rs lines 100-163). PROF-01/03 tests (`create_multiple`, `preserve_membership`) live here.

**Imports** (registry.rs lines 6-11):
```rust
use std::path::PathBuf;
use core::{Game, StoreError};
use rusqlite::params;
use crate::db::Store;
```

---

### `crates/deploy/src/conflict.rs` (resolver service, transform)

**Analog:** `crates/deploy/src/engine.rs` — the resolver's OUTPUT (`StagedFiles`, lines 132-138) is consumed by `deploy()` (line 109). It must emit exactly one entry per `target_rel` to satisfy `deployed_file UNIQUE(appid, target_rel)` (Pitfall 3).

**Output contract** (engine.rs lines 132-138): `StagedFiles { staging_root: PathBuf, files: Vec<PathBuf> }`. NOTE: today `StagedFiles` has ONE `staging_root`; multi-mod winners come from DIFFERENT staging roots, so the resolver likely needs to emit per-file (root, rel) pairs — plan must decide whether to extend `StagedFiles` or have the resolver emit a `Vec<(PathBuf staging_root, PathBuf rel)>` that the deploy loop iterates. Flag this as a small contract decision; keep `engine::deploy` the unchanged disk primitive.

**Path-safety pattern to reuse** (engine.rs lines 374-384 `guard_within_root`, lines 387-400 `lexical_normalize`): assert each winner path resolves inside its mod's staging root and the game `Data/` root (Security §V5/V12). Hash via `backup::blake3_file` (engine.rs line 195) so `FileEntry.hash` records the winning content.

**Resolver shape** (RESEARCH Pattern 1, lines 166-181): pure fold, `resolve(mods: &[ModInput]) -> (StagedFiles, Vec<FileConflict>)`, sort providers by rank, winner = lowest rank. Unit tests (CONF-01/02/03) go in this module's `#[cfg(test)]` — assert no duplicate `target_rel` in output.

---

### `crates/loadorder/` crate (new headless crate)

**Crate-structure analog:** `crates/deploy/` and `crates/steam/` (multi-file lib with `error.rs`).

**`error.rs` — copy `crates/deploy/src/error.rs` exactly** (whole file, 1-54): `thiserror` enum, `#[from] StoreError`, a structured `Io { path, source }` variant with a `fn io(path, source)` constructor (lines 46-53), domain-specific variants. Libs use `thiserror`, NEVER `anyhow` (that's the Tauri boundary only).

**`scan.rs` (plugin discovery, file-I/O):** mirror the engine's `walkdir`/staged-tree iteration; scan enabled mods' staged trees + game `Data/` for `.esp/.esm/.esl`. Reuse `guard_within_root` semantics for path confinement.

**`Cargo.toml`:** copy `crates/deploy/Cargo.toml` layout; add `libloot = "0.29"` (RESEARCH lines 68-74). `esplugin`/`libloadorder` only if libloot's `Plugin` API is insufficient. Planner inserts `checkpoint:human-verify` before first `libloot` install (SUS-but-approved, RESEARCH line 94) and adds the GPL-3.0 `cargo-deny` allowance (RESEARCH line 96, line 302).

**Steam prefix reuse:** build the libloot `local_path` from the resolved prefix — `<prefix>/drive_c/users/steamuser/AppData/Local/<GameName>`. `Game.prefix` already exists (`crates/core/src/model.rs` line 27); steam's `resolve` (`crates/steam/src/resolve.rs`) is the existing prefix source. ALWAYS use `Game::with_local_path` (Pitfall 1).

---

### `src-tauri/src/commands/{conflicts,plugins,profiles}.rs` (thin command adapters, request-response)

**Analog:** `src-tauri/src/commands/deploy.rs` (whole file) and `mods.rs` (whole file).

**Locked adapter rules** (commands/mod.rs lines 1-23 header + helpers):
- 3–10 lines per `#[tauri::command]`; ZERO safety/business logic.
- `state: State<'_, Mutex<AppState>>` first arg; `appid: u32` for game ops.
- Look up the game via the shared `require_game(&state, appid).await?` helper (mod.rs lines 28-39) — do NOT inline a store lookup.
- Call exactly ONE headless function; map errors with `.map_err(boundary_err)` (mod.rs lines 21-23) so the webview gets a `String`.
- Lock the store inline: `&state.lock().await.store` (deploy.rs line 24).

**Exact template** (deploy.rs lines 16-25):
```rust
#[tauri::command]
pub async fn deploy(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    staged: StagedMod,
) -> Result<DeployReport, String> {
    let game = require_game(&state, appid).await?;
    let work = StagedFiles { staging_root: staged.staging_root, files: staged.files };
    deploy::deploy(&state.lock().await.store, &game, &work).map_err(boundary_err)
}
```

**Registration:** add `pub mod conflicts; pub mod plugins; pub mod profiles;` to `commands/mod.rs` (lines 10-12) and wire each command into the Tauri `invoke_handler` (check `src-tauri/src/lib.rs` builder — same place `deploy/purge/verify` are registered). The profile-switch command delegates to the existing `purge` then `deploy` engine calls (RESEARCH Pattern 4).

---

### Frontend: `frontend/src/lib/api.ts` (EXTEND) + new Svelte views

**Analog:** `frontend/src/lib/api.ts` (whole file) and `frontend/src/routes/+page.svelte`.

**`api.ts` conventions** (lines 1-72): the ONLY place the UI calls `invoke`. One `export const <name> = (args): Promise<T> => invoke("<command>", { ...args })` per command (lines 55-72); mirror the snake_case backend command name. Define a matching `export interface`/`export type` per Rust return type (lines 8-48) — add `Profile`, `PluginInfo`, `FileConflict`, `SortProposal` to match the new core types.

**Svelte view conventions** (+page.svelte lines 1-86): Svelte 5 runes (`$state`, `$derived`); UI holds NO business logic / path resolution (line 2-3 header); every action wrapped in the `run(label, fn)` busy/error/status helper (lines 40-54); scoped CSS. "Functional-minimal" — match Phase-1 polish level. A UI-SPEC is generated before planning (CONTEXT line 65) and governs the conflict/plugin/profile view layout; honor masters-first grouping and propose-then-apply LOOT sort (no silent apply).

---

### `crates/deploy/tests/profile_switch.rs` (+ conflict-redeploy) (integration test)

**Analog:** `crates/deploy/tests/crash_recovery.rs` (lines 1-80, the `Fixture` harness).

**Test harness pattern to copy:**
- `Fixture` struct with `TempDir` root + `install`/`staging`/`db` paths and a `new()` that pre-creates `install/Data` and staging (lines 24-40).
- `fn game(&self) -> Game { ... }` building a Skyrim SE `Game` (appid 489830) (lines 42-50).
- `fn open_store(&self)` reopening the same DB + re-registering the game to simulate relaunch (lines 52-59).
- Lay vanilla + staged trees, `snapshot_tree(&install)` for the pristine baseline (lines 63-79).
- Assert pristine with `testkit::{snapshot_tree, assert_trees_identical}` (imports line 22; snapshot semantics in `crates/testkit/src/lib.rs` lines 87-114 — DIR_SENTINEL tracks empty-dir shape too; diff classifier lines 127-169).

**PROF-02 test:** `purge(old)` → `conflict::resolve(new)` → `deploy(new)` → assert pristine after a full purge (RESEARCH Pattern 4, lines 226-236). Pitfall 4: switch MUST purge-to-pristine between profiles.

## Shared Patterns

### Error handling (libs)
**Source:** `crates/deploy/src/error.rs` (whole file)
**Apply to:** `crates/loadorder/src/error.rs` (and `conflict`'s errors — likely reuse `DeployError`)
`thiserror` enum, `#[from] StoreError`, structured `Io { path, #[source] source }` + `fn io(path, e)` constructor. NEVER `anyhow` in a lib crate.

### Store query module shape
**Source:** `crates/store/src/registry.rs` + `manifest.rs`
**Apply to:** `mods.rs`, `profiles.rs`, `plugins.rs`
`impl Store`, `params![]`, `.map_err(|e| StoreError::Db(e.to_string()))`, free `row_to_X` mappers, `Option` for single fetch, `bool` for idempotent delete, `path_str` for paths, `ORDER BY` for determinism. No rusqlite types in public API.

### Thin Tauri adapter
**Source:** `src-tauri/src/commands/deploy.rs` + `commands/mod.rs` helpers
**Apply to:** `conflicts.rs`, `plugins.rs`, `profiles.rs`
3–10 lines, `require_game` lookup, single headless call, `.map_err(boundary_err)`.

### Path-confinement guard
**Source:** `crates/deploy/src/engine.rs` lines 374-400 (`guard_within_root`, `lexical_normalize`)
**Apply to:** conflict resolver (winner paths) + loadorder scan (plugin paths) — Security §V5/V12. Promote to a shared util if reused across crates, else copy the lexical-containment pattern.

### Pristine round-trip assertion
**Source:** `crates/testkit/src/lib.rs` (`snapshot_tree` lines 87-114, `assert_trees_identical` lines 127-169) via the `crash_recovery.rs` `Fixture`
**Apply to:** every new disk-mutating test (profile switch, conflict redeploy). DIR_SENTINEL means empty-dir shape is asserted too (GAP-01 already covered).

### Core type + DB-token enum
**Source:** `crates/core/src/model.rs` `DeployMethod` (lines 70-104) + round-trip test (110-121)
**Apply to:** new `PluginKind` enum (Esm/Esl/Esp). `as_str()`/`from_token()` for the store layer; `#[serde(rename_all = "lowercase")]`.

## No Analog Found

Files with no close codebase match (planner uses RESEARCH.md patterns — all VERIFIED against libloot source):

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `crates/loadorder/src/loot.rs` | libloot wrapper | transform | First libloot integration; use RESEARCH Pattern 2 (lines 184-212) — `Game::with_local_path`, `load_plugins`, `sort_plugins`, `set_load_order`, `is_plugin_active`. NEVER hand-roll plugins.txt. Planner should spike A1/A3 first (RESEARCH line 349). |
| `crates/loadorder/src/masterlist.rs` | masterlist fetch/cache | file-I/O + network | First `reqwest` use in the codebase; use RESEARCH Pattern 3 (lines 214-224) — fetch `loot/<game>` `v0.29` branch over rustls-tls, cache at `<app_data>/masterlists/<appid>/`, bundled snapshot fallback. CC0 data. |

(The crate scaffold, `error.rs`, `Cargo.toml`, and `scan.rs` DO have analogs — see Pattern Assignments — only the libloot/masterlist glue is genuinely new.)

## Metadata

**Analog search scope:** `crates/{core,store,deploy,steam,testkit}/src`, `crates/store/src/migrations`, `crates/deploy/tests`, `src-tauri/src/commands`, `frontend/src/{routes,lib}`
**Files scanned:** ~14 source files read in full or targeted
**Pattern extraction date:** 2026-06-20
**Cross-cutting note for planner:** Workspace MSRV bump 1.85 → 1.89 required for libloot (RESEARCH line 374) — early task, touches `rust-toolchain`/`Cargo.toml rust-version`/CI.
