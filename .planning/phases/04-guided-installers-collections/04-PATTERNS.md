# Phase 4: Guided Installers & Collections - Pattern Map

**Mapped:** 2026-06-21
**Files analyzed:** 17 new/modified files
**Analogs found:** 15 / 17 (2 net-new parse/resolve concerns map to RESEARCH AST spec, no codebase analog)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/fomod/Cargo.toml` | config | — | `crates/nexus/Cargo.toml` | exact (crate-shape) |
| `crates/fomod/src/lib.rs` | model (crate root) | — | `crates/nexus/src/lib.rs` | exact (crate-shape) |
| `crates/fomod/src/error.rs` | model (error enum) | — | `crates/nexus/src/error.rs` | exact |
| `crates/fomod/src/model.rs` | model (typed AST) | transform | RESEARCH §FOMOD Schema Reference (XSD) | **no analog** (new) |
| `crates/fomod/src/parse.rs` | utility (XML→AST) | transform | RESEARCH Pattern 1 + `quick-xml` docs | **no analog** (new) |
| `crates/fomod/src/condition.rs` | utility (evaluator) | transform | RESEARCH Pattern 2 | partial (logic, no analog) |
| `crates/fomod/src/resolve.rs` | service (dry-run resolver) | transform | RESEARCH Pattern 3 | partial (logic, no analog) |
| `crates/nexus/src/collection.rs` | model + parser | transform | `crates/store/src/nexus.rs` (serde+pure) / `client.rs` | role-match |
| `crates/nexus/src/resolve.rs` | service (availability) | request-response | `crates/nexus/src/client.rs` (`download_link`/`mod_file_metadata`) | exact |
| `crates/store/src/migrations/V5__collections.sql` | migration | CRUD | `V4__nexus_provenance.sql` | exact |
| `crates/store/src/collections.rs` | store query module | CRUD | `crates/store/src/nexus.rs` / `profiles.rs` | exact |
| `crates/extract/src/staging.rs` (modify) | utility (staging) | file-I/O | self (`install_archive` — add root-detection) | self |
| `src-tauri/src/commands/fomod.rs` | controller (adapter) | request-response | `commands/downloads.rs` (thin adapter shape) | role-match |
| `src-tauri/src/commands/collections.rs` | controller (adapter) | request-response | `commands/downloads.rs` + `profiles.rs` | exact |
| `src-tauri/src/commands/mod.rs` (modify) | route registration | — | self (`pub mod` list) | self |
| `src-tauri/src/lib.rs` (modify) | route registration | — | self (`generate_handler!`) | self |
| `frontend/src/routes/+page.svelte` + `frontend/src/lib/api.ts` (modify) | component + provider | request-response | self (existing `invoke` wrapper + UI primitives) | self |

## Pattern Assignments

### `crates/fomod/Cargo.toml` (config)

**Analog:** `crates/nexus/Cargo.toml`

**Crate-shape pattern** (`crates/nexus/Cargo.toml:10-31`): a `[lib]` with a short `name` (`fomod`) distinct from the package (`nextwist-fomod`), and the **`nextwist_core` alias rule** — depend on core as `package = "nextwist-core"` aliased to `nextwist_core`, **never** `core`, because a dep literally named `core` shadows std `::core` and breaks `thiserror`'s derive (which expands to `::core::fmt`). Since `fomod` derives `thiserror` locally, it MUST use the `nextwist_core` alias:
```toml
[lib]
name = "fomod"
path = "src/lib.rs"

[dependencies]
nextwist_core = { path = "../core", package = "nextwist-core" }
quick-xml = { workspace = true }   # add to root [workspace.dependencies] with features = ["serialize"]
serde.workspace = true
thiserror.workspace = true
tracing.workspace = true
walkdir.workspace = true

[dev-dependencies]
nextwist-testkit = { workspace = true }
tempfile.workspace = true
```
**HARD INVARIANT (mirrors the `nexus` header comment, lines 20-22):** no `tauri`, `reqwest`, `keyring`, or `tauri-plugin-*` dep. State this as a Cargo.toml comment exactly as `nexus` does. Add `quick-xml = { version = "0.40", features = ["serialize"] }` to root `[workspace.dependencies]` (verification checklist line 514: the `serialize` feature is required for serde support).

---

### `crates/fomod/src/lib.rs` (crate root)

**Analog:** `crates/nexus/src/lib.rs` (lines 24-39)

**Module + re-export pattern:** declare `pub mod {error, model, parse, condition, resolve};` then re-export the public surface with `pub use` (mirrors `nexus/src/lib.rs:24-38`):
```rust
pub mod condition;
pub mod error;
pub mod model;
pub mod parse;
pub mod resolve;

pub use error::FomodError;
pub use model::{FomodModule, InstallStep, Group, Plugin, FileInstall /* … */};
pub use parse::parse_module_config;
pub use resolve::resolve;
```
The crate-level doc comment should state the headless/Tauri-free invariant and the parse→condition→resolve split (mirrors the `nexus` lib.rs doc block lines 1-22).

---

### `crates/fomod/src/error.rs` (error enum)

**Analog:** `crates/nexus/src/error.rs` (entire file, esp. lines 9-42)

**thiserror + external-error-flattening pattern:** the convention (stated in the `nexus` error doc, lines 1-7) is `thiserror` enums in libs, **anyhow only at the Tauri boundary, NEVER here**; wrap `StoreError` via `#[from]`, tag I/O with the offending `PathBuf`, and **flatten external-library errors to a `String`** so the foreign error type never crosses the crate boundary (`nexus` flattens reqwest/oauth2 → `String` exactly this way, lines 33-42):
```rust
use std::path::PathBuf;
use nextwist_core::StoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FomodError {
    #[error("xml parse error: {0}")]
    Xml(String),                              // flatten quick_xml::DeError → String (like NexusError::Http)
    #[error("malformed FOMOD: {0}")]
    MalformedSchema(String),                  // the locked "fail clearly, never mis-install" variant
    #[error("source not found in archive: {0}")]
    MissingSource(String),
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error("i/o error for {path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },
}
```
Also copy the `pub(crate) fn io(path, source)` constructor convention (`nexus/src/error.rs:70-76`).

---

### `crates/fomod/src/{model,parse,condition,resolve}.rs` (typed AST + parser + evaluator + resolver)

**Analog:** none in codebase — this is the genuinely new engine code. Map directly to RESEARCH:
- `model.rs` → **RESEARCH §FOMOD Schema Reference** (the full XSD element/attribute/enum tree: `config`/`moduleName`/`moduleDependencies`/`requiredInstallFiles`/`installSteps`→`group`(5 types)→`plugin`→`typeDescriptor`(static `type` OR `dependencyType`+`patterns`), 5-state `pluginType` enum, `fileSystemItem` attrs `source`/`destination`/`priority`/`alwaysInstall`/`installIfUsable`, `conditionalFileInstalls`, `compositeDependency` `And`/`Or`). Serde-derive against **local names** with `#[serde(default)]` on every optional element (RESEARCH Pitfall 5).
- `parse.rs` → **RESEARCH Pattern 1** (`quick_xml::de::from_str`; case-insensitive `fomod/`+`ModuleConfig.xml` locate via `walkdir`; strip BOM; RESEARCH Pitfall 3).
- `condition.rs` → **RESEARCH Pattern 2** (`eval(dep, flags, files)` recursive composite-dependency evaluator; plugin type-state walk of `dependencyType.patterns`).
- `resolve.rs` → **RESEARCH Pattern 3** (pure `resolve(module, sel) -> Result<Vec<FileInstall>, FomodError>`; `requiredInstallFiles` + selected files + `conditionalFileInstalls`; dedup by `(dest_rel, priority desc)`; **NO `std::fs` write** — the dry-run safety gate, verification checklist line 517).

**Output contract for downstream reuse:** `resolve` emits `Vec<FileInstall { src, dest_rel, priority, always }>` rooted at `Data/`, which feeds the (extended) `extract` staging move and then `deploy::conflict::resolve` (which expects `Data/`-rooted trees).

---

### `crates/nexus/src/resolve.rs` (Collection availability resolver)

**Analog:** `crates/nexus/src/client.rs` — `download_link` (lines 139-158) and `mod_file_metadata` (line 218)

**Request-response + rate-limit pattern:** reuse the client's existing REST-v1 file-info / `download_link.json` calls to confirm each pinned `(modId, fileId)` exists/is-archived. Every request goes through `self.limiter.until_ready().await` FIRST (the proactive gate, `client.rs:148`) and `error_for_status()`. Classify per **RESEARCH Pattern 4 / Source-type table**: `nexus`→Available/Archived/Unavailable, `bundle`→Available, `direct`/`browse`/`manual`→Manual (never fetched). Emit a `ResolveReport` (`enum ModStatus { Available, Archived, Unavailable, Manual }`) with **zero downloads** before the report is accepted (checklist line 518).

---

### `crates/nexus/src/collection.rs` (collection.json parser)

**Analog:** `crates/store/src/nexus.rs` (serde+pure, `core`-types-in/out discipline)

**Pure-parse pattern:** parse `collection.json` with `serde_json` (already a `nexus` dep, `Cargo.toml:28`) into a typed `Collection { info, mods, mod_rules }` per **RESEARCH §Collection Manifest Reference** (Vortex `ICollection`/`ICollectionMod`/`ICollectionSourceInfo`/`ICollectionModRule` + the `IChoices` FOMOD replay encoding). Keep it a pure transform — no I/O, no Tauri.

---

### `crates/store/src/migrations/V5__collections.sql` (migration)

**Analog:** `crates/store/src/migrations/V4__nexus_provenance.sql` (entire file)

**Additive refinery migration pattern:** copy V4's exact discipline (lines 1-26) — a header comment asserting the migration is **strictly additive** (only `CREATE TABLE`/`CREATE INDEX`, never modifies a Phase-1/2 table in place, so the reversibility core is unaffected), an `INTEGER PRIMARY KEY AUTOINCREMENT`, **FK to the owning row `ON DELETE CASCADE`**, a `UNIQUE(...)` for idempotent upsert, and a covering `CREATE INDEX`. V4 is the current highest → this is **V5** (checklist line 515). Tables: `collection`, `collection_mod` (FK→collection, FK→managed_mod CASCADE), `fomod_choice` (FK→collection_mod CASCADE, stores the replayed `IChoices` JSON).

---

### `crates/store/src/collections.rs` (store query module)

**Analog:** `crates/store/src/nexus.rs` (entire file)

**Store-facade pattern (no rusqlite in public API — hard invariant):** mirror `nexus.rs` exactly — `use core::{<NewType>, StoreError};`, `impl Store { … }`, all SQL inside `store`, **`core` types in/out only**, no `rusqlite` type in any public signature (checklist line 515; `nexus.rs` doc lines 1-7). Use the single-statement `INSERT … ON CONFLICT … DO UPDATE … RETURNING id` upsert (`nexus.rs:26-47`, WR-05 atomic-upsert) and a `fn row_to_<type>(row: &rusqlite::Row) -> rusqlite::Result<…>` mapper (`nexus.rs:69-77`). Register the module in `crates/store/src/lib.rs` (add `mod collections;` alongside `mod nexus;` at line 26). Copy the `#[cfg(test)] mod tests` round-trip + CASCADE-delete test shape (`nexus.rs:79-156`).

---

### `crates/extract/src/staging.rs` (MODIFY — close the carried root-detection gap)

**Analog:** self — `install_archive` (lines 37-76)

**Where to add it:** after extraction validates into `temp_root` (line 61) and **before** `move_into_staging(temp_root, staging_root)` (line 64). Add a root-detection pre-pass: if `temp_root`'s top level is a **single directory** that itself contains the recognizable game root (`Data/`, or known top-level files like `SKSE/`), treat that subdirectory as the move source (RESEARCH Pitfall 1). This is the **carried Phase-2 gap**: `list_files_rel` (line 70) lists verbatim with **no flattening**, so a `MyMod/Data/foo.esp` archive currently deploys to `Data/MyMod/Data/foo.esp`. FOMOD `<file>/<folder>` `source` resolution depends on the **detected** root, case-insensitively. Make detection explicit + unit-tested (wrapper / no-wrapper / nested-wrapper / FOMOD-with-wrapper fixtures — Wave-0 gap line 571). Do not alter the validated extract→validate→move→read-only ordering.

---

### `src-tauri/src/commands/fomod.rs` & `collections.rs` (thin adapters)

**Analog:** `src-tauri/src/commands/downloads.rs` (the canonical thin-adapter, esp. doc lines 1-15, `start_download` lines 59-98, `run_download_to_window` line 100)

**Thin-adapter pattern (no business logic — Anti-Pattern-4):** the adapter (a) resolves the managed game + session auth via `require_game` / `appid_for_domain` (`downloads.rs:24`, `commands/mod.rs:40`), (b) **locks `State<'_, Mutex<AppState>>`** and calls the headless crate, (c) maps errors via `boundary_err` (`commands/mod.rs:26`). For Collection bulk download, reuse `run_download_to_window` (`downloads.rs:100`) **verbatim** per mod, reusing the shared `governor` limiter from `AppState` (`client.rs:84 with_limiter`) and bounding concurrency with a small semaphore (RESEARCH Pattern 5). **Premium gate:** read `UserInfo.is_premium` first; non-Premium → return the "Collections require a NexusMods Premium account" notice and do **not** start (checklist line 520). Progress events use the `window.emit("download://progress", …)` + `ProgressEvent` payload pattern (`downloads.rs:21,29-41`).

**Deploy/uninstall in `collections.rs`** (RESEARCH Patterns 8-9, no new primitive):
```rust
// deploy: create_profile → set_profile_mod(rank_from_rules) per mod → deploy::switch_profile
let pid = store.create_profile(game.appid, &collection_name)?;   // store/profiles.rs:17
for m in mods { store.set_profile_mod(pid, m.mod_id, rank, true)?; } // profiles.rs:112
deploy::switch_profile(&store, &game, pid)?;                     // deploy/profile.rs:78
// uninstall: purge-to-pristine → delete_profile → remove staged mods
deploy::purge(&store, &game)?;                                   // deploy/engine.rs:401
store.delete_profile(pid)?;                                      // profiles.rs:167 (rejects active)
```
Note `delete_profile` **rejects an active profile** (`profiles.rs:167-185`) — uninstall must `purge`/switch away first.

---

### `src-tauri/src/commands/mod.rs` + `src-tauri/src/lib.rs` (MODIFY — registration)

**Analog:** self.
- `commands/mod.rs:10-17`: add `pub mod fomod;` and `pub mod collections;` to the `pub mod` list.
- `src-tauri/src/lib.rs:102-125`: add each new `#[tauri::command]` to `tauri::generate_handler![…]` (e.g. `commands::fomod::parse_fomod`, `commands::fomod::resolve_fomod`, `commands::fomod::apply_fomod`, `commands::collections::resolve_collection`, `commands::collections::download_collection`, `commands::collections::deploy_collection`, `commands::collections::uninstall_collection`) following the existing `commands::profiles::*` / `commands::downloads::*` entries.

---

### `frontend/src/lib/api.ts` + `frontend/src/routes/+page.svelte` (MODIFY)

**Analog:** self.
- `api.ts:1-6`: the **only** place the UI calls the backend — a thin typed `invoke` wrapper with **no business logic, no path resolution**. Add `invoke(...)` wrappers + `interface` report mirrors for each new command, mirroring the existing `DeployReport`/`PurgeReport`/`StagedMod` interfaces (lines 24-57) that 1:1 mirror the Rust report structs. Progress for the Collection bulk path reuses the existing `listen`/`UnlistenFn` pattern (line 6) on `download://progress`.
- `+page.svelte`: add the FOMOD wizard view (radio for `SelectExactlyOne`/`SelectAtMostOne`, checkbox for `SelectAny`/`SelectAtLeastOne`/`SelectAll`; live re-eval calls `fomod::condition` via the adapter; dry-run conflict preview) and the Collections browse/resolve/progress view, reusing the existing `.modal`/`.report`/`.bar-track`/`button.cta` CSS primitives the UI-SPEC names.

## Shared Patterns

### thiserror error flattening (libs)
**Source:** `crates/nexus/src/error.rs:1-42`
**Apply to:** `crates/fomod/src/error.rs`, the Collection resolver
`thiserror` enums in engine crates; `anyhow` ONLY at the Tauri boundary. Flatten foreign errors (`quick_xml::DeError`, `serde_json::Error`) to `String` so they never cross the crate boundary; wrap `StoreError` via `#[from]`; tag I/O with `PathBuf`.

### `nextwist_core` alias (never bare `core`)
**Source:** `crates/nexus/Cargo.toml:14-18` (comment) + `crates/nexus/src/error.rs:11`
**Apply to:** any new crate deriving `thiserror` (`crates/fomod`)
Depend on `nextwist-core` aliased to `nextwist_core`; a bare `core` dep shadows std `::core` and breaks the derive.

### Store facade — no rusqlite in public API
**Source:** `crates/store/src/nexus.rs:1-77`
**Apply to:** `crates/store/src/collections.rs`
`core` types in/out, all SQL inside `store`, atomic `RETURNING`-id upsert, `row_to_*` mappers, `#[cfg(test)]` round-trip + CASCADE tests.

### Additive refinery migration
**Source:** `crates/store/src/migrations/V4__nexus_provenance.sql:1-26`
**Apply to:** `V5__collections.sql`
Header asserting additive-only; AUTOINCREMENT PK; FK `ON DELETE CASCADE`; `UNIQUE` for idempotent upsert; covering index. V5 (V4 is current highest).

### Thin Tauri adapter (no business logic)
**Source:** `src-tauri/src/commands/downloads.rs:1-15,59-98`; `src-tauri/src/commands/mod.rs:26-54`
**Apply to:** `commands/fomod.rs`, `commands/collections.rs`
Lock `State<Mutex<AppState>>`, call the headless crate, map errors via `boundary_err`, resolve game via `require_game`/`appid_for_domain`, emit progress via `window.emit`. Reuse `run_download_to_window` + the shared `governor` limiter verbatim for bulk download.

### Reuse the safe deploy engine (no new primitive)
**Source:** `crates/deploy/src/profile.rs:78 switch_profile`; `crates/deploy/src/engine.rs:401 purge`; `crates/store/src/profiles.rs:17,112,167`; `crates/loadorder/src/loot.rs:301 apply_load_order`; `crates/testkit/src/lib.rs:121 snapshot_tree,161 assert_trees_identical`
**Apply to:** `commands/collections.rs` deploy/uninstall + the COLL-04/05 integration tests
Collection deploy = `create_profile`→`set_profile_mod`→`switch_profile` (which internally purges→deploys winners→`apply_load_order`→sets active). Uninstall = `purge`→`delete_profile`→remove staged mods. Round-trip pristine asserted by the testkit blake3 harness.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `crates/fomod/src/model.rs` | model (AST) | transform | First XML-schema-derived AST in the codebase; map to RESEARCH §FOMOD Schema Reference (the XSD enumeration is the spec). |
| `crates/fomod/src/parse.rs` | utility | transform | No existing `quick-xml`/serde XML parse in-repo; map to RESEARCH Pattern 1. |

(`condition.rs` and `resolve.rs` are pure new logic but follow the crate's own error/module conventions; their algorithms come from RESEARCH Patterns 2-3.)

## Metadata

**Analog search scope:** `crates/{nexus,store,deploy,extract,loadorder,testkit}`, `src-tauri/src/commands/`, `src-tauri/src/lib.rs`, `frontend/src/{lib,routes}`
**Files scanned:** ~18 (crate Cargo.toml/lib/error, V4 migration, store nexus/profiles, deploy profile/engine, extract staging, nexus client/download, downloads adapter, commands mod/lib registration, frontend api)
**Pattern extraction date:** 2026-06-21
