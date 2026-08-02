---
phase: 02-multi-mod-management
plan: 05
subsystem: profile vertical slice (switch reconcile + Tauri commands + Svelte selector)
tags: [profiles, switch, purge-deploy-reconcile, pristine, plugins.txt, tauri, svelte, PROF-01, PROF-02, PROF-03, D-13, D-14, D-15, D-16]
status: complete
requires:
  - "Plan 01: profile/profile_mod/plugin_state store CRUD + set_active_profile + core Profile/Plugin types + auto-Default profile"
  - "Plan 03: deploy::conflict::resolve + deploy_winners + WinnerFile/ModInput (winner-set deploy through the unchanged safe engine)"
  - "Plan 04: loadorder::apply_load_order(appid, install_dir, appdata_local, &[Plugin]) + appdata_folder_name/appdata_local_path"
  - "Phase-1 deploy engine (purge/recover, journal, testkit DIR_SENTINEL pristine harness)"
provides:
  - "deploy::switch_profile(store, game, target_profile_id) -> SwitchReport — purge(old) -> resolve+deploy_winners(new) -> apply_load_order(new plugins.txt) -> set_active(new)"
  - "deploy::SwitchReport { purged: PurgeReport, deployed: DeployReport, plugins_txt: PathBuf }"
  - "DeployError::Profile(String) — wraps non-engine reconcile failures (plugins.txt write / store read)"
  - "Tauri commands: list_profiles / create_profile / switch_profile / delete_profile"
  - "frontend Profile selector + confirmation-gated switch/delete modals (UI-SPEC §D)"
affects:
  - "Phase-2 complete — this is the capstone vertical slice; no downstream Phase-2 plan depends on it"
tech-stack:
  added: []
  patterns:
    - "Profile switch = full purge-to-pristine then fresh deploy of the target set (NEVER diff-deploy; Pitfall 4 / D-15)"
    - "Composition over the EXISTING safe primitives (purge + deploy_winners + apply_load_order) — the engine is never reimplemented or bypassed"
    - "deploy -> loadorder edge (acyclic: loadorder depends only on store + core) so switch_profile writes plugins.txt directly; the Tauri adapter does NOT re-apply"
    - "set_active only AFTER a successful deploy (the active flag never points at a half-applied state)"
    - "Confirmation-gated disk mutation in the UI: selecting a profile opens a modal; the engine runs only on confirm (D-15)"
key-files:
  created:
    - crates/deploy/src/profile.rs
    - crates/deploy/tests/profile_switch.rs
    - src-tauri/src/commands/profiles.rs
  modified:
    - crates/deploy/Cargo.toml
    - crates/deploy/src/lib.rs
    - crates/deploy/src/error.rs
    - src-tauri/src/commands/mod.rs
    - src-tauri/src/lib.rs
    - frontend/src/lib/api.ts
    - frontend/src/routes/+page.svelte
    - Cargo.lock
decisions:
  - "switch_profile wiring = deploy -> loadorder DIRECT call (apply_load_order inside switch_profile), NOT command-level apply. loadorder depends only on store + core (NOT deploy), so the deploy -> loadorder edge is acyclic; this keeps the full reconcile (purge -> deploy -> plugins.txt -> set_active) atomic in one headless function and the Tauri adapter a pure 1-line delegate."
  - "SwitchReport shape = { purged: PurgeReport, deployed: DeployReport, plugins_txt: PathBuf } — surfaces both halves of the reconcile (what was restored + what was deployed) plus the written plugins.txt path for the UI."
  - "Added DeployError::Profile(String) for non-engine reconcile failures (plugins.txt write via libloot returns LoadOrderError, not StoreError) so the boundary error stays typed and the deploy/purge halves' own errors remain distinct."
  - "Per-profile rank (from profile_mod) — not the global managed_mod.rank — drives the winner set, so the same shared mod can win in one profile and lose in another (PROF-03)."
  - "Delete keeps staged mod files (D-14): the command calls store.delete_profile (profile + profile_mod + plugin_state rows only); no staging-store deletion."
metrics:
  duration_min: 18
  tasks: 2
  files: 11
  tests_added: 2
  completed: 2026-06-21
---

# Phase 2 Plan 05: Profile Vertical Slice Summary

A user can create multiple independent per-game profiles, switch the active one (confirmation-gated) to change which mods/plugins/order are deployed, and delete profiles — each profile preserving its own enabled-mod set + per-profile priority + plugin order. Switching reconciles the on-disk deployment THROUGH the existing journaled safe engine (purge old → deploy new winner set → write new plugins.txt → mark active), with the round-trip-pristine guarantee regression-locked ACROSS switches (A→B→A leaves the game byte-for-byte restorable to vanilla). This completes the Phase-2 multi-mod management capability.

## What Was Built

- **`deploy::switch_profile`** (`crates/deploy/src/profile.rs`) — the headless reconcile, a pure composition over the EXISTING safe primitives (D-15 / RESEARCH Pattern 4):
  1. `purge(store, game)` — manifest-driven crash-safe restore to byte-for-byte pristine. Total purge between profiles means profile A's unique files can never leak into B (T-02-15; Pitfall 4 — never a diff-deploy).
  2. Read the target profile's enabled membership (`list_profile_mods` joined against `list_mods` for staging roots, tagged with **per-profile** ranks) → build `ModInput`s → `conflict::resolve` → `deploy_winners` (the unchanged journaled per-file primitive).
  3. `loadorder::apply_load_order` — write the target profile's asterisk `plugins.txt` at the Proton-prefix AppData location (Plan-04 primitive; PROF-02 carries plugin order, D-13).
  4. `store.set_active_profile` — ONLY after a successful deploy (exactly one active; the flag never points at a half-applied state, T-02-16).
- **`SwitchReport { purged, deployed, plugins_txt }`** — serializable, crosses the Tauri boundary.
- **`DeployError::Profile(String)`** — wraps non-engine reconcile failures (plugins.txt write / store read).
- **`src-tauri/commands/profiles.rs`** — four thin adapters (`require_game` + one store/headless call + `boundary_err`): `list_profiles`, `create_profile` (returns the created Profile), `switch_profile` (delegates to `deploy::switch_profile`), `delete_profile` (keeps staged files — D-14). Zero business logic.
- **frontend** — `Profile`/`SwitchReport` types + `listProfiles`/`createProfile`/`switchProfile`/`deleteProfile` bindings; the §D Profile selector: per-game list with the active (deployed) profile marked by the Accent indicator + "active" label, **confirmation-gated** switch modal (only on confirm does the engine run — D-15), Destructive-red delete confirmation (D-14 copy), per-profile preservation (switch reloads the conflict + plugin lists), and the "Default profile" empty state.

## Plan-Required Recordings

- **switch_profile wiring chosen:** `deploy → loadorder` DIRECT call. `apply_load_order` is called INSIDE `switch_profile` (not at the command level). `loadorder` depends only on `store` + `core` (NOT `deploy`), so the `deploy → loadorder` edge is acyclic — confirmed by a clean `cargo build`. This keeps the entire reconcile atomic in one tested headless function; the Tauri `switch_profile` command is a single-line delegate.
- **SwitchReport shape:** `{ purged: PurgeReport, deployed: DeployReport, plugins_txt: PathBuf }`.
- **Cross-switch pristine invariant regression-locked?** YES — `crates/deploy/tests/profile_switch.rs::profile_switch_round_trips_pristine_across_switches` asserts byte-for-byte pristine after A→B→A + final purge (testkit DIR_SENTINEL), and that A→B→A reproduces profile A's exact deployed set (PROF-03).

## The BLOCKING-PRISTINE Test (Task 1)

`crates/deploy/tests/profile_switch.rs` (testkit DIR_SENTINEL harness, mirroring conflict_redeploy.rs; mods staged at independent roots OUTSIDE `game.staging_dir`):

- **`profile_switch_round_trips_pristine_across_switches`** — seeds two profiles with different enabled sets/per-profile ranks (A = mods {1,2}, mod1 wins shared.esp by rank; B = mod {3} only). Snapshots vanilla pristine. Switches A (deploys 3, mod1's bytes win) → B (purges A's 3, deploys mod3's 1; asserts A's shared.esp/only1/only2 are GONE — T-02-15 no leak) → A again (asserts the install snapshot is byte-for-byte identical to the first A deploy — PROF-03 each profile reproduces its own set). Final purge → `assert_trees_identical(pristine, after)` with zero orphans. Uses a fresh `Store` handle per switch (relaunch resilience). NON-NEGOTIABLE pristine-across-switches: **passes.**
- **`switch_writes_target_profile_plugins_txt_at_prefix`** — asserts the SwitchReport's `plugins_txt` lives under the resolved Proton-prefix AppData/Local/<game> path and the file was written (PROF-02 plugins.txt wiring). The deep libloot plugins.txt content round-trip is covered by the Plan-04 loadorder tests (`crates/loadorder/tests/plugins.rs`) per the planned split.

TDD gate sequence honored: `test(02-05)` RED commit `e914ffe` (failed: `switch_profile`/`SwitchReport` unresolved) → `feat(02-05)` GREEN commit `7479652`.

## Deviations from Plan

None — the plan executed exactly as written. The deploy→loadorder direct-call wiring was the plan's preferred path (it explicitly allowed the command-level fallback only if a cycle would result; no cycle results, so the direct path was taken). No bugs, missing functionality, blocking issues, or architectural changes were encountered.

## Threat-Model Mitigations Applied

- **T-02-14 (switch interrupted / integrity):** switch composes the EXISTING journaled, crash-safe `purge` + `deploy_winners`; `recover_on_launch` replays a partial op on next start. `profile_switch.rs` regression-locks byte-for-byte pristine across A→B→A (Pitfall 4).
- **T-02-15 (stale files leak between profiles):** full purge-to-pristine BETWEEN profiles (never diff-deploy); the cross-switch test asserts A's unique files (shared.esp winner, only1, only2) are gone after switching to B.
- **T-02-16 (accidental loss from UI):** every disk-mutating profile action is confirmation-gated (switch modal + Destructive-red delete modal, UI-SPEC §D.2 + Copywriting); delete keeps staged mod files; `set_active` runs only after a successful deploy.
- **T-02-17 (duplicate/invalid profile):** accepted per plan — `store` `UNIQUE(appid,name)` rejects dupes; the create command surfaces the error verbatim; single-user desktop, low risk.

No new security surface beyond the threat_model.

## Verification Results

- `cargo test -p nextwist-deploy --test profile_switch` — **2 passed** (PROF-02 cross-switch pristine + PROF-03 set reproduction; plugins.txt prefix wiring).
- `cargo test --workspace` — **142 passed**, 0 failed (Phase-1 + Plans 01/02/03/04 baseline 140 + 2 new; additive, no regressions).
- `cargo build -p nextwist` — compiles (four profile commands registered).
- `cd frontend && npm run check` — 141 files, **0 errors, 0 warnings**.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok (no new crate added; `loadorder` was already in the graph).

## Notes

- No new dependency or package-legitimacy checkpoint: `loadorder` is an existing workspace crate; the only change is a new intra-workspace `deploy → loadorder` path edge (acyclic).
- `delete_profile` is intentionally NOT confirmation-checked at the headless layer (the store call is idempotent); the confirmation gate lives in the UI per D-15, and the command surfaces the result verbatim.

## Self-Check: PASSED

- FOUND: crates/deploy/src/profile.rs
- FOUND: crates/deploy/tests/profile_switch.rs
- FOUND: src-tauri/src/commands/profiles.rs
- FOUND: frontend/src/routes/+page.svelte (§D Profiles section + switch/delete modals)
- FOUND commit e914ffe (RED test), 7479652 (Task 1 GREEN), 6d346f3 (Task 2)
