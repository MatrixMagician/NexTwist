---
phase: 08-reversible-starfieldcustom-ini-activation
plan: 02
subsystem: deploy
tags: [rust, deploy, engine, reversibility, crash-recovery, starfield, verify, repair]

# Dependency graph
requires:
  - phase: 08-reversible-starfieldcustom-ini-activation
    plan: 01
    provides: "gameconfig::{ensure_ini_active, restore_ini, preview_ini_activation}, journal KIND_INI + replay_ini, three-valued vanilla provenance, IniConflictResolution/IniOutcome"
provides:
  - "is_starfield-gated INI hooks at the deploy / deploy_winners / purge tails (auto-activate on deploy, provenance-restore on purge)"
  - "is_starfield-gated INI-drift participation in verify()/repair() (VerifyReport.ini_drift + RepairReport.restored_ini) via gameconfig::ini_drift + IniDrift"
  - "the crates/deploy/tests/ini_activation.rs reversibility suite (SFINI-04/05 regression lock across both provenance branches)"
affects: [08-03-tauri-ui, 09-hardware-validation]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Out-of-Data/-root reversible op wired at engine choke-point TAILS behind a single is_starfield gate; redeploy_winners (= purge + deploy_winners) covers profile switch for free"
    - "INI participates in verify/repair as a distinct gated check (VerifyReport.ini_drift) resolved entirely in gameconfig — never resolve_target/guard_within_root, never widening the deploy_root-bounded orphan walk (T-08-03)"
    - "Drift detection reuses editor::plan_merge vs current bytes (already-active = no-op) with list_deployed_files non-empty as the should-be-active signal"

key-files:
  created:
    - crates/deploy/tests/ini_activation.rs
  modified:
    - crates/deploy/src/engine.rs
    - crates/deploy/src/verify.rs
    - crates/deploy/src/gameconfig.rs
    - crates/deploy/src/lib.rs

key-decisions:
  - "Reused editor::plan_merge for verify drift detection so 'already exactly active' is a byte-compare no-op — preview alone (will_edit=exists) cannot distinguish active from key-stripped, so the raw editor is the correct oracle"
  - "should-be-active is keyed off list_deployed_files(appid) non-empty — the SAME signal verify already reads — so the INI is active exactly when loose files are deployed and clear after purge"
  - "repair re-activates under Block so a user conflict stays Blocked (ini_drift is None for a conflict) and is never treated as repairable drift / clobbered"

requirements-completed: [SFINI-04, SFINI-05]

coverage:
  - id: D1
    description: "Starfield deploy/deploy_winners auto-activate the INI at the tail; purge restores to provenance; redeploy_winners (profile switch) round-trips for free (SFINI-04)"
    requirement: "SFINI-04"
    verification:
      - kind: integration
        ref: "crates/deploy/tests/ini_activation.rs#ini_restore_preexisting_byte_for_byte, ini_restore_absence_prunes_created_dirs, ini_idempotent_one_archive"
        status: pass
    human_judgment: false
  - id: D2
    description: "Both provenance branches proven byte-for-byte pristine (DIR_SENTINEL) after activate->purge; exactly one [Archive] under deploy×2 / redeploy / crash-replay (SFINI-04)"
    requirement: "SFINI-04"
    verification:
      - kind: integration
        ref: "crates/deploy/tests/ini_activation.rs#ini_restore_preexisting_byte_for_byte, ini_restore_absence_prunes_created_dirs, ini_idempotent_one_archive, ini_merge_nonclobber, ini_crlf_bom_preserved"
        status: pass
    human_judgment: false
  - id: D3
    description: "recover_on_launch replays a crashed KIND_INI op to a provenance-consistent state (bytes for PreExisting, absence+prune for CreatedByNexTwist), no pending rows; the target stays under the prefix, never Data/ (SFINI-05)"
    requirement: "SFINI-05"
    verification:
      - kind: integration
        ref: "crates/deploy/tests/ini_activation.rs#ini_crash_recovery_consistent, ini_kind_path_under_prefix"
        status: pass
    human_judgment: false
  - id: D4
    description: "verify() detects a removed or key-stripped StarfieldCustom.ini while loose files are deployed and repair() re-activates it byte-identically; a user-blocked conflict is NOT drift and is never clobbered; non-Starfield games untouched (SFINI-05)"
    requirement: "SFINI-05"
    verification:
      - kind: integration
        ref: "crates/deploy/tests/ini_activation.rs#ini_verify_pristine_when_active, ini_verify_detects_removed_ini, ini_verify_detects_key_stripped_ini, ini_verify_conflict_is_not_drift, ini_verify_ignores_nonstarfield"
        status: pass
    human_judgment: false
  - id: D5
    description: "The is_starfield gate is regression-locked: a non-Starfield (Skyrim SE) deploy touches no INI and takes no StarfieldCustom.ini vanilla row; the Data/ deploy path + deploy_root-bounded orphan walk are unchanged (T-08-03/T-08-07)"
    requirement: "SFINI-04"
    verification:
      - kind: integration
        ref: "crates/deploy/tests/ini_activation.rs#nonstarfield_deploy_touches_no_ini, ini_verify_ignores_nonstarfield; unchanged vanilla_restore.rs + crash_recovery.rs + verify suites"
        status: pass
    human_judgment: false
  - id: D6
    description: "Preview reports will-create vs will-edit + the exact two lines + conflict without writing (SFINI-01, exercised end-to-end here)"
    requirement: "SFINI-01"
    verification:
      - kind: integration
        ref: "crates/deploy/tests/ini_activation.rs#ini_preview_reports_intent_without_writing"
        status: pass
    human_judgment: false

# Metrics
duration: 7min
completed: 2026-07-08
status: complete
---

# Phase 8 Plan 02: Reversible StarfieldCustom.ini Activation — Choke-point Wiring + Reversibility Suite Summary

**A handful of `is_starfield`-gated tail calls wire the Plan-01 INI engine into `deploy`/`deploy_winners`/`purge` and into `verify`/`repair`, so loose-file activation rides the SAME reversibility lifecycle as `Data/` deployment — auto-ensure on deploy, provenance-restore on purge, idempotent crash-replay, verify/repair drift detection + re-activation — all locked against regression by a 15-test reversibility suite covering both provenance branches. Zero new crates; the `Data/`-root guard and orphan walk are byte-for-byte untouched.**

## Performance

- **Duration:** ~7 min
- **Tasks:** 3 (Tasks 1 and 3 TDD)
- **Files:** 5 (1 created, 4 modified)

## Accomplishments
- **Choke-point wiring (engine.rs):** `ensure_ini_active(Block)` at the tails of the PUBLIC `deploy` (captured from `deploy_inner`, so the `deploy_with_abort` crash-sim path never activates) and `deploy_winners`; `restore_ini` inside `purge` after `remove_emptied_dirs` — each behind `game.appid == steam::STARFIELD`. `redeploy_winners` (= purge + deploy_winners) round-trips profile switch for free (no separate codepath); `recover_on_launch` already drives the Plan-01 `KIND_INI` replay arm (no new call site).
- **verify/repair participation (verify.rs + gameconfig.rs):** a new `gameconfig::ini_drift` + `IniDrift{Missing,Changed}`, an `is_starfield`-gated `VerifyReport.ini_drift` folded into `pristine`, and a gated `repair()` re-activation (`RepairReport.restored_ini`). All INI/path logic stays in `gameconfig` (resolved via `my_games_path`); `verify.rs` never touches `resolve_target`/`guard_within_root` and never widens the `deploy_root`-bounded orphan walk (T-08-03).
- **Reversibility suite (crates/deploy/tests/ini_activation.rs, NEW):** 15 tests driven through the public engine — both provenance branches proven byte-for-byte pristine via `snapshot_tree`/`assert_trees_identical` (DIR_SENTINEL), idempotency (deploy×2 / `redeploy_winners` / crash-replay → exactly one `[Archive]`), CRLF/BOM no-op, conflict-blocks, KIND_INI-path-under-prefix, crash recovery for both provenance branches, verify-detects-removed/key-stripped + repair-restores, conflict-is-not-drift, and non-Starfield controls.

## Task Commits

1. **Task 1: RED reversibility suite** — `a79b5b2` (test) — the full suite, compiling clean with the wiring-dependent reversibility assertions failing.
2. **Task 2: is_starfield INI hooks at deploy/deploy_winners/purge tails** — `a051d81` (feat) — greens the Task-1 rows; vanilla_restore + crash_recovery unchanged.
3. **Task 3: is_starfield INI-drift check in verify/repair (SFINI-05)** — `35b3d65` (feat) — `gameconfig::ini_drift`/`IniDrift`, the gated verify/repair fields + calls, and the 5 appended verify/repair test rows.

## Files Created/Modified
- `crates/deploy/tests/ini_activation.rs` (NEW) — 15-test reversibility suite (SFINI-01..05 observable truths + non-Starfield controls), extending `testkit::{snapshot_tree, assert_trees_identical, DIR_SENTINEL, fake_my_games_prefix}`.
- `crates/deploy/src/engine.rs` — `is_starfield`-gated `ensure_ini_active` at the `deploy` + `deploy_winners` tails and `restore_ini` inside `purge`; `use crate::gameconfig::{self, IniConflictResolution}`.
- `crates/deploy/src/verify.rs` — `VerifyReport.ini_drift: Option<IniDrift>` (folded into `pristine`), `RepairReport.restored_ini`, the two `steam::STARFIELD`-gated calls into `gameconfig`.
- `crates/deploy/src/gameconfig.rs` — `pub enum IniDrift{Missing,Changed}` + `pub fn ini_drift` (reuses `editor::plan_merge`; `list_deployed_files` non-empty = should-be-active; conflict = not-drift; already-active = no-op = clear).
- `crates/deploy/src/lib.rs` — re-export `ini_drift` + `IniDrift`.

## Decisions Made
- **`editor::plan_merge` is the drift oracle, not `preview`.** `preview_ini_activation.will_edit` is merely "the file exists", so it cannot tell an already-active INI from a key-stripped one. `ini_drift` plans the merge against the current bytes and treats a byte-identical plan as "already active → clear", `will_create`/absent as `Missing`, a differing plan as `Changed`, and a `Conflict` as not-drift.
- **`list_deployed_files(appid)` non-empty is the should-be-active signal** — the same source of truth `verify` already reads for `Data/` drift — so the INI is drift-checked exactly when loose files are deployed and is clear after a purge (which already restored it).
- **`repair` re-activates under `Block`.** A genuine user conflict yields `ini_drift == None` (a recorded state, not drift), so `repair` never re-activates or clobbers it — mirroring the deploy-tail hook.

## Deviations from Plan

None - plan executed exactly as written. The Plan-01 three-valued provenance and `KIND_INI` replay arm were consumed verbatim; the plan's optional "bool vs enum" call for the verify field was resolved as an `Option<IniDrift{Missing,Changed}>` enum (folded into `pristine`) for a richer UI signal.

**Total deviations:** 0.
**Impact:** None — all artifacts, gates, and prohibitions (T-08-03 copy-paste trap, is_starfield gating, orphan-walk non-widening) honored as written.

## Issues Encountered
None.

## User Setup Required
None — code + local-filesystem only; zero external crates, no `cargo-deny`/MSRV/AppImage surface change.

## Next Phase Readiness
- **Plan 08-03 (Tauri/UI)** can consume `preview_ini_activation` (→ `IniActivationPreview`), `ensure_ini_active(.., IniConflictResolution)` (→ `IniOutcome`), and now `verify().ini_drift` (→ `Option<IniDrift>`) / `repair().restored_ini` as thin adapters. The engine surface for SFINI-01..05 is complete.
- **Phase 9** corrects the CE2 recipe (Assumption A1) on real hardware by editing the one labelled `gameconfig` constants block only — the wiring + reversibility machinery is recipe-agnostic and unaffected.

---
*Phase: 08-reversible-starfieldcustom-ini-activation*
*Completed: 2026-07-08*

## Self-Check: PASSED
- All created/modified files present on disk (ini_activation.rs, engine.rs, verify.rs, gameconfig.rs, lib.rs, 08-02-SUMMARY.md).
- All three task commits present in git history (a79b5b2, a051d81, 35b3d65).
- `cargo test --workspace --locked` green (45 suites ok, 0 failed); `cargo clippy --workspace --all-targets -- -D warnings` clean; `ini_activation` suite 15/15; vanilla_restore + crash_recovery + verify unchanged and green.
- T-08-03 grep-confirmed: verify.rs references no `my_games_path`/`guard_within_root` for the INI; both INI arms behind `steam::STARFIELD`; `deploy_inner`/`deploy_with_abort` carry no INI hook.
