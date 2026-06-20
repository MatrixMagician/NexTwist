# Phase 2: Multi-Mod Management - Research

**Researched:** 2026-06-20
**Domain:** Bethesda Creation Engine mod conflict resolution, plugin load-order management (LOOT), and per-game profiles — layered on the existing Phase-1 safe-deploy engine, headless Rust core + thin Tauri shell.
**Confidence:** HIGH (the flagged LOOT unknown is fully resolved with a verified, authoritative answer; conflict/profile/schema work is local design over a well-understood existing engine)

## Summary

The single largest flagged unknown — LOOT integration on Linux for an AppImage (PLUGIN-03) — is **resolved decisively in NexTwist's favour**. As of 2025, the LOOT author (Ortham) **rewrote `libloot` in pure Rust**. There is now an official, native `libloot` crate (v0.29.5, published by the LOOT project from `github.com/loot/libloot`) that needs **no C++ toolchain, no FFI shim, and no system libraries** — it is `cargo build` and statically links like any other Rust crate, which is ideal for a self-contained AppImage. It supersedes the old C++ libloot the original CONTEXT decision assumed. The crate's `Game` type exposes everything this phase needs: `sort_plugins` (PLUGIN-03), `set_load_order` / `load_order` (PLUGIN-02), `is_plugin_active` / `active_plugins_file_path` (PLUGIN-01), and `with_local_path` (so NexTwist supplies the Proton-prefix AppData path it already resolves). The asterisk-enabled `plugins.txt` format and masters-first ordering are handled **inside the library** — NexTwist must NOT hand-roll a `plugins.txt` writer.

The one cost: `libloot` (and its deps `libloadorder`, `esplugin`) are **GPL-3.0-or-later**. This is compatible with linking into NexTwist, but it makes the *distributed binary* effectively GPL-3.0 — a real constraint that must be recorded now and verified in the Phase-5 license audit (DIST-02). LOOT masterlists are separate per-game GitHub repos (`loot/skyrimse`, `loot/fallout4`), each a single `masterlist.yaml` licensed **CC0-1.0** (public domain — safe to bundle a snapshot, safe to fetch/cache).

Conflict resolution, the V2 schema, and profile switching are all **local design on top of the proven Phase-1 engine**, not external unknowns. The structural constraint `deployed_file UNIQUE(appid, target_rel)` means conflict resolution is a pure in-memory fold over enabled mods' staged trees (ordered by priority) that emits a **single-winner-per-path `StagedFiles`** set — which the existing `engine::deploy` consumes unchanged. Profile switching is `purge(old) → deploy(new winner set)` through the existing crash-safe journal/manifest, asserted pristine by the existing `testkit` harness.

**Primary recommendation:** Adopt the `libloot` Rust crate (0.29.5) as the single dependency for BOTH plugin management and sorting — it covers PLUGIN-01/02/03 with one library, no FFI, AppImage-clean. Build a headless `crates/loadorder` (plugins + LOOT) and a headless conflict resolver (in `crates/deploy` or new `crates/conflict`) that emit a deduplicated winner `StagedFiles`; add a V2 refinery migration for mods/profiles/plugin-state; route every disk change through the unchanged Phase-1 `deploy`/`purge`. Record the GPL-3.0 distribution implication immediately for the Phase-5 audit.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Conflict detection (file-level + per-mod) | Headless core (`crates/conflict` or `crates/deploy`) | — | Pure data fold over staged trees + priority; zero I/O, fully unit-testable headless (zero Tauri dep — established pattern) |
| Winner selection → `StagedFiles` | Headless core | `crates/deploy::engine` | Must satisfy `deployed_file UNIQUE(appid,target_rel)` BEFORE the existing safe `deploy` runs; deploy stays the only disk primitive |
| Mod priority / rank persistence | `crates/store` (V2 migration) | `crates/core` (type) | Priority is per-profile state; SQLite is the existing persistence tier |
| Plugin discovery (.esp/.esm/.esl scan) | Headless core (`crates/loadorder`) | `esplugin` via libloot | Scan enabled mods' staged trees + game `Data/`; classify via libloot/esplugin header flags |
| Plugin enable/disable + load order + plugins.txt write | `libloot` crate (in headless `crates/loadorder`) | `crates/steam` (prefix path) | libloot owns the asterisk format, masters-first, and the file write; steam supplies the Proton AppData path |
| LOOT auto-sort + masterlist | `libloot` crate | `reqwest` (masterlist fetch) | libloot `sort_plugins`; reqwest downloads/caches the per-game masterlist.yaml |
| Profile model + membership + switching | `crates/store` (V2) + headless reconcile | `crates/deploy` (purge/deploy) | Profile is a lightweight reference set; switch reconciles through the existing safe engine |
| Confirmation-gated disk mutation UI | Tauri shell + Svelte | headless commands | Thin adapters; all safety logic stays headless |

## Standard Stack

### Core (NEW for Phase 2)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `libloot` | 0.29.5 | LOOT sorting (PLUGIN-03) **and** plugin load-order / plugins.txt read+write / active-plugin state (PLUGIN-01/02) | `[VERIFIED: crates.io]` Official LOOT-project crate (`github.com/loot/libloot`), **pure Rust** (rewritten from C++ in 2025), no FFI/system deps — AppImage-clean. It is the exact engine the LOOT app itself uses. **License GPL-3.0-or-later** (see Audit). |
| `reqwest` | 0.13.x | Download/refresh the per-game LOOT masterlist.yaml | `[VERIFIED: project CLAUDE.md]` Already the sanctioned HTTP client; use `rustls-tls` (no OpenSSL) for AppImage portability. (Same crate Phase-3 will use; introducing it here is fine and avoids a second HTTP stack.) |

### Supporting (likely transitive via libloot; only add directly if needed)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `libloadorder` | 18.8.2 | Load-order + active-plugin (plugins.txt) primitives | `[VERIFIED: crates.io]` libloot depends on this. Use **only** if you need lower-level load-order control libloot doesn't surface. libloot's `Game` already exposes `load_order`/`set_load_order`/`is_plugin_active`, so a direct dep is probably unnecessary. GPL-3.0. |
| `esplugin` | 6.1.4 | Read .esp/.esm/.esl headers: `is_master_file()`, `is_light_plugin()` (ESL), `is_medium_plugin()`, `masters()` | `[VERIFIED: crates.io]` libloot depends on this. Use directly **only** for the plugin-list type badges (ESM/ESL/ESP) and master-detection if libloot's `Plugin` doesn't expose the flag you want. GPL-3.0. |
| `serde_yaml` *(or `serde_yml`)* | latest | Parse masterlist.yaml warnings (dirty plugins / missing masters) **only if** libloot's `Database` doesn't already surface them as structured data | `[ASSUMED]` Prefer libloot's `Database` API (it parses the masterlist for you); add a YAML crate only if you must read raw masterlist fields libloot doesn't expose. Verify before adding. |

### Already in the workspace — reuse, do not re-add

| Library | Role in Phase 2 |
|---------|-----------------|
| `rusqlite` (bundled) + `refinery` 0.9.2 | V2 migration + new query modules (mods, profiles, plugin-state) |
| `walkdir` 2.x | Plugin/conflict scan of staged trees + game `Data/` |
| `blake3` (via existing `backup`) | Reuse the existing content hash for winner `FileEntry.hash` |
| `serde` / `serde_json` | New core types serialization |
| `thiserror` (libs) / `anyhow` (app) | New `crates/loadorder/error.rs`, `crates/conflict/error.rs` |
| `tracing` | Structured logs for sort/switch/deploy |
| `crates/testkit` (DIR_SENTINEL pristine harness) | Profile-switch + conflict-redeploy regression tests |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `libloot` Rust crate (in-process) | Shell out to a LOOT CLI binary bundled in the AppImage | The CONTEXT-named fallback. **No longer needed** — the native Rust crate removes every reason to shell out (no C++ build, no process boundary, no bundling a second binary). Keep CLI fallback fully out of scope. |
| `libloot` for plugins.txt | Hand-rolled plugins.txt writer | **Anti-pattern — do not.** The asterisk format, masters-first hoisting, ESL/light handling, implicit-active base masters, and ghosted-plugin rules are subtle and game-specific; libloot/libloadorder encode them correctly and are tested. Hand-rolling is the #1 way plugins silently fail to load in-game. |
| `libloot` for sorting | Re-implement topological sort + masterlist | **Anti-pattern.** Sorting is a multi-graph topological sort over masterlist groups, requirements, load-after, overlap, and hardcoded edges. Re-implementing is a multi-month effort and will diverge from the community masterlist semantics. |
| New `crates/conflict` | Put conflict resolution inside `crates/deploy` | Either is fine. A dedicated crate keeps `deploy` focused on the disk primitive; co-locating avoids a crate. Recommend a **module in `crates/deploy`** (e.g. `deploy::conflict`) unless it grows large — the resolver's output type (`StagedFiles`) already lives in `deploy`. |

**Installation (`src-tauri/Cargo.toml` workspace deps — add to the relevant new crate's `Cargo.toml`):**
```toml
# crates/loadorder/Cargo.toml
libloot = "0.29"
# reqwest already configured at the workspace level with rustls-tls; reuse it.
# esplugin / libloadorder: add ONLY if libloot's API proves insufficient (verify first).
```

**Version verification performed (crates.io API, 2026-06-20):**
- `libloot` → `0.29.5`, license `GPL-3.0-or-later`, repo `github.com/loot/libloot.git`, first published 2025-08-02, MSRV **1.89**. `[VERIFIED: crates.io]`
- `libloadorder` → `18.8.2`, GPL-3.0, repo `github.com/Ortham/libloadorder.git`. `[VERIFIED: crates.io]`
- `esplugin` → `6.1.4`, GPL-3.0, repo `github.com/Ortham/esplugin.git`. `[VERIFIED: crates.io]`

## Package Legitimacy Audit

> Ran the legitimacy seam (`gsd-tools query package-legitimacy check --ecosystem crates`) + crates.io registry verification.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `libloot` | crates.io | ~10 mo (since 2025-08) | ~103/wk (3.3k total) | `github.com/loot/libloot` (official LOOT org) | SUS (low-downloads only) | **Approved — niche, authoritative.** Planner adds one `checkpoint:human-verify` before first install. |
| `libloadorder` | crates.io | since 2017 | ~595/wk | `github.com/Ortham/libloadorder` (LOOT author) | SUS (low-downloads only) | Approved if needed — same author as LOOT, 8+ yrs. |
| `esplugin` | crates.io | since 2017 | ~577/wk | `github.com/Ortham/esplugin` (LOOT author) | SUS (low-downloads only) | Approved if needed — same author, 8+ yrs. |

**Why SUS but approved:** All three are flagged **solely** on low weekly downloads, which reflects the niche (Bethesda modding tooling), not a supply-chain risk. They are authored/maintained by Ortham (the LOOT project lead), hosted under the official `loot`/`Ortham` GitHub orgs, are the exact libraries the LOOT desktop app ships, have no postinstall scripts, and are not deprecated/yanked. None are SLOP. The package names were confirmed against the **authoritative LOOT GitHub source**, not just registry existence.

**Packages removed due to SLOP verdict:** none
**Packages flagged SUS (planner inserts `checkpoint:human-verify` before first install):** `libloot` (and `libloadorder`/`esplugin` if added directly)

**LICENSE FLAG (carry to Phase 5 / DIST-02):** `libloot`, `libloadorder`, `esplugin` are **GPL-3.0-or-later**. Linking them makes the distributed NexTwist binary effectively GPL-3.0. This is internally consistent (it's a from-scratch app) but MUST be: (1) recorded in `cargo-deny` config as an allowed copyleft license for these crates, (2) reflected in NexTwist's own LICENSE choice, (3) verified by the Phase-5 license audit. Masterlist data (`masterlist.yaml`) is **CC0-1.0** — public domain, safe to bundle and to fetch.

## Architecture Patterns

### System Architecture Diagram

```
                         ┌─────────────────────────────────────────────┐
   Browser click /       │            Tauri Shell (thin)               │
   user action  ───────► │  commands/{conflicts,plugins,profiles}.rs   │  3–10 line adapters,
                         │  + Svelte views (conflict/plugins/profile)  │  zero safety logic
                         └───────────────┬─────────────────────────────┘
                                         │ (call headless)
            ┌────────────────────────────┼───────────────────────────────┐
            ▼                            ▼                                 ▼
   ┌─────────────────┐        ┌───────────────────────┐        ┌────────────────────┐
   │ Conflict resolve│        │  Load-order / Plugins  │        │   Profile reconcile│
   │ (headless)      │        │  (headless, crates/    │        │   (headless)       │
   │                 │        │   loadorder)           │        │                    │
   │ enabled mods +  │        │  scan staged+Data/ →   │        │ active profile →   │
   │ priority ranks  │        │  plugin list           │        │ enabled set+rank+  │
   │      │          │        │       │                │        │ plugin order       │
   │      ▼          │        │       ▼                │        │      │             │
   │ fold per path → │        │  libloot::Game         │        │  purge(old) then   │
   │ single winner   │        │  .sort_plugins()       │        │  deploy(new winner)│
   │ per target_rel  │        │  .set_load_order()     │        │      │             │
   └──────┬──────────┘        │  .save() → plugins.txt │        └──────┼─────────────┘
          │ winner StagedFiles│   (in Proton prefix    │               │
          │ (deduped)         │    AppData via         │               │
          ▼                   │    with_local_path)    │               ▼
   ┌──────────────────────────┴───────────────────────┴───────────────────────────┐
   │            crates/deploy::engine  (UNCHANGED Phase-1 safe primitive)          │
   │   deploy(store, game, &StagedFiles)  /  purge(...)  — journal → backup →      │
   │   method ladder (reflink→hardlink→symlink→copy) → manifest → 'done'           │
   └───────────────────────────────┬──────────────────────────────────────────────┘
                                    ▼
   ┌──────────────────────────────────────────────────────────────────────────────┐
   │ crates/store (rusqlite+refinery): V1 (managed_game, deployed_file UNIQUE,      │
   │ op_journal, vanilla_backup) + V2 (managed_mod, profile, profile_mod,           │
   │ plugin_state)        crates/steam: resolve prefix → AppData/Local/<Game>       │
   └──────────────────────────────────────────────────────────────────────────────┘
```

Primary use case (switch profile): UI confirm → profile-reconcile loads new profile's enabled-mod set + ranks → conflict-resolve emits winner `StagedFiles` → `purge(old)` then `deploy(new)` through the safe engine → load-order module writes plugins.txt for the new profile → testkit asserts pristine round-trip holds.

### Recommended Project Structure
```
crates/
├── core/src/model.rs       # EXTEND: add `priority`/`rank` to ManagedMod; add Profile, Plugin, PluginKind, Conflict types
├── store/src/
│   ├── migrations/V2__multi_mod.sql   # NEW: managed_mod, profile, profile_mod, plugin_state
│   ├── mods.rs             # NEW: managed_mod CRUD + priority
│   ├── profiles.rs         # NEW: profile + membership CRUD, active-profile flag
│   └── plugins.rs          # NEW: per-profile plugin enable/order state
├── deploy/src/
│   ├── engine.rs           # UNCHANGED public deploy/purge (the safe primitive)
│   └── conflict.rs         # NEW (recommended home): resolve winner StagedFiles set
├── loadorder/              # NEW headless crate (Tauri-free): plugin scan + libloot wrapper + masterlist fetch/cache
│   └── src/{lib.rs, scan.rs, loot.rs, masterlist.rs, error.rs}
└── testkit/                # reuse pristine harness for switch/redeploy tests
src-tauri/src/commands/
├── conflicts.rs            # NEW thin adapter
├── plugins.rs              # NEW thin adapter
└── profiles.rs             # NEW thin adapter
```

### Pattern 1: Conflict resolution → single-winner `StagedFiles` (CONF-01/02/03)
**What:** A pure fold over enabled mods (already ordered by priority/rank, top = highest = wins) producing one winner per `target_rel`, satisfying `deployed_file UNIQUE(appid, target_rel)` before deploy.
**When to use:** Before every Deploy and before every profile switch's deploy half.
**Example:**
```rust
// Headless, zero I/O beyond reading staged trees. Source pattern (NexTwist design over
// existing deploy::StagedFiles), informed by the Vortex "deployment rank" model.
// [CITED: Vortex/MO2 priority model — DeepWiki/modding.wiki]
pub struct ModInput { pub mod_id: i64, pub staging_root: PathBuf, pub rank: u32 } // lower rank = higher priority (1-based)
pub struct FileConflict { pub target_rel: PathBuf, pub providers: Vec<i64>, pub winner: i64 }

pub fn resolve(mods: &[ModInput]) -> (StagedFiles, Vec<FileConflict>) {
    // Walk each enabled mod's staged tree → map target_rel -> Vec<(rank, mod_id)>.
    // Sort providers by rank asc; winner = providers[0]. Emit winner's staging_root+rel.
    // Result.files has exactly one entry per target_rel (deduped) → UNIQUE-safe.
    // FileConflict produced only where providers.len() > 1 (drives the CONF-01 table).
    // NOTE: keep deploy order = libloadorder/engine expectations (deploy order is by path; engine already iterates staged.files).
    todo!()
}
```
Winner `FileEntry.hash` reuses the existing `backup::blake3_file`; the manifest then records exactly the winning (mod, file, hash) so purge stays pristine (CONTEXT: "winning (mod,file,hash) recorded in the per-game manifest").

### Pattern 2: Plugins + LOOT via libloot (PLUGIN-01/02/03) — ONE library
**What:** Construct a `libloot::Game` with the **resolved Proton-prefix AppData path** (NexTwist supplies it; libloot cannot derive it on Linux — see Pitfall 1). Load plugins, sort, set order, persist plugins.txt.
**When to use:** Plugin scan/display, enable/disable, reorder, "Sort with LOOT", and apply-on-deploy.
**Example:**
```rust
// [VERIFIED: github.com/loot/libloot src/game.rs public API @ master, 2026-06-20]
use libloot::{Game, GameType};

// game_path = resolved install dir (.../steamapps/common/Skyrim Special Edition)
// local_path = <prefix>/drive_c/users/steamuser/AppData/Local/<GameName>  (NexTwist resolves)
let mut g = Game::with_local_path(GameType::SkyrimSE, &install_dir, &proton_appdata_local)?;

g.load_current_load_order_state()?;            // read existing plugins.txt / order
let installed: Vec<&Path> = /* enabled mods' staged plugins + game Data/ plugins */;
g.load_plugins(&installed)?;                    // parse headers (master/ESL flags handled)

// PLUGIN-03: propose a sorted order (DOES NOT write — user reviews, then applies)
let sorted: Vec<String> = g.sort_plugins(&plugin_names)?;

// PLUGIN-02: apply the user-approved order (masters-first enforced INTERNALLY)
g.set_load_order(&sorted.iter().map(String::as_str).collect::<Vec<_>>())?;

// PLUGIN-01: query/set active state
let active = g.is_plugin_active("Foo.esp");
// active_plugins_file_path() returns the exact plugins.txt path libloot will write.
```
- `GameType::SkyrimSE` and `GameType::Fallout4` both map to **Asterisk-based plugins.txt** (`*Enabled.esp`) — libloot/libloadorder write the `*`-enabled format automatically. `[VERIFIED: libloadorder src/game_settings.rs load_order_method()]`
- Masters (`.esm` + ESL/light-flagged) are hoisted before regular `.esp` **inside the library** (`add`/`set_load_order` reject orderings that violate masters-first). The UI's masters-first invariant (UI-SPEC §B.2) is enforced by libloot, not NexTwist. `[VERIFIED: libloadorder writable.rs add()]`
- Type badges (ESM/ESL/ESP, UI-SPEC §B.1): use libloot `Plugin` flags, or `esplugin::Plugin::{is_master_file, is_light_plugin}` directly. `[VERIFIED: esplugin src/plugin.rs]`

### Pattern 3: Masterlist fetch + cache (PLUGIN-03)
**What:** Per-game masterlist from `github.com/loot/<game>` (`loot/skyrimse`, `loot/fallout4`), file `masterlist.yaml`, branch **matching libloot's major** (e.g. `v0.29` for libloot 0.29.x). Load into libloot's `Database` before sorting.
**When to use:** On "Sort with LOOT" if cache stale/absent; manual refresh action; bundled snapshot as offline fallback.
**Example:**
```rust
// Raw URL: https://raw.githubusercontent.com/loot/skyrimse/v0.29/masterlist.yaml  (CC0-1.0)
// Cache at <app_data>/masterlists/<appid>/masterlist.yaml; refresh via reqwest+rustls.
// Then: game.database().write().unwrap().load_masterlist(&cached_path)?  (see libloot Database API)
```
- Masterlist license **CC0-1.0** — public domain. Bundling a snapshot in the AppImage is legally safe. `[VERIFIED: GitHub API loot/skyrimse license spdx_id]`
- **Branch = libloot major version.** libloot 0.29.x ⇒ `v0.29` branch (confirmed present in `loot/skyrimse` branches). Pin the branch to your libloot version, not `master`.

### Pattern 4: Profile switch through the safe engine (PROF-01/02/03)
**What:** A profile is a lightweight reference set (enabled mod ids + ranks + plugin order) over a single shared staging store. Switching = `purge(old)` then `deploy(new winner StagedFiles)`, then write the new profile's plugins.txt.
**Example:**
```rust
// All behind one user confirmation (UI-SPEC §D.2). Crash-safe via existing journal.
purge(&store, &game)?;                       // restore pristine (existing primitive)
let (winner, _conflicts) = conflict::resolve(&new_profile_enabled_mods);
deploy(&store, &game, &winner)?;             // deploy new set (existing primitive)
loadorder::apply(&game, &new_profile_plugin_order)?;  // libloot save() to prefix
// testkit::assert_round_trip_pristine after a full purge to prove reversibility survives.
```

### Anti-Patterns to Avoid
- **Hand-rolling `plugins.txt`** (asterisk format, masters-first, ESL hoisting, ghosted/implicit-active rules): use libloot. The Creation Engine silently ignores malformed load order.
- **Re-implementing LOOT sorting**: use `libloot::Game::sort_plugins`.
- **Letting libloot derive the AppData path on Linux**: it can't (returns `NoLocalAppData`); always use `with_local_path`.
- **Bypassing `engine::deploy`** with direct file ops for conflicts/profiles: every disk change must go through the journaled primitive or the pristine guarantee breaks.
- **Two owners for one path**: `deployed_file UNIQUE(appid,target_rel)` will reject it — resolve to a single winner first.
- **Per-profile staged copies**: CONTEXT rejects this; profiles share one staging store.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| plugins.txt read/write | Custom asterisk-format serializer | `libloot` (`set_load_order`/`save`) | Asterisk + masters-first + ESL + implicit-active + ghosting rules are subtle & game-specific |
| LOOT auto-sort | Topological sort over masterlist | `libloot::Game::sort_plugins` | Multi-graph topo sort with groups/requirements/overlap/hardcoded edges |
| ESL/ESM/master detection | Parse TES4 header bytes by hand | `esplugin` (`is_master_file`/`is_light_plugin`) or libloot `Plugin` | Header flag semantics differ per game/format version |
| AppData/Local plugins.txt path per game | Hardcode `<GameName>` folder | libloot `appdata_folder_name` logic via `with_local_path` (you supply prefix root) | Folder name varies: Steam vs GOG vs Enderal vs MS Store |
| Crash-safe deploy/purge | New transactional file ops | Existing `crates/deploy::engine` | Already journaled, backed-up, method-laddered, pristine-tested |
| Pristine round-trip assertion | New verification | `crates/testkit` DIR_SENTINEL harness | Already catches orphan empty dirs (GAP-01) |

**Key insight:** Phase 2's hardest-looking problems (LOOT, plugins.txt, ESL flags) are entirely solved by **one** pure-Rust, AppImage-clean library family authored by the LOOT project. The genuinely new NexTwist code is the *local* glue: conflict fold, V2 schema, profile reconcile — all on top of the unchanged Phase-1 safe engine.

## Runtime State Inventory

> Not a rename/refactor phase, but Phase 2 introduces NEW runtime state and a DB migration over EXISTING Phase-1 state. Recorded so the planner handles migration explicitly.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data (existing) | Phase-1 SQLite has `managed_game`, `deployed_file`, `op_journal`, `vanilla_backup` only — **no mod/profile/plugin tables**. A Phase-1 install may have a single deployed mod with no `managed_mod` row concept. | **V2 refinery migration** (additive). Data migration: auto-create a "Default" profile per managed game and fold existing single-mod/deployed state into it (CONTEXT decision). |
| Stored data (new) | `managed_mod` (with priority/rank), `profile`, `profile_mod` (membership + per-profile rank), `plugin_state` (per-profile enable + order). | New V2 tables + query modules. |
| Live service config | `plugins.txt` lives in the **Proton prefix** (`<prefix>/drive_c/users/steamuser/AppData/Local/<GameName>/Plugins.txt`) — NOT in git, NOT in the game install dir, NOT in NexTwist's DB. It is in-game-observable runtime state. | Write via libloot on plugin/profile apply; re-resolve prefix each session (consistent with Phase-1). Crash mid-write: plugins.txt is regenerable from DB plugin_state, so treat it as derived output, not source of truth. |
| OS-registered state | None — no Task Scheduler / systemd / MIME handlers in this phase (nxm:// is Phase 3). | None. |
| Secrets/env vars | None new (NexusMods auth is Phase 3). `$STEAM_COMPAT_DATA_PATH` already honored by Phase-1 prefix resolution — reuse. | None. |
| Build artifacts | New `crates/loadorder` + libloot dep increases build deps; libloot **MSRV 1.89** > workspace `rust-version=1.85`. | See Environment Availability — bump workspace MSRV/CI to ≥1.89. |

## Common Pitfalls

### Pitfall 1: libloot/libloadorder cannot derive the AppData path on Linux
**What goes wrong:** `Game::new(GameType::SkyrimSE, &install)` (and libloadorder `GameSettings::new`) call `local_path()` which, on non-Windows, returns `Err(NoLocalAppData)` for any game needing an AppData folder (SkyrimSE, Fallout4). Plugins.txt operations then fail.
**Why it happens:** On Windows the lib reads `%LOCALAPPDATA%`; on Linux there is no such concept — the real location is inside the Proton prefix, which only NexTwist knows.
**How to avoid:** ALWAYS use `Game::with_local_path(game_type, &install_dir, &proton_appdata_local)` where `proton_appdata_local = <prefix>/drive_c/users/steamuser/AppData/Local/<GameName>`. NexTwist already resolves `<prefix>` (steam crate); build the AppData subpath from it. The `<GameName>` folder is `Skyrim Special Edition` / `Fallout4` for Steam installs (libloadorder's `skyrim_se_appdata_folder_name`/`fallout4_appdata_folder_name`). `[VERIFIED: libloadorder src/game_settings.rs local_path() #[cfg(not(windows))]]`
**Warning signs:** `NoLocalAppData` error; plugins.txt written to a wrong/empty path; plugins not loading in-game.

### Pitfall 2: AppData folder name is not always `<GameName>` — and the prefix folder may not exist yet
**What goes wrong:** Hardcoding `Fallout4` breaks GOG/Enderal/MS-Store; and a freshly-installed game that has never been launched has no `AppData/Local/<Game>/Plugins.txt` yet.
**How to avoid:** Pass the prefix root and let libloot's folder-name logic apply where possible; for the path you construct, create parent dirs before write (libloadorder has `create_parent_dirs`). If the game was never run, plugins.txt may be absent — `load_current_load_order_state` tolerates absence; on save it creates the file. Surface UI-SPEC error "Couldn't write plugins.txt … Check the game's Proton prefix is resolved" when the prefix dir is missing (reuse Phase-1 `prefix_exists`).
**Warning signs:** Writes succeed but in-game order unchanged (wrong folder); ENOENT on save (missing parent).

### Pitfall 3: Conflict winner set must be deduped BEFORE deploy, and order matters
**What goes wrong:** Emitting two providers for the same `target_rel` hits `deployed_file UNIQUE(appid,target_rel)` and the deploy aborts mid-way (then crash-recovery kicks in unnecessarily).
**How to avoid:** Resolver emits exactly one winner per path. Unit-test that `resolve()` output has no duplicate `target_rel`. Keep the existing engine's per-path iteration; do not assume deploy order equals plugin load order (plugin order is a *separate* concern handled by libloot, not by file deploy order).
**Warning signs:** `UNIQUE constraint failed: deployed_file.appid, deployed_file.target_rel`.

### Pitfall 4: Profile switch must purge-to-pristine between profiles, or stale files leak
**What goes wrong:** Deploying profile B's winner set over profile A's deployment without purging leaves A-only files on disk (B doesn't own them, so B's purge later won't remove them) — breaking reversibility and conflict correctness.
**How to avoid:** Switch = full `purge(old)` (restores pristine) THEN `deploy(new)`. The existing purge is manifest-driven and crash-safe; the testkit pristine assertion between phases proves no leak. This is the CONTEXT decision; do not optimize to a diff-deploy in v1.
**Warning signs:** Files from a previous profile remain after switching; purge of the new profile leaves orphans.

### Pitfall 5: Masterlist branch ≠ `master`; it tracks the libloot major
**What goes wrong:** Fetching `master` branch masterlist against an older/newer libloot can produce schema mismatches LOOT can't parse.
**How to avoid:** Pin the masterlist branch to your libloot major (libloot 0.29.x → `v0.29`). Bump both together. `[VERIFIED: GitHub API loot/skyrimse branches include v0.29]`
**Warning signs:** Masterlist parse errors after a libloot upgrade.

### Pitfall 6: GPL-3.0 contamination surfaces only at distribution (Phase 5)
**What goes wrong:** libloot family is GPL-3.0; if Phase 5's `cargo-deny` is configured to deny copyleft (as the project leans toward for the UnRAR concern), the build fails late.
**How to avoid:** Now: add an explicit `cargo-deny` allowance for GPL-3.0 on `libloot`/`libloadorder`/`esplugin`, and choose NexTwist's own license compatibly. Flag for DIST-02 audit.

## Code Examples

### Detect plugin type for UI badge (ESM/ESL/ESP)
```rust
// [VERIFIED: esplugin src/plugin.rs @ master]
use esplugin::{GameId, Plugin, ParseOptions};
let mut p = Plugin::new(GameId::SkyrimSE, path);
p.parse_file(ParseOptions::header_only())?;
let kind = if p.is_light_plugin() { "ESL" }          // ESL / light-flagged
           else if p.is_master_file() { "ESM" }      // master (.esm or master-flagged)
           else { "ESP" };
// Masters-first grouping (UI-SPEC §B.2): ESM + ESL render in the pinned "Masters" group.
```

### Propose-then-apply LOOT sort (no silent apply, UI-SPEC §C)
```rust
// [VERIFIED: libloot src/game.rs sort_plugins/set_load_order]
let proposed = game.sort_plugins(&current_names)?;   // returns proposed order; writes nothing
// UI shows diff vs current; on "Apply sorted order":
game.set_load_order(&proposed.iter().map(String::as_str).collect::<Vec<_>>())?;
// order remains hand-editable afterward; persist per-profile plugin_state to SQLite.
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| libloot is a **C++** library needing FFI + C++ toolchain to use from Rust | libloot is **pure Rust** (`cargo add libloot`) | 2025 (port complete; crate first published 2025-08-02) | Removes the entire FFI/C++/AppImage-static-link risk the CONTEXT flagged. No CLI-shell fallback needed. `[CITED: blog.ortham.net/posts/2025-04-24-porting-libloot-to-rust]` |
| Shell out to a bundled LOOT CLI binary | In-process `libloot` crate | 2025 | One process, one dep, statically linked; cleaner AppImage. |
| Hand-derive plugins.txt path / format | libloot/libloadorder own it (Asterisk method, masters-first) | stable | Don't hand-roll. |

**Deprecated/outdated:**
- The CONTEXT assumption "FFI binding over the C++ library, vs CLI fallback" — both are obsolete; use the native crate.
- `sevenz-rust` (original) noted project-wide as abandoned — not relevant to Phase 2 but reaffirmed.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | libloot's `Game` API alone covers plugins.txt read/write AND active state (no direct `libloadorder` dep needed) | Standard Stack / Pattern 2 | LOW — `load_order`/`set_load_order`/`is_plugin_active`/`active_plugins_file_path` are verified public on `Game`; if a gap appears, add `libloadorder` (already mapped). Planner should spike a minimal `with_local_path → load → sort → save` round-trip early. |
| A2 | libloot's `Database` surfaces dirty-plugin / missing-master warnings as structured data (so no raw YAML parsing) | Supporting libs | LOW–MED — if not, add `serde_yaml` to read masterlist warnings. Verify against libloot `Database` docs at plan time. |
| A3 | Steam SkyrimSE AppData folder is `Skyrim Special Edition` and Fallout4 is `Fallout4` inside the Proton prefix | Pitfall 1/2 | MED — verified from libloadorder source for non-GOG/non-Enderal Steam installs; GOG/Enderal differ. Confirm on-hardware (UAT) that the resolved path matches the real prefix folder. |
| A4 | Masterlist branch convention is `v<libloot-major>` (e.g. `v0.29`) | Pattern 3 | LOW — `v0.29` confirmed present in `loot/skyrimse` branches; pin per libloot version. |
| A5 | A dedicated `serde_yaml`/`serde_yml` choice if YAML is needed | Supporting | LOW — only if A2 is wrong; pick the maintained fork at that time. |

**Note for discuss/planner:** A1 and A3 are the two to de-risk first with a tiny libloot spike against a real Proton prefix (or a fixture mimicking it). Everything else is design over the proven Phase-1 engine.

## Open Questions

1. **Does libloot's `Database` expose dirty-plugin / missing-master warnings directly, or must we read masterlist.yaml?** (A2)
   - What we know: libloot parses the masterlist for sorting; warnings are a core LOOT feature.
   - What's unclear: exact Rust API to enumerate warnings for the v1 "critical warnings" list (UI-SPEC §C.3).
   - Recommendation: check `libloot::Database` methods at plan time; fall back to `serde_yaml` only if absent.

2. **Where should the conflict resolver live — `crates/deploy::conflict` module or a new `crates/conflict`?**
   - Recommendation: module in `crates/deploy` (its output `StagedFiles` already lives there); promote to a crate only if it grows.

3. **plugins.txt crash-safety:** the existing journal covers `deployed_file` ops, not the prefix plugins.txt write.
   - Recommendation: treat plugins.txt as **derived** from `plugin_state` (DB is source of truth); on launch/recovery, regenerate plugins.txt from DB rather than journaling the file write. Confirm this is acceptable (it is, since plugins.txt is regenerable and not part of the game-pristine invariant).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | Build (libloot MSRV **1.89**) | ⚠ verify | workspace pins `channel="stable"` (resolves to installed) + `rust-version=1.85` | Bump workspace `rust-version` to `1.89` and CI pin ≥1.89; `stable` channel already gets latest, so local dev is fine if stable ≥1.89 |
| Internet (masterlist fetch) | PLUGIN-03 refresh | runtime-only | — | Bundled masterlist snapshot (CC0) — offline fallback per CONTEXT |
| `reqwest` + rustls | masterlist download | ✓ (sanctioned) | 0.13.x | — |
| A real Proton prefix w/ AppData | plugins.txt write (in-game) | hardware-only | — | Fixture prefix dir for headless tests; **real in-game load is manual UAT** |

**Missing dependencies with no fallback:** none blocking.
**Action required:** Workspace MSRV bump from 1.85 → **1.89** for libloot. This is a small, explicit change the planner must include as an early task (it touches `rust-toolchain`/`Cargo.toml rust-version` and CI). It does not conflict with the EXDEV (1.85) requirement — 1.89 ⊇ 1.85.

## Validation Architecture

> nyquist_validation: config.json not asserting false → treated as ENABLED.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `crates/testkit` round-trip-pristine harness (DIR_SENTINEL) |
| Config file | none (cargo workspace); `crates/testkit` provides shared fixtures |
| Quick run command | `cargo test -p conflict -p loadorder -p store` (per-crate, <30s) |
| Full suite command | `cargo test --workspace` (Phase-1 baseline: 82 tests on tmpfs + btrfs) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CONF-01 | File-level conflicts enumerated across N enabled mods | unit | `cargo test -p deploy conflict::detects_file_level` | ❌ Wave 0 |
| CONF-02 | Priority/rank determines winner | unit | `cargo test -p deploy conflict::higher_priority_wins` | ❌ Wave 0 |
| CONF-03 | Deploy applies winner set deterministically; one owner/path | integration | `cargo test -p deploy conflict::winner_set_deploys_unique` | ❌ Wave 0 |
| PLUGIN-01 | Enable/disable plugin reflected in active state | unit | `cargo test -p loadorder plugins::toggle_active` | ❌ Wave 0 |
| PLUGIN-02 | Load order written as asterisk plugins.txt at prefix path; masters-first | integration | `cargo test -p loadorder plugins::writes_asterisk_masters_first` | ❌ Wave 0 |
| PLUGIN-03 | LOOT sort proposes valid order from masterlist | integration | `cargo test -p loadorder loot::sort_proposes_order` | ❌ Wave 0 |
| PROF-01 | Create multiple profiles per game | unit | `cargo test -p store profiles::create_multiple` | ❌ Wave 0 |
| PROF-02 | Switch profile reconciles deploy through safe engine | integration | `cargo test -p deploy profile::switch_round_trip_pristine` (testkit) | ❌ Wave 0 |
| PROF-03 | Each profile preserves its own enabled set + order | unit | `cargo test -p store profiles::preserve_membership` | ❌ Wave 0 |
| (migration) | V2 migration + Phase-1 state → Default profile | integration | `cargo test -p store migrations::v2_migrates_phase1_state` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p <touched crate>` + `cargo clippy -- -D warnings`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** `cargo test --workspace` green (≥82 prior + new) + `cargo deny check` (GPL-3.0 allowance present) before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/store/src/migrations/V2__multi_mod.sql` + migration test fixture (Phase-1 DB → V2)
- [ ] `crates/deploy/src/conflict.rs` test module
- [ ] `crates/loadorder/` crate scaffold + tests (libloot spike covering A1/A3 first)
- [ ] testkit helper: a fixture Proton-prefix AppData dir for headless plugins.txt write tests
- [ ] `cargo-deny` config update to allow GPL-3.0 for libloot family

## Security Domain

> security_enforcement not disabled in config → included. Phase 2 is local-only (no network auth, no untrusted input beyond mod file paths already validated in Phase-1 extract).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | No auth in this phase (Phase 3) |
| V3 Session Management | no | — |
| V4 Access Control | no | Single-user desktop |
| V5 Input Validation | yes | Masterlist YAML is parsed by libloot (trusted CC0 source over TLS); plugin paths come from already-validated staged trees (Phase-1 zip-slip/symlink validation upstream); enforce paths stay within staging roots in the resolver |
| V6 Cryptography | yes (reuse) | Reuse existing blake3 hashing; reqwest **rustls-tls** for masterlist fetch (no OpenSSL) |
| V12 Files/Resources | yes | plugins.txt write confined to the resolved prefix; deploy confined to `Data/` via existing `guard_within_root` |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malicious masterlist over plain HTTP / MITM | Tampering | Fetch over HTTPS (rustls) from `raw.githubusercontent.com`; pin repo+branch; bundled snapshot fallback |
| Path traversal via crafted plugin/target path in conflict resolver | Tampering/EoP | Reuse Phase-1 `guard_within_root`; assert winner paths resolve inside the mod's staging root and the game `Data/` root |
| Deploying two owners → UNIQUE violation aborts mid-op | DoS (self) | Dedup winner set before deploy; engine's crash-recovery already recovers a partial op |
| Profile switch interrupted → non-pristine game | Tampering (integrity) | purge→deploy through existing journal; testkit pristine assertion regression-locks it |

## Sources

### Primary (HIGH confidence)
- crates.io API (`api/v1/crates/{libloot,libloadorder,esplugin,reqwest}`) — verified versions/licenses/repos: libloot 0.29.5 GPL-3.0-or-later (2025-08-02), libloadorder 18.8.2 GPL-3.0, esplugin 6.1.4 GPL-3.0. `[VERIFIED]`
- `github.com/loot/libloot` `src/{lib.rs,game.rs}` @ master — public API: `Game::with_local_path`, `sort_plugins`, `load_order`/`set_load_order`, `is_plugin_active`, `active_plugins_file_path`, `GameType`. `[VERIFIED]`
- `github.com/Ortham/libloadorder` `src/{enums.rs,game_settings.rs,load_order/*.rs}` @ master — `GameId` variants, `LoadOrderMethod::Asterisk` for SkyrimSE/Fallout4, non-Windows `local_path()` returns `NoLocalAppData`, AppData folder-name logic, `WritableLoadOrder` trait. `[VERIFIED]`
- `github.com/Ortham/esplugin` `src/plugin.rs` @ master — `is_master_file`/`is_light_plugin`/`is_medium_plugin`/`masters`. `[VERIFIED]`
- GitHub API `orgs/loot/repos` + `repos/loot/skyrimse/{branches,contents}` — per-game masterlist repos, `masterlist.yaml`, CC0-1.0, `v0.29` branch. `[VERIFIED]`
- NexTwist source (`crates/core/model.rs`, `crates/store/migrations/V1__init.sql`, `crates/deploy/engine.rs`, `crates/store/{manifest,registry}.rs`, `crates/steam/resolve.rs`, `rust-toolchain`) — existing integration surfaces. `[VERIFIED: codebase]`

### Secondary (MEDIUM confidence)
- `blog.ortham.net/posts/2025-04-24-porting-libloot-to-rust` — libloot C++→Rust port narrative. `[CITED]`
- LOOT sorting algorithm overview (`docs/api/sorting.rst`, loot.github.io) — multi-graph topo sort inputs. `[CITED]`
- Vortex/MO2 deployment-rank conflict model (project CLAUDE.md sources: DeepWiki, modding.wiki) — priority-list winner convention. `[CITED]`

### Tertiary (LOW confidence)
- Exact `libloot::Database` warning-enumeration API (A2) — inferred; verify at plan time.

## Metadata

**Confidence breakdown:**
- Standard stack / LOOT integration: HIGH — verified against official LOOT-project source + crates.io; the flagged unknown is fully resolved.
- Plugins.txt format/path under Proton: HIGH for format/method (verified source), MED for exact AppData folder name on a real prefix (needs on-hardware UAT, A3).
- Conflict resolution / V2 schema / profiles: HIGH — local design over the verified, proven Phase-1 engine and an explicit `UNIQUE` constraint.
- Masterlist warnings API (A2): LOW–MED — single open question, with a clear fallback.

**Research date:** 2026-06-20
**Valid until:** 2026-07-20 (libloot is on a ~monthly 0.29.x cadence; re-verify the crate version + matching masterlist branch at plan time if more than ~30 days elapse).
