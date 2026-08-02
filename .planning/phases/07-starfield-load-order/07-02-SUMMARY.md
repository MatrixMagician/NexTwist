---
phase: 07-starfield-load-order
plan: 02
subsystem: tauri-command-wiring
tags: [starfield, load-order, tauri, thin-adapter, plugins, reconcile, sflo]
requires:
  - "07-01 (loadorder engine: PluginView, scan_plugin_views_for, protected_plugins, reconcile_plugins_txt, ReconcileState, masterlist_date)"
  - "src-tauri plugins adapter (v1.0 list_plugins / set_plugin_enabled / save_plugin_order / sort_with_loot)"
provides:
  - "commands::plugins::list_plugins now returns Vec<loadorder::PluginView> (medium + protected per row)"
  - "commands::plugins::reconcile_plugins(state, appid) -> Result<loadorder::ReconcileState, String>"
  - "reconcile_plugins registered in src-tauri generate_handler!"
affects:
  - "07-03 (frontend) renders medium/protected badges + the InSync/Drift reconcile verdict from these commands"
tech-stack:
  added: []
  patterns:
    - "Thin IPC adapter: gather store reads + one headless loadorder call, map typed error to String"
    - "Graceful degrade on a live-probe failure (log + empty set) so the read command never fails outright"
    - "View-only wire model (PluginView) at the boundary; frozen core::Plugin persisted underneath"
key-files:
  created: []
  modified:
    - src-tauri/src/commands/plugins.rs
    - src-tauri/src/lib.rs
decisions:
  - "protected filled from the live loadorder::protected_plugins probe for EVERY supported game (no Starfield special-case, no name literals in the adapter)"
  - "protected_plugins does no blocking HTTP (unlike sort_with_loot's masterlist fetch) so it is called directly under the lock — matches save_plugin_order, no spawn_blocking"
  - "reconcile reads Plugins.txt via std::fs::read_to_string; NotFound -> empty string (never-launched game reconciles cleanly), any other io error -> boundary error"
  - "set_plugin_enabled / save_plugin_order / sort_with_loot map PluginView down to core::Plugin at the call site (view-only booleans dropped)"
metrics:
  duration: ~20m
  completed: 2026-07-07
status: complete
---

# Phase 7 Plan 02: Starfield Load-Order Tauri Wiring Summary

Wired the three 07-01 engine additions through the thin `src-tauri` command layer so 07-03's frontend can render them: `list_plugins` now returns the richer `PluginView` (medium from the scan, protected from a live libloot probe), and a new thin `reconcile_plugins` command surfaces the SFLO-04 `ReconcileState`. Zero classification/reconcile logic lives in the adapter — every path forwards exactly one headless `loadorder` call. No `core` change, no DB migration, no new dependency.

## What was built

**Task 1 — `list_plugins` returns `PluginView` (commit cf7e035)**
- `merged_plugins_locked` now scans via `loadorder::scan_plugin_views_for` (carries `medium`), merges the per-profile `list_plugin_state` enable/order onto each view by name, then fills `protected` from a new `protected_set` helper.
- `protected_set(game, appid, enabled_names)` resolves the prefix AppData path (`appdata_folder_name` + `appdata_local_path`) and calls `loadorder::protected_plugins`. **Graceful degrade (checker W3):** a probe `Err` is logged via `tracing::warn!` and the protected set treated as EMPTY, so `list_plugins` still returns the full list — no SSE/FO4 regression vs v1.0's scan+store-only path.
- `merged_plugins` / `list_plugins` return `Vec<loadorder::PluginView>`. `set_plugin_enabled` builds a `core::Plugin` from the matched view before persisting (`set_plugin_state`'s frozen contract); `sort_with_loot` maps views down to `core::Plugin` for `propose_sort`.

**Task 2 — `reconcile_plugins` thin command + registration (commit 691365f)**
- New `#[tauri::command] async fn reconcile_plugins(state, appid) -> Result<loadorder::ReconcileState, String>`. Under one held lock it resolves the managed game + active profile, reads recorded `list_plugin_state`, reads the on-disk `Plugins.txt` at the prefix AppData (absent → empty string, not an error), computes the protected set, and forwards ONE call to `loadorder::reconcile_plugins_txt`.
- Registered `commands::plugins::reconcile_plugins` in `src-tauri/src/lib.rs` `generate_handler!` next to the other plugin commands. `deploy::verify` / `VerifyReport` untouched (SFLO-04 is a separate prefix-AppData surface — 07-RESEARCH Pitfall 4).

## Verification

- `cargo test -p nextwist --lib plugins --locked`: 1 passed (`save_plugin_order_inner_leaves_db_untouched_on_write_failure` still green), 0 failed.
- `cargo build -p nextwist --locked`: clean (src-tauri links WebKitGTK 4.1 — real build, not stubbed).
- `cargo test --workspace --locked`: all suites pass, 0 failed (full CI-equivalent gate across every crate + src-tauri).
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean.
- `crates/loadorder` / `crates/deploy` unchanged; no `core` change; no `V*.sql` migration; `Cargo.lock` / `deny.toml` untouched.

## Deviations from Plan

### Auto-fixed / minor

**1. [Rule 3 - Blocking] Plan's verify command names a non-existent package `nextwist-app`**
- **Found during:** Task 1 (running the first `<automated>` gate).
- **Issue:** The plan's verify commands use `-p nextwist-app`; the actual `src-tauri` crate is named `nextwist` (`src-tauri/Cargo.toml`, matching CLAUDE.md's `cargo build -p nextwist`). `-p nextwist-app` errors with "package ID specification did not match any packages".
- **Fix:** Ran the identical gate against the real package name `-p nextwist`. No source change; naming-only correction to the runbook.
- **Files modified:** none (command substitution only).
- **Commit:** n/a.

## Notes for downstream plans

- 07-03 must type `list_plugins`'s result as `PluginView[]` (each row carries `medium: bool` + `protected: bool`) and lock/badge from those booleans — the engine is the sole authority.
- `reconcile_plugins` returns the serde-external-tagged `ReconcileState`: `"InSync"` (bare string) or `{ Drift: string[] }`. Type it `"InSync" | { Drift: string[] }` (07-01 SUMMARY locks the wire shape).
- `protected` on a real machine depends on a resolvable Proton prefix + a libloot open succeeding; when the probe fails the UI simply shows nothing locked that call (by design — the list still renders).

## Self-Check: PASSED

- FOUND: src-tauri/src/commands/plugins.rs (reconcile_plugins + PluginView list_plugins)
- FOUND: src-tauri/src/lib.rs (reconcile_plugins registered)
- FOUND commits: cf7e035, 691365f
