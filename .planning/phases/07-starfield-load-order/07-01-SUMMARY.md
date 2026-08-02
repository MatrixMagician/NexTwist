---
phase: 07-starfield-load-order
plan: 01
subsystem: loadorder-engine
tags: [starfield, ce2, load-order, libloot, esplugin, reconcile, headless]
requires:
  - "crates/loadorder (v1.0 scan/apply/sort/masterlist engine)"
  - "crates/testkit (fake prefix + tree fixtures)"
  - "libloot 0.29.5 / esplugin 6.1.4 (already vendored)"
  - "Phase 6: GameType::Starfield + appdata_folder_name(1716740)=Starfield + bundled masterlist"
provides:
  - "loadorder::scan::PluginView { medium, protected } + scan_plugin_views_for"
  - "loadorder::loot::protected_plugins + defensive ProtectedMaster guard in apply_load_order"
  - "loadorder::masterlist::masterlist_snapshot_date + SortProposal.masterlist_date"
  - "loadorder::reconcile::{ReconcileState, reconcile_plugins_txt}"
  - "LoadOrderError::ProtectedMaster"
  - "testkit::{write_plugin_header, write_min_plugin, write_medium_plugin}"
affects:
  - "07-02 (Tauri command wiring) consumes PluginView/protected_plugins/ReconcileState/masterlist_date"
  - "07-03 (frontend) renders the medium/protected badges, masterlist-date note, InSync/Drift verdict"
tech-stack:
  added: []
  patterns:
    - "Derived-boolean wire model (PluginView) separate from frozen core::Plugin"
    - "libloot-derived protected set via is_plugin_active (never a hard-coded name list)"
    - "Pure compare-and-classify verdict (ReconcileState), not a raw text diff"
key-files:
  created:
    - crates/loadorder/src/reconcile.rs
    - crates/loadorder/tests/reconcile.rs
  modified:
    - crates/testkit/src/lib.rs
    - crates/loadorder/src/scan.rs
    - crates/loadorder/src/loot.rs
    - crates/loadorder/src/error.rs
    - crates/loadorder/src/masterlist.rs
    - crates/loadorder/src/lib.rs
    - crates/loadorder/tests/plugins.rs
    - crates/loadorder/Cargo.toml
    - Cargo.lock
decisions:
  - "Medium is a bool on PluginView, NOT a PluginKind::Medium variant (core/DB frozen)"
  - "Protected = is_plugin_active && !user-enabled; guard fires only on the implicit set, never a user-enabled *-line ESM master (FO4/SSE non-regression preserved)"
  - "Guard checks reorder (b) before disable (a) so a reorder attempt names the moved master"
  - "masterlist_snapshot_date is a recorded const (2026-07-07) — include_str! snapshot has no runtime mtime"
  - "ReconcileState uses default serde external tagging: 'InSync' bare string, {'Drift':[...]}"
metrics:
  duration: ~35m
  completed: 2026-07-07
status: complete
---

# Phase 7 Plan 01: Starfield Load-Order Engine Summary

Extended the shipped v1.0 headless load-order engine with the three CE2-specific additions — medium-master classification, a libloot-derived protected-master probe plus a defensive engine guard, and the SFLO-04 `plugins.txt` reconciler — with zero `core` change, zero DB migration, and zero dependency/version bump.

## What was built

**Task 1 — Medium classification + PluginView (commit 01481af)**
- `classify_kind` → `classify(game_id, path) -> (PluginKind, bool)` reads `esplugin::Plugin::is_medium_plugin()` alongside the existing light/master/regular precedence. A medium master stays `PluginKind::Esm`; `medium` is a separate boolean (no `PluginKind::Medium`).
- New serde `PluginView { name, kind, enabled, order, medium, protected }` + `scan_plugin_views_for`. `scan_plugins`/`scan_plugins_for` now delegate to the view scan and map down to `core::Plugin`, so the apply/sort callers are unchanged and the walk logic lives in one place.
- `testkit::write_plugin_header`/`write_min_plugin`/`write_medium_plugin` — one shared plugin-fixture builder (Starfield medium flag `0x400`) with a self-test.

**Task 2 — Protected probe + defensive guard + masterlist date (commit 810d4d6)**
- `LoadOrderError::ProtectedMaster(String)`.
- `loot::protected_plugins(appid, install, appdata, enabled_names)` derives the protected set purely from `Game::is_plugin_active` after `load_current_load_order_state()`, minus the plugins NexTwist itself enabled — no base-master name literal anywhere in the code path.
- Defensive guard in `apply_load_order`: after `load_canonical_order` it rejects, with the typed `ProtectedMaster` error BEFORE the libloot call, any request that reorders (checked first) or disables a plugin in the libloot-implicit protected set. It fires ONLY on that set, never a user-enabled `*`-line ESM master.
- `masterlist::masterlist_snapshot_date(appid)` (Starfield = `"2026-07-07"`) + `SortProposal.masterlist_date`.

**Task 3 — SFLO-04 reconcile (commit 1023221)**
- `reconcile::reconcile_plugins_txt(recorded, on_disk_txt, protected) -> ReconcileState { InSync | Drift(Vec<String>) }` — a pure function (no FS/libloot I/O). Parses on-disk active `*`-lines as opaque filenames, derives user intent from recorded enabled (excluding protected), and classifies protected `.ccc`/stripped-implicit/re-added-blueprint deltas as EXPECTED (`InSync`) and any beyond-expected user-mod delta as `Drift(sorted unique names)`.

## Verification

- Named assertions present and green: `medium` (2), `protected`, `protected_reorder_rejected`, `protected_disable_rejected`, `starfield_asterisk`, `starfield_sort_determinism`, `reconcile_expected_deltas`, `reconcile_real_drift` (+ reorder/identical-empty/serde-wire-shape).
- FO4 non-regression `fo4_multi_master_game_master_first_active_survives` stays GREEN — the guard is dormant for user-enabled masters.
- `cargo test --workspace --locked`: **300 passed, 0 failed**.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `crates/core` unchanged; no `V*.sql` migration; `deny.toml` unchanged; no dep/MSRV bump.

## Deviations from Plan

### Auto-fixed / minor

**1. [Rule 3 - Blocking] Added already-vendored `serde_json` as a `loadorder` dev-dependency**
- **Found during:** Task 3.
- **Why:** The plan's key_link requires the `ReconcileState` serde wire shape (`"InSync"` bare string vs `{"Drift":[...]}`) to be locked by a test — the frontend depends on it. Asserting the exact JSON needs `serde_json`, which `loadorder` did not list as a dev-dep.
- **Fix:** Added `serde_json.workspace = true` under `[dev-dependencies]`. `serde_json 1.x` is already vendored and in `Cargo.lock` (used by other crates), so this is NOT a new external package or a version bump — `Cargo.lock` changed by exactly one line (listing `serde_json` under `loadorder`'s dependency set). `deny.toml` unchanged.
- **Files modified:** `crates/loadorder/Cargo.toml`, `Cargo.lock`.
- **Commit:** 1023221.

## Notes for downstream plans

- 07-02 must fill `PluginView.protected` at the command layer by calling `protected_plugins(...)` after the scan (scan defaults it `false`), and surface `SortProposal.masterlist_date` + a `ReconcileState` field.
- The Starfield-specific implicit set (which `SFBGS0xx`/`.ccc`/blueprint names) and the exact on-launch rewrite deltas remain MEDIUM-confidence (Phase-9 hardware-confirmed); the protected set and reconcile EXPECTED set are data-driven from libloot's active set, so they self-correct against the real game.

## Self-Check: PASSED

- FOUND: crates/loadorder/src/reconcile.rs
- FOUND: crates/loadorder/tests/reconcile.rs
- FOUND commits: 01481af, 810d4d6, 1023221
