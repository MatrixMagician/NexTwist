# Phase 3: NexusMods Login & Download - Pattern Map

**Mapped:** 2026-06-21
**Files analyzed:** 19 new/modified files
**Analogs found:** 17 / 19 (2 no-analog: OAuth orchestration + nxm:// deep-link wiring — shell OS-integration with no existing precedent)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/nexus/Cargo.toml` | config | — | `crates/loadorder/Cargo.toml` + `crates/extract/Cargo.toml` | exact (headless crate) |
| `crates/nexus/src/lib.rs` | crate-root | — | `crates/loadorder/src/lib.rs` | exact |
| `crates/nexus/src/error.rs` | model (error) | — | `crates/loadorder/src/error.rs` | exact |
| `crates/nexus/src/auth.rs` | service | request-response | `crates/loadorder/src/masterlist.rs` (HTTP + injectable fetcher) | role-match |
| `crates/nexus/src/client.rs` | service | request-response | `crates/loadorder/src/masterlist.rs` (`real_fetch`: rustls client, `Policy::none()`, `error_for_status`) | role-match |
| `crates/nexus/src/download.rs` | service | streaming | `crates/loadorder/src/masterlist.rs` (HTTP fetch shape) | partial (streaming is new) |
| `crates/nexus/src/ratelimit.rs` | utility | transform | — (governor wrapper; no analog) | no-analog |
| `crates/nexus/src/model.rs` | model | — | `crates/core/src/model.rs` (serde DTOs + token round-trip) | role-match |
| `crates/nexus/tests/client_mock.rs` | test | request-response | `crates/loadorder/src/masterlist.rs` `#[cfg(test)]` (injectable-fetcher tests) | role-match |
| `crates/store/src/migrations/V4__nexus_provenance.sql` | migration | — | `crates/store/src/migrations/V2__multi_mod.sql` (additive CREATE) / `V3__profile_fks.sql` | exact |
| `crates/store/src/nexus.rs` (query module) | model (store facade) | CRUD | `crates/store/src/mods.rs` / `profiles.rs` (no-rusqlite-in-API facade) | exact |
| `crates/core/src/model.rs` (modify `ManagedMod` + new `NexusSource`) | model | — | `crates/core/src/model.rs` `ManagedMod`/`Profile` (additive serde fields) | exact |
| `src-tauri/src/keyring.rs` | service | — | — (OS Secret Service; no analog) | no-analog (research Pattern 2 verbatim) |
| `src-tauri/src/auth/` (OAuth orchestration) | service | request-response | `src-tauri/src/commands/plugins.rs` `spawn_blocking` offload pattern | partial |
| `src-tauri/src/commands/nexus.rs` | controller (adapter) | request-response | `src-tauri/src/commands/mods.rs` / `plugins.rs` | exact |
| `src-tauri/src/commands/downloads.rs` | controller (adapter) | streaming/event | `src-tauri/src/commands/mods.rs` (install_archive terminus) | role-match |
| `src-tauri/src/commands/mod.rs` (modify) | config | — | `src-tauri/src/commands/mod.rs` (module decls + shared helpers) | exact |
| `src-tauri/src/lib.rs` (modify: plugins + handlers) | config/bootstrap | event-driven | `src-tauri/src/lib.rs` (`setup()` + `generate_handler!`) | exact |
| `src-tauri/src/state.rs` (modify: Nexus client + tokens) | store | — | `src-tauri/src/state.rs` `AppState` | exact |
| `frontend/src/lib/api.ts` (modify) | utility (IPC bridge) | request-response | `frontend/src/lib/api.ts` (typed `invoke` wrappers) | exact |
| `frontend/src/routes/+page.svelte` (modify: account + downloads) | component | event-driven | `frontend/src/routes/+page.svelte` (Svelte 5 `$state`) | exact |

## Pattern Assignments

### `crates/nexus/Cargo.toml` (headless crate config)

**Analogs:** `crates/loadorder/Cargo.toml`, `crates/extract/Cargo.toml`

**Copy verbatim:** the package header (workspace-inherited fields), the `[lib]` block with a short `name`, and — CRITICALLY — the `nextwist_core` aliasing comment + dependency line. This is load-bearing: a dep literally named `core` shadows `::core` and breaks `thiserror`'s derive.

```toml
# crates/loadorder/Cargo.toml:10-28 — copy this shape
[lib]
name = "nexus"           # short alias, like loadorder/extract
path = "src/lib.rs"

# Aliased as `nextwist_core` (NOT `core`): a dependency literally named `core`
# shadows the std `::core` crate, which breaks `thiserror`'s derive ...
[dependencies]
nextwist_core = { path = "../core", package = "nextwist-core" }
store.workspace = true          # speaks core types via store facade
reqwest.workspace = true        # already pinned rustls; add json+stream features to root pin
serde.workspace = true
thiserror.workspace = true
tracing.workspace = true
# NEW to root [workspace.dependencies]: oauth2, governor, futures-util
oauth2 = { workspace = true }
governor = { workspace = true }
futures-util = { workspace = true }

[dev-dependencies]
nextwist_core = { path = "../core", package = "nextwist-core" }
mockito = { workspace = true }   # NEW — no HTTP mock exists in the repo
tempfile.workspace = true
```

**Guardrail (from research checklist):** NO `tauri`/`keyring`/`tauri-plugin-*` dep here. `reqwest` stays `default-features = false` + `rustls` (never native-tls).

---

### `crates/nexus/src/lib.rs` (crate root)

**Analog:** `crates/loadorder/src/lib.rs:16-27`

Copy the module-declaration + curated re-export shape: a doc comment stating the headless/Tauri-free invariant, then `pub mod` declarations, then `pub use` of the public surface.

```rust
// loadorder/src/lib.rs:16-27 pattern
pub mod auth;
pub mod client;
pub mod download;
pub mod error;
pub mod model;
pub mod ratelimit;

pub use error::NexusError;
pub use client::NexusClient;
pub use model::{ModFile, DownloadLink, NxmLink, UserInfo};
```

---

### `crates/nexus/src/error.rs` (thiserror enum)

**Analog:** `crates/loadorder/src/error.rs` (entire file — closest possible match)

Copy the structure exactly: `#[derive(Debug, Error)]`, a `#[from] StoreError` variant, an `Io { path, source }` struct variant with a `pub(crate) fn io(...)` constructor, and string-flattened variants for external library errors (loadorder flattens libloot → `String`; nexus flattens reqwest/oauth2 → `String` so those types never cross the crate boundary).

```rust
// loadorder/src/error.rs:16-55 — mirror this, renamed for Nexus
#[derive(Debug, Error)]
pub enum NexusError {
    #[error("store error: {0}")]
    Store(#[from] StoreError),

    #[error("i/o error for {path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },

    #[error("http error: {0}")]          // flatten reqwest to String (like Loot(String))
    Http(String),

    #[error("auth error: {0}")]          // oauth2 / token-exchange failures flattened
    Auth(String),

    #[error("rate limited; retry after {0}s")]
    RateLimited(u64),

    #[error("download-link redemption failed: {0}")]  // free-user key/expires path
    Redeem(String),
}
// + the io() constructor at loadorder/src/error.rs:57-65
```

---

### `crates/nexus/src/client.rs` + `auth.rs` (async HTTP client)

**Analog:** `crates/loadorder/src/masterlist.rs:167-184` (`real_fetch`) — the security-hardened reqwest pattern to copy, **converted from blocking to async**.

Copy these load-bearing client-builder choices (they are security-reviewed in loadorder and the research Security Domain re-mandates them):

```rust
// loadorder/src/masterlist.rs:167-184 — async equivalent for crates/nexus
let client = reqwest::Client::builder()         // async, NOT reqwest::blocking
    .redirect(reqwest::redirect::Policy::none())// disable redirect-following (V9 / SSRF guard)
    .build()
    .map_err(|e| NexusError::Http(e.to_string()))?;
let resp = client.get(url).send().await
    .map_err(|e| NexusError::Http(e.to_string()))?;
let resp = resp.error_for_status()              // validate status before reading body
    .map_err(|e| NexusError::Http(e.to_string()))?;
```

**Testable-core / injectable-fetcher pattern** — copy from `masterlist.rs:103-147` (`ensure_masterlist_with_fetcher<F>`): the public fn delegates to a generic-over-`F` core so tests inject a stub. For nexus, `mockito` replaces the stub closure (it stands up a real local server), but keep the same public-thin / testable-core split.

**OAuth (`auth.rs`):** no codebase analog — follow RESEARCH Pattern 1 (oauth2 5.0 PKCE) verbatim. Keep the `code_verifier` in memory only; never `tracing::` a token (research V7).

---

### `crates/nexus/src/download.rs` (streaming download)

**Analog:** partial — the HTTP-call shape is `masterlist.rs:167-184`; the streaming body has no analog. Follow RESEARCH Pattern 4.

Key constraints (anti-patterns from research): stream via `resp.bytes_stream()` + `futures_util::StreamExt` — NEVER `resp.bytes().await` (OOM on multi-GB packs). The progress callback is a plain `Fn(u64, Option<u64>)` with **no Tauri type** (the shell wraps it to `window.emit`). Write to a temp/staging-adjacent path with `tokio::fs::File`.

---

### `crates/store/src/migrations/V4__nexus_provenance.sql` (additive migration)

**Analog:** `crates/store/src/migrations/V2__multi_mod.sql` (additive CREATE shape) — NOT V3 (V3 is a table-rebuild; this is purely additive).

**CRITICAL (research anti-pattern):** the file is **`V4`**, not `V3` — `V3__profile_fks.sql` already exists.

Copy V2's discipline: a header comment stating the migration is strictly ADDITIVE (never ALTER/DROP/UPDATE a Phase-1/2 table), then `CREATE TABLE` + `CREATE INDEX`. Mirror V2 column conventions (`INTEGER PRIMARY KEY AUTOINCREMENT`, `... NOT NULL DEFAULT`, FK with `ON DELETE CASCADE` referencing `managed_mod(id)` like `profile_mod` at `V3__profile_fks.sql:38-39`).

```sql
-- V4: Nexus provenance (additive — mirrors V2 header discipline).
-- One row per managed mod that came from NexusMods. FK CASCADE so deleting the
-- mod sheds its provenance (like profile_mod in V3).
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
```

---

### `crates/store/src/nexus.rs` (store facade — query module)

**Analog:** `crates/store/src/mods.rs` (entire file — closest CRUD facade) + `crates/store/src/profiles.rs`

**Hard invariant (re-stated by research):** no `rusqlite` type in the public API. Copy `mods.rs` exactly:
- `use core::{<DTO>, StoreError};` + `use rusqlite::params;`
- `impl Store { ... }` with `.execute(...).map_err(|e| StoreError::Db(e.to_string()))?` and `self.conn.last_insert_rowid()` for inserts (`mods.rs:18-33`).
- A free `fn row_to_<x>(row: &rusqlite::Row) -> rusqlite::Result<T>` + a `collect_<x>` helper (`mods.rs:90-108`).
- A `#[cfg(test)]` round-trip test with a `TempDir` + `Store::open` (`mods.rs:110-141`).

Idempotency convention: deletes/updates return `bool` from `n > 0` (`mods.rs:69-87`).

Register the module in `crates/store/src/lib.rs` alongside `mods`/`profiles`.

---

### `crates/core/src/model.rs` (additive — modify `ManagedMod`, add `NexusSource`)

**Analog:** `crates/core/src/model.rs` itself — `ManagedMod` (lines 34-46) and `Profile` (54-64).

Field shapes are a stable contract (file header) — **additive only**. Add a `NexusSource` DTO mirroring the existing pattern: `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]`, `PathBuf`/`String`/integer fields, doc comment per field. Add a serde round-trip test mirroring `managed_mod_serde_round_trips` (lines 237-249). For token-style enums (e.g. auth state), copy the `as_str`/`from_token` + `#[serde(rename_all = "lowercase")]` pattern from `PluginKind` (71-101).

```rust
// Mirror ManagedMod's shape (model.rs:34-46)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NexusSource {
    pub mod_id: i64,          // local managed_mod id
    pub nexus_mod_id: u64,
    pub file_id: u64,
    pub version: String,
    pub display_name: String,
}
```

---

### `src-tauri/src/commands/nexus.rs` + `downloads.rs` (thin adapters)

**Analog:** `src-tauri/src/commands/mods.rs` (the cleanest thin adapter) + `plugins.rs`

Copy the adapter contract from `commands/mod.rs:1-26` (the doc comment is the law: no business logic, lock state, call ONE headless fn, map error to `String`). Concrete skeleton from `mods.rs:17-25`:

```rust
// mods.rs:17-25 — copy this exact shape
#[tauri::command]
pub async fn generate_download(
    state: State<'_, Mutex<AppState>>,
    appid: u32,
    /* nexus_mod_id, file_id, key?, expires? */
) -> Result<..., String> {
    let game = require_game(&state, appid).await?;       // shared helper, mod.rs:31-42
    // delegate to nexus client held in AppState; map error at boundary:
    state.lock().await.nexus.<fn>(...).await.map_err(boundary_err)
}
```

**For the download terminus (NEXUS-06):** reuse `mods.rs:24` verbatim — `extract::install_archive(&downloaded_archive, &game.staging_dir).map_err(boundary_err)` — then persist provenance via the new `store.nexus.rs` facade. The downloaded archive is indistinguishable from a local one here.

**For streaming progress events:** `commands/plugins.rs:235-240` shows the `tauri::async_runtime::spawn_blocking` offload precedent; for async streaming the analog is thinner — the shell wraps the headless `Fn(u64, Option<u64>)` callback into `window.emit("download://progress", ...)`. Use `boundary_err` (`mod.rs:24`) and `require_game` (`mod.rs:31-42`) unchanged.

---

### `src-tauri/src/keyring.rs` (no analog — follow research Pattern 2)

No existing OS-Secret-Service code. Follow RESEARCH Pattern 2 verbatim. The hard invariant (NEXUS-02): no-backend → `Err(NoKeyringBackend)`, NEVER a plaintext file. Map this error in the shell to the UI banner. Keyring lives ONLY in the shell — `crates/nexus` receives a token *value*, never a keyring handle. Use `keyring = "3.6"` (NOT 4.x).

---

### `src-tauri/src/state.rs` (modify `AppState`)

**Analog:** `src-tauri/src/state.rs:14-28` (entire `AppState`)

Keep it thin (the file's own warning: business logic lives in headless crates). Add the async `NexusClient` and the in-memory access token to `AppState`; extend `AppState::init` (lines 22-27) to construct the client. The refresh token is NOT held here — it lives in the keyring.

---

### `src-tauri/src/lib.rs` (modify — plugins + handler registration)

**Analog:** `src-tauri/src/lib.rs:52-89`

Two edits, both following the existing shape:
1. Add the new commands to `invoke_handler![ ... ]` (lines 65-86) — list `commands::nexus::*` and `commands::downloads::*` exactly like the existing `commands::plugins::*` entries.
2. In `.setup()` (lines 57-64) — register plugins. **Order is load-bearing (research Pitfall):** `tauri_plugin_single_instance::init(...)` FIRST (with `deep-link` feature), THEN `tauri_plugin_deep_link::init()`, THEN `app.deep_link().register_all()` + `on_open_url(...)` inside setup — alongside the existing `recover_all_on_launch` call which already runs before the UI is served.

---

### `frontend/src/lib/api.ts` (modify — typed IPC wrappers)

**Analog:** `frontend/src/lib/api.ts` (entire file)

Copy the exact convention: a TS `interface` mirroring each new Rust DTO (with a `mirrors core::X` comment, like lines 14-21), then a `export const fn = (args): Promise<T> => invoke("command_name", { args })` one-liner per command (lines 109-165). Field names must match the Rust serde output (snake_case crossing the boundary). Add `NexusSource`, `UserInfo`, `DownloadItem` interfaces + `login`/`logout`/`accountInfo`/`startDownload` wrappers.

For download-progress events, use `@tauri-apps/api/event` `listen(...)` (new import) — there is no existing event-listen analog in api.ts; this is the one new bridge primitive.

---

### `frontend/src/routes/+page.svelte` (modify — account panel + downloads list)

**Analog:** `frontend/src/routes/+page.svelte:1-60`

Copy the Svelte 5 conventions: `import * as api from "$lib/api"` + `import type {...}`, `let x = $state<T>(initial)` for every reactive value (lines 27-60), and the "functional-minimal, no business logic / path resolution in the UI" altitude stated in the file header (lines 1-4). Add account state (`loggedIn`, `userInfo`) and a `downloads = $state<DownloadItem[]>([])` list driven by the progress-event listener.

## Shared Patterns

### Headless/Shell Boundary (the defining invariant)
**Sources:** `crates/loadorder/src/lib.rs:1-14` (headless doc), `src-tauri/src/commands/mod.rs:1-8` (thin-adapter doc)
**Apply to:** ALL `crates/nexus/*` (zero Tauri/keyring deps) + ALL `src-tauri/src/commands/*` (no logic).
The HTTP client, OAuth token-exchange, rate limiter, streaming, and Nexus DTOs are headless. Keyring, deep-link, OAuth-redirect capture, single-instance, and `window.emit` are shell-only.

### Error Handling (thiserror in engine, boundary_err at shell)
**Source:** `crates/loadorder/src/error.rs` (full pattern) + `src-tauri/src/commands/mod.rs:24-26` (`boundary_err`)
**Apply to:** `crates/nexus/src/error.rs` (new `NexusError` enum, flatten reqwest/oauth2 to `String`); every command adapter maps via `boundary_err`. `anyhow` only at the shell (`state.rs:23` `AppState::init`).

### Hardened HTTP client (rustls, no-redirect, error_for_status)
**Source:** `crates/loadorder/src/masterlist.rs:167-184` (`real_fetch`)
**Apply to:** `crates/nexus/src/client.rs`, `auth.rs`, `download.rs` — async equivalent. `Policy::none()` (SSRF/redirect guard), `error_for_status()`, rustls-only. These are security-reviewed choices, not stylistic.

### Store facade (no rusqlite in public API)
**Source:** `crates/store/src/mods.rs` (full) + `profiles.rs:8-25`
**Apply to:** `crates/store/src/nexus.rs` — `core` types in/out, `StoreError::Db(e.to_string())`, free `row_to_x`/`collect_x` helpers, `TempDir` round-trip tests.

### Thin Tauri adapter (lock state, one call, map error)
**Source:** `src-tauri/src/commands/mods.rs:17-25` + `commands/mod.rs:24-42` (`boundary_err`, `require_game`)
**Apply to:** `commands/nexus.rs`, `commands/downloads.rs`. Reuse `require_game`/`boundary_err` unchanged.

### Frontend IPC bridge (typed invoke wrappers)
**Source:** `frontend/src/lib/api.ts` (interface-mirrors-DTO + `invoke` one-liners) + `+page.svelte:1-19,27-60` (Svelte 5 `$state`)
**Apply to:** new account/download api wrappers + the account panel / downloads list views.

### Injectable-core for testability
**Source:** `crates/loadorder/src/masterlist.rs:103-147` (generic-over-`F` testable core) and its `#[cfg(test)]` block
**Apply to:** `crates/nexus` HTTP paths — but with `mockito` (a real local server) per research, rather than a closure stub.

### Async/blocking discipline
**Source:** `crates/loadorder/src/masterlist.rs:168` uses `reqwest::blocking`; `commands/plugins.rs:235` offloads it via `spawn_blocking`.
**Apply to:** `crates/nexus` is async-only (`reqwest::Client`) — it MUST NOT call loadorder's blocking client in the tokio runtime (research Pitfall 6). The two clients never share a call path.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `crates/nexus/src/ratelimit.rs` | utility | transform | No rate-limiter exists; follow RESEARCH Pattern 6 (`governor` direct limiter + `X-RL-*` header backoff). |
| `src-tauri/src/keyring.rs` | service | — | No OS-Secret-Service code exists; follow RESEARCH Pattern 2 (hard-fail-no-plaintext, keyring 3.6). |
| `src-tauri/src/auth/` (OAuth orchestration + nxm:// capture) | service | request-response/event | No browser-launch / deep-link code exists; follow RESEARCH Pattern 1 (oauth2 5.0 PKCE) + Pattern 5 (deep-link + single-instance, register order). |

## Metadata

**Analog search scope:** `crates/{loadorder,extract,store,core}`, `src-tauri/src/{commands,lib.rs,state.rs}`, `frontend/src/{lib,routes}`
**Files scanned:** 14 analog files read in full or targeted ranges
**Pattern extraction date:** 2026-06-21
