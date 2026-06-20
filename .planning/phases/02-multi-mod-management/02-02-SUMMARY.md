---
phase: 02-multi-mod-management
plan: 02
subsystem: load-order / plugins (libloot Linux seam)
tags: [libloot, plugins, load-order, proton-prefix, with_local_path, spike, gpl, cargo-deny]
status: complete
requires:
  - "Phase-1 core model + StoreError re-export (nextwist_core::StoreError)"
  - "Phase-1 steam crate prefix-resolution conventions (SKYRIM_SE/FALLOUT4 appids, prefix layout)"
  - "Plan 01 MSRV 1.89 + cargo-deny GPL allowance scaffold"
  - "testkit fake_game_tree / snapshot harness conventions"
provides:
  - "crates/loadorder (nextwist-loadorder): Tauri-free libloot wrapper crate"
  - "loot::open_game — ALWAYS Game::with_local_path (the Linux seam, A1/A3 de-risked)"
  - "loot::appdata_local_path / game_type_for / set_order_and_save"
  - "LoadOrderError thiserror enum (Store/Io/Loot/NoLocalAppData)"
  - "testkit::fake_proton_prefix headless Proton-prefix AppData fixture builder"
  - "VERIFIED libloot 0.29.5 API surface (recorded below — Plan 04 builds on it)"
affects:
  - "Plan 04 (plugin/LOOT manager) — consumes the verified libloot wrapper + API findings"
  - "Plan 05 (profile switch) — loadorder::apply (libloot save) per profile builds on this seam"
  - "Phase-5 DIST-02 — GPL-3.0 (+ -or-later) license carry-forward confirmed against installed crates"
tech-stack:
  added:
    - "libloot 0.29.5 (GPL-3.0-or-later) — the LOOT project's pure-Rust crate"
    - "transitive: libloadorder 18.8.2 (GPL-3.0), esplugin 6.1.4 (GPL-3.0), loot-condition-interpreter 6.0.0 (MIT)"
  patterns:
    - "Headless crate analog of crates/deploy (nextwist_core alias to avoid ::core shadowing thiserror)"
    - "libloot error types flattened to LoadOrderError::Loot(String) at the crate boundary"
    - "with_local_path ALWAYS (never Game::new) — the Linux Proton-prefix seam (Pitfall 1)"
    - "Minimal 24-byte TES4 header fixtures for header-only esplugin parsing in tests"
key-files:
  created:
    - crates/loadorder/Cargo.toml
    - crates/loadorder/src/lib.rs
    - crates/loadorder/src/error.rs
    - crates/loadorder/src/loot.rs
    - crates/loadorder/tests/libloot_spike.rs
  modified:
    - Cargo.toml
    - Cargo.lock
    - deny.toml
    - crates/testkit/src/lib.rs
decisions:
  - "libloot 0.29.5's public Game API exposes load-order (set_load_order) + active-state QUERY (is_plugin_active) but NO active-state SETTER; plugin active state enters via the Plugins.txt libloot loads (NexTwist generates it from DB plugin_state in Plan 04)."
  - "set_load_order persists internally (calls load_order.save()); there is NO separate Game::save in 0.29.5, so set_order_and_save = load_current_load_order_state + set_load_order."
  - "with_local_path's local_path IS the AppData/Local/<GameName> folder itself; libloot does not re-append the game folder name when given an explicit local path. active_plugins_file = <local_path>/Plugins.txt for SkyrimSE."
  - "deny.toml gains a bare GPL-3.0 allowance: libloadorder/esplugin publish GPL-3.0 (only), not GPL-3.0-or-later as Plan 01 assumed; GPL-3.0 is compatible to ship in a GPL-3.0-or-later AppImage."
metrics:
  duration_min: 22
  tasks: 3
  files: 9
  tests_added: 8
  completed: 2026-06-21
---

# Phase 2 Plan 02: libloot Linux Seam (A1/A3 De-risk) Summary

Stood up the headless `crates/loadorder` crate, wired `libloot 0.29.5` behind the approved package-legitimacy checkpoint, and **proved the single largest Phase-2 unknown**: libloot's `Game::with_local_path → load → set_load_order` round-trip works on Linux against a fixture Proton-prefix AppData dir with NO `NoLocalAppData`, writing an asterisk-format `Plugins.txt` at libloot's reported path bounded inside the prefix. Plan 04's plugin manager is now mechanical on top of this verified wrapper.

## What Was Built

- **`crates/loadorder` (`nextwist-loadorder`)** — Tauri-free workspace member, analog of `crates/deploy`. Aliases `nextwist_core` (not `core`) to avoid shadowing `::core` for `thiserror`.
- **`LoadOrderError`** — `thiserror` enum mirroring `DeployError`: `Store(#[from] StoreError)`, structured `Io { path, source }` + `io()` ctor, `Loot(String)`, `NoLocalAppData(PathBuf)`. No anyhow.
- **`loot.rs` wrapper** — `appdata_local_path`, `game_type_for`, `open_game` (ALWAYS `with_local_path`), `set_order_and_save`.
- **`testkit::fake_proton_prefix`** — builds `<root>/drive_c/users/steamuser/AppData/Local/<game>` (+ optional seeded `Plugins.txt`) for headless plugins.txt tests.
- **`libloot_spike.rs`** — 4-case A1/A3 spike (the primary deliverable).

## VERIFIED libloot 0.29.5 API Surface (Plan 04 depends on this)

Verified against the installed crate source (`~/.cargo/registry/.../libloot-0.29.5`), not assumptions:

```rust
// game.rs — all the methods the wrapper uses:
Game::with_local_path(GameType, game_path: &Path, game_local_path: &Path)
    -> Result<Game, GameHandleCreationError>;   // game_path MUST be an existing dir
Game::load_current_load_order_state(&mut self) -> Result<(), LoadOrderStateError>; // tolerates absent file
Game::set_load_order(&mut self, &[&str]) -> Result<(), LoadOrderError>;  // SETS + PERSISTS (save() internal)
Game::active_plugins_file_path(&self) -> &PathBuf;   // <local_path>/Plugins.txt for SkyrimSE
Game::is_plugin_active(&self, &str) -> bool;
Game::sort_plugins(&self, &[&str]) -> Result<Vec<String>, SortPluginsError>; // (unused this plan; Plan 04)
Game::load_plugins(&mut self, &[&Path]) -> Result<(), LoadPluginsError>;      // (unused this plan; Plan 04)
GameType::{SkyrimSE, Fallout4}   // also Fallout4VR, Skyrim, Oblivion, Morrowind, Starfield, ...
```

**Deltas from the plan's research-derived assumptions (adapted per Deviation Rule):**

1. **No `Game::save()`.** The plan assumed a separate save step ("then persist (libloot's save/equivalent)"). In 0.29.5 `set_load_order` calls `self.load_order.save()` internally (`game.rs:545`). So `set_order_and_save` = `load_current_load_order_state` + `set_load_order` — no third call. Plan 04 must NOT look for `Game::save`.
2. **No active-plugin setter.** The plan's `is_plugin_active` is query-only; there is no `set_active`/`activate` on `Game` in 0.29.5. A plugin's active flag enters through the `Plugins.txt` libloot reads (`load_current_load_order_state`), and `set_load_order` preserves the active state of already-loaded plugins. NexTwist will generate that file from DB `plugin_state` in Plan 04. The spike seeds `*MyMod.esp` to exercise the asterisk path.
3. **`with_local_path` requires `game_path` to be an existing directory** (`game.rs:214` `is_dir()` check) — the spike creates a real fixture install `Data/` dir. The `local_path` may be absent (it is created on save / by `open_game`).
4. **`local_path` is the AppData folder itself** — libloot does not re-append the `<GameName>` folder when an explicit local path is given (its `appdata_folder_name` logic is only used by the `Game::new` path we bypass). So `appdata_local_path` returns the full `.../AppData/Local/<GameName>` and that whole path is the `local_path` argument.

## Spike Limitation (recorded for Plan 04)

libloot/libloadorder **open and header-parse every plugin named in a load order** (`libloadorder::Plugin::with_path` → `esplugin::parse_reader(header_only())`). A named plugin that does not physically exist in the game `Data/` dir errors out. The spike therefore writes minimal but VALID **24-byte TES4 header** files:

```
[0..4)  b"TES4"   [4..8) size_of_subrecords=0   [8..12) flags (0x1=master)
[12..16) form_id=0   [16..24) version-control/unknown (ignored by header parser)
```

`Skyrim.esm` is hardcoded implicitly-active for SkyrimSE and must exist. Plan 04's real plugin scan will operate on real game/mod ESM/ESP files, so this is a test-fixture concern only — but Plan 04's libloot calls must assume every plugin in an order exists on disk.

## Verified `appdata_local_path` Shape (Plan 04 reuses verbatim)

```
appdata_local_path(prefix, "Skyrim Special Edition")
  == <prefix>/drive_c/users/steamuser/AppData/Local/Skyrim Special Edition
appdata_local_path(prefix, "Fallout4")
  == <prefix>/drive_c/users/steamuser/AppData/Local/Fallout4
```
Game folder names (A3): `"Skyrim Special Edition"` / `"Fallout4"` (Steam), matching libloadorder's `skyrim_se_appdata_folder_name` / `fallout4_appdata_folder_name`.

## Checkpoint (Task 0) — Resolved

The blocking package-legitimacy + GPL-3.0 checkpoint was **pre-approved by the user/orchestrator** before this run. Verified facts honored: libloot 0.29.5 is `GPL-3.0-or-later`, repo `github.com/loot/libloot` (the LOOT org), the project is already relicensed to GPL-3.0 (commit `d92d295`), and cargo-deny allows the family. Proceeded through to install as instructed.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking gate] cargo-deny rejected bare `GPL-3.0` from the libloot family**
- **Found during:** Task 1 verification (`cargo deny check`).
- **Issue:** Plan 01 added only a `GPL-3.0-or-later` allowance, assuming the whole libloot family declared that identifier. The actually-published crates declare TWO distinct SPDX identifiers: `libloot` 0.29.5 → `GPL-3.0-or-later`, but `libloadorder` 18.8.2 and `esplugin` 6.1.4 → bare `GPL-3.0` (the "v3.0 only" identifier, a deprecated but distinct token). `cargo deny check` FAILED licenses with libloot in the graph — a gate the plan's verification requires green.
- **Fix:** Added `"GPL-3.0"` alongside `"GPL-3.0-or-later"` in `deny.toml` `[licenses].allow`, with a comment recording the exact per-crate license map verified against the installed sources. License-compatible: NexTwist is GPL-3.0-or-later and may incorporate GPL-3.0-only components; the shipped AppImage is already GPL-3.0. Carried forward to DIST-02.
- **Files modified:** `deny.toml`
- **Commit:** `5fc7e04`

**2. [API adaptation] libloot 0.29.5 API differs from the plan's research-derived names** — documented in detail under "Deltas from the plan's research-derived assumptions" above (no separate `Game::save`; no active-plugin setter; `with_local_path` dir requirement; local_path is the AppData folder). Adapted the wrapper accordingly and recorded for Plan 04. Not a defect — the plan explicitly instructed verifying against the installed crate and adapting.

## Verification Results

- `cargo build -p nextwist-loadorder` — compiles, libloot 0.29.5 resolved.
- `cargo test -p nextwist-loadorder --test libloot_spike` — **4 passed** (A1/A3 de-risked: NO NoLocalAppData; asterisk Plugins.txt round-trips under the fixture AppData).
- `cargo test -p nextwist-loadorder` (lib + spike) — **8 passed** (4 loot.rs unit + 4 spike).
- `cargo test -p nextwist-testkit fake_proton_prefix` — **2 passed**.
- `cargo test --workspace` — all green, no Phase-1 regressions (every prior suite still passes).
- `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok (GPL-3.0 + GPL-3.0-or-later allowed).
- `cargo clippy -p nextwist-loadorder --all-targets -- -D warnings` — clean (after fixing a doc-overindented-list-items lint).

## Notes for Downstream Plans

- **Plan 04**: build `scan.rs`/`masterlist.rs` on `loot::open_game`. Use `Game::load_plugins(&[&Path])` then `Game::sort_plugins(&[&str]) -> Vec<String>` for "Sort with LOOT" (propose-then-apply); apply via `set_load_order`. Generate the Plugins.txt active flags from DB `plugin_state` BEFORE `load_current_load_order_state`, since there is no active-plugin setter. Every plugin named in an order MUST exist in `Data/`.
- **Plan 05**: `loadorder::apply(game, order)` = the `set_order_and_save` pattern, run after `deploy` inside the profile-switch confirmation.
- **`active_plugins_file_path()` is the bounded write target** (T-02-04) — Plan 04 should re-assert the path stays under the resolved prefix when writing for real prefixes.

## Self-Check: PASSED

- FOUND: crates/loadorder/Cargo.toml
- FOUND: crates/loadorder/src/lib.rs
- FOUND: crates/loadorder/src/error.rs
- FOUND: crates/loadorder/src/loot.rs
- FOUND: crates/loadorder/tests/libloot_spike.rs
- FOUND: crates/testkit/src/lib.rs (fake_proton_prefix)
- FOUND commit 5fc7e04 (Task 1 + deny deviation), 3512372 (Task 2), 7d5bb8a (Task 3)
