---
phase: 01-safe-local-round-trip
plan: 02
subsystem: steam-proton-discovery
status: complete
tags: [rust, steamlocate, keyvalues-serde, proton, compatdata, casing, flatpak, allow-list]
dependency_graph:
  requires:
    - "crates/core domain types (Game) from Plan 01"
    - "crates/store registry (add_managed_game) from Plan 01 — linked, persistence is the caller's job"
  provides:
    - "crates/steam: detect_games (native + Flatpak roots) + DetectedGame"
    - "resolve_game(appid) + injectable resolve_from_root(root, appid) test seam"
    - "Proton prefix derivation compatdata/<appid>/pfx (manual; steamlocate has no compatdata API) honoring $STEAM_COMPAT_DATA_PATH"
    - "add_game_by_folder(path, appid) manual fallback validating Bethesda markers"
    - "canonical_data_casing(install_dir) -> CasingMap (DEPLOY-08 input for deploy casefold)"
    - "supported-AppID allow-list: SKYRIM_SE 489830, FALLOUT4 377160, SUPPORTED_APPIDS, is_supported"
  affects:
    - "Plan 04/05 (deploy): consumes ResolvedGame paths + CasingMap for case normalization"
    - "Plan 06 (tauri): wraps detect_games/resolve_game/add_game_by_folder; persists via store::add_managed_game"
tech_stack:
  added:
    - "steamlocate 2.1 (locate_all, find_app -> (App, Library), SteamDir::from_dir)"
    - "keyvalues-serde 0.2 (appmanifest_<id>.acf installdir/name parsing)"
    - "walkdir 2.5 (Data/ tree traversal for casing map)"
  patterns:
    - "Steam/Proton path knowledge quarantined in crates/steam; deploy stays Proton-agnostic"
    - "Injectable resolve_from_root seam so CI never depends on a real Steam install"
    - "Re-resolve every call (no disk caching) — Steam can move/rebuild paths"
    - "Crate-local dep aliased nextwist_core (not core) to avoid shadowing Rust's built-in core in thiserror derive"
key_files:
  created:
    - crates/steam/Cargo.toml
    - crates/steam/src/lib.rs
    - crates/steam/src/error.rs
    - crates/steam/src/discover.rs
    - crates/steam/src/resolve.rs
    - crates/steam/src/casing.rs
    - crates/steam/tests/resolve_game.rs
    - crates/steam/tests/fixtures/.keep
  modified:
    - Cargo.lock
decisions:
  - "Alias the shared-types dep as nextwist_core (path dep) instead of the workspace `core` alias: steam derives thiserror::Error locally and a `core` extern crate shadows the built-in core:: paths thiserror expands to"
  - "Snap Steam root NOT auto-detected (RESEARCH A2 LOW confidence); Snap users use add_game_by_folder; TODO marker left in discover.rs"
  - "ResolvedGame carries prefix_exists so a derived-but-not-yet-created Proton prefix is a caller-surfaced warning, not an error"
  - "$STEAM_COMPAT_DATA_PATH override points at compatdata/<appid>; prefix = that + /pfx"
  - "add_game_by_folder validates Data/ + game exe case-insensitively before accepting (threat T-01-04)"
metrics:
  duration_min: 18
  tasks_completed: 2
  files_created: 8
  tests_passing: 16
  completed: 2026-06-20
---

# Phase 1 Plan 02: Steam/Proton Discovery & Resolution Summary

Built `crates/steam`, the headless crate that quarantines all Steam/Proton-layout knowledge: it auto-detects installed Steam games across native + Flatpak roots, enforces a two-game Bethesda allow-list (Skyrim SE 489830, Fallout 4 377160), resolves each game's install dir, manually derives the Proton prefix `compatdata/<appid>/pfx` (which steamlocate does not expose) while honoring `$STEAM_COMPAT_DATA_PATH`, offers a manual add-game-by-folder fallback that validates Bethesda markers, and produces a per-game canonical `Data/` casing map (DEPLOY-08 input) — all proven against synthetic Steam-layout fixtures so CI never needs a real Steam install.

## What Was Built

- **Task 1 — Discovery + Proton-prefix resolution** (`a77a737`): `crates/steam` scaffold (deps: steamlocate 2.1, keyvalues-serde 0.2, walkdir, core, store, thiserror, tracing). `discover.rs` enumerates Steam roots via `steamlocate::locate_all()` plus an explicit Flatpak-root probe (`~/.var/app/com.valvesoftware.Steam/.steam/steam`), filters to the supported AppIDs, and returns `DetectedGame { appid, name, library_path }`; Snap is deliberately excluded (A2). `resolve.rs` defines the `SKYRIM_SE`/`FALLOUT4` allow-list, `resolve_game(appid)` (rejects non-allow-listed AppIDs before any FS access, finds the app across roots, builds `install_dir = library/steamapps/common/<App.install_dir>`, derives `prefix = library/steamapps/compatdata/<appid>/pfx`), an injectable `resolve_from_root(root, appid)` test seam (reads `appmanifest_<id>.acf` via keyvalues-serde), `$STEAM_COMPAT_DATA_PATH` honoring, and `add_game_by_folder(path, appid)` validating a `Data/` dir + the game exe (case-insensitively) before accepting. `error.rs` holds the `SteamError` thiserror enum.
- **Task 2 — Canonical Data/ casing map + integration test** (`e114224`): `casing.rs` implements `canonical_data_casing(install_dir) -> CasingMap` — walks the game's `Data/` tree with walkdir and builds a serializable lowercase-relative-path → on-disk-canonical-casing map (the input Plan 05's `casefold.rs` rewrites mod paths with), locating `Data/` case-insensitively and recording only directories. `tests/resolve_game.rs` constructs a full synthetic Steam library under a `TempDir` (`libraryfolders.vdf` + `appmanifest_489830/377160.acf` + `steamapps/common/<dir>/Data/...` + `compatdata/<id>/pfx/`) and asserts both supported games resolve to the expected install dir + prefix, that a missing prefix is resolved-but-flagged-absent, and that the resolved install dir feeds a usable casing map.

## Interfaces Provided (contract for Plans 04–06)

- `steam::{SKYRIM_SE, FALLOUT4, SUPPORTED_APPIDS, is_supported}` — the allow-list (ENV-03).
- `steam::detect_games() -> Result<Vec<DetectedGame>, SteamError>` — native + Flatpak scan (ENV-01).
- `steam::resolve_game(appid) -> Result<ResolvedGame, SteamError>` and `resolve_from_root(root, appid)` (test seam) — install dir + derived prefix (ENV-02).
- `steam::add_game_by_folder(path, appid) -> Result<ResolvedGame, SteamError>` — manual fallback with marker validation (ENV-03).
- `ResolvedGame { appid, name, install_dir, prefix, prefix_exists }` + `into_game()` → `core::Game` (staging on the same FS as install).
- `steam::canonical_data_casing(install_dir) -> Result<CasingMap, SteamError>`; `CasingMap { data_dir_name, dirs }` with `canonical_dir(lower_rel)` lookup (DEPLOY-08 input).
- `steam::SteamError` — `Unsupported`/`NotInstalled`/`NoSteam`/`InvalidGameFolder`/`Locate`/`Io`.

## Verification

- `cargo test -p nextwist-steam` — **16 passed** (8 resolve + 5 casing lib tests + 3 resolve_game integration), 0 failed.
- `cargo test -p nextwist-steam --test resolve_game` — 3 passed against synthetic fixtures (no real Steam install).
- `cargo test --workspace` — **37 passed** (21 from Plan 01 + 16 new), 0 failed.
- `cargo build -p nextwist-steam` — clean.
- `cargo clippy -p nextwist-steam --all-targets` — 0 warnings.
- `cargo deny check bans` — `bans ok` (new deps respect the unrar ban).
- Acceptance checks confirmed: only 489830/377160 accepted (220 → `Unsupported`); prefix derived as `compatdata/<appid>/pfx` (not hardcoded steamuser/native HOME); `$STEAM_COMPAT_DATA_PATH` override honored; `add_game_by_folder` rejects folders lacking `Data/` + exe.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking compile error] Aliased the core dep as `nextwist_core` to avoid shadowing Rust's built-in `core`**
- **Found during:** Task 1 (`cargo test -p nextwist-steam --lib`)
- **Issue:** Plan 01 exposes the shared-types crate under the workspace dependency alias `core`. The store crate uses that alias safely because it only *imports* types. But the steam crate *derives* `thiserror::Error` locally, and thiserror's derive expands to built-in `core::fmt`/`core::option` paths — the `core` extern-crate alias shadowed Rust's prelude `core`, so the derive failed with "could not find `fmt`/`option` in `core`".
- **Fix:** Declared the dependency as `nextwist_core = { path = "../core", package = "nextwist-core" }` and imported `use nextwist_core::Game`. No version/behavior change; only the in-crate alias differs. Documented inline in `crates/steam/Cargo.toml`.
- **Files modified:** crates/steam/Cargo.toml, crates/steam/src/resolve.rs
- **Commit:** a77a737

**2. [Rule 1 - Test flakiness/bug] Serialized `$STEAM_COMPAT_DATA_PATH` env-var tests with a mutex**
- **Found during:** Task 1 (`cargo test -p nextwist-steam --lib`)
- **Issue:** Environment variables are process-global; the override test's `set_var` leaked into the no-override test under the default parallel test runner, producing a spurious assertion failure.
- **Fix:** Added a `static ENV_LOCK: Mutex<()>` and guarded both env-touching tests so they never run concurrently. Production code is unaffected.
- **Files modified:** crates/steam/src/resolve.rs
- **Commit:** a77a737

### Plan-stated open detail resolved

The plan flagged that the exact `App.install_dir` accessor name and `Library::path()` shape were "the only open detail" to confirm at code time. Confirmed directly against the vendored steamlocate 2.1.0 source: `App { app_id: u32, install_dir: String, name: Option<String> }`, `Library::path() -> &Path` (the library root, parent of `steamapps`), `SteamDir::find_app(app_id) -> Result<Option<(App, Library)>>`, `steamlocate::locate_all() -> Result<Vec<SteamDir>>`, `SteamDir::from_dir(&Path)`. No TODO left for these — they are now used directly.

## Threat Mitigations Applied

- **T-01-04 (untrusted add-game path):** `add_game_by_folder` validates the path is an existing directory containing a `Data/` subdir and the game executable (case-insensitively) before returning a `ResolvedGame` — invalid folders error via `SteamError::InvalidGameFolder`.
- **T-01-05 (malformed .acf/.vdf):** ACF parse failures map to `SteamError::Locate` (a typed error), never a panic.
- **T-01-06 (wrong-prefix spoofing):** prefix is derived from Steam's own layout + `$STEAM_COMPAT_DATA_PATH`, re-resolved each call, with no hardcoded `steamuser`/native-HOME assumptions.

## Notes for Downstream Plans

- **Plan 05 (deploy `casefold.rs`)** consumes `CasingMap`: keys are lowercased `/`-joined relative directory paths under `Data/`; values are the on-disk canonical casing. Only directories are recorded (leaf filename normalization, if needed, is deploy's call). `data_dir_name` carries the real top-level `Data` vs `data` casing.
- **Plan 06 (Tauri commands)** must call `store::add_managed_game(&resolved.into_game())` to persist — this crate intentionally only resolves and returns structs; it does not touch the DB.
- `resolve_from_root` is `pub` specifically as the CI-safe / fixture seam; production callers use `resolve_game` / `detect_games`.
- Snap support is deferred to `add_game_by_folder` (A2 LOW confidence); a TODO marker in `discover.rs` records this for a future verify-on-Snap pass.

## Known Stubs

None. `casing.rs` is fully implemented (the Task-1 placeholder was replaced in Task 2); every public function has unit and/or integration coverage. No placeholder/empty-return code remains.

## Self-Check: PASSED

- All 8 created files verified present on disk.
- Both commits verified in git log (`a77a737`, `e114224`).
- `cargo test -p nextwist-steam` 16/16 green; `cargo test --workspace` 37/37 green; clippy clean; `cargo deny check bans` ok.
