---
phase: 08-reversible-starfieldcustom-ini-activation
plan: 01
subsystem: deploy
tags: [rust, deploy, ini, reversibility, starfield, journal, provenance]

# Dependency graph
requires:
  - phase: 06-starfield-detection
    provides: "steam::my_games_path (drive_c-contained, case-folded CE2 config resolver)"
  - phase: v1.0-deploy-engine
    provides: "backup::{backup_vanilla_if_absent, restore_vanilla}, journal intent-before-act protocol, remove_emptied_dirs prune discipline"
provides:
  - "std-only surgical StarfieldCustom.ini byte editor (BOM/EOL/comment/order-preserving merge of the two owned [Archive] keys)"
  - "gameconfig op wrappers: ensure_ini_active / restore_ini / preview_ini_activation + shared restore_ini_at"
  - "journal KIND_INI token + begin_ini + replay_ini (resolves via my_games_path, never Data/)"
  - "DeployError::IniConflict typed conflict arm"
  - "store::remove_vanilla DELETE facade (no migration)"
affects: [08-02-choke-point-wiring, 08-03-tauri-ui, 09-hardware-validation]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Surgical byte-fidelity INI merge over std (no parse-and-reserialize crate)"
    - "Three-valued provenance in the existing vanilla_backup table: no row / ABSENCE_MARKER / real hash"
    - "Out-of-Data/-root reversible op via a distinct journal kind + independent path resolution"

key-files:
  created:
    - crates/deploy/src/gameconfig.rs
  modified:
    - crates/deploy/src/error.rs
    - crates/deploy/src/lib.rs
    - crates/deploy/src/journal.rs
    - crates/store/src/vanilla.rs

key-decisions:
  - "Three-valued provenance in vanilla_backup (no row=never-activated, ABSENCE_MARKER=CreatedByNexTwist, real hash=PreExisting) — the only self-consistent reading that satisfies BOTH 'restore driven by the vanilla_backup row' AND 'no-op cleanly when never activated' without deleting a user's untouched INI"
  - "Added store::remove_vanilla (DELETE facade, no migration) so restore resets provenance to never-activated, closing a future-user-file data-loss window (T-08-05)"
  - "AlreadyActive short-circuits BEFORE journaling/backup so a second activation never re-captures our own created file as a pre-existing vanilla original"
  - "Hand-rolled the ~60-line editor over std; rejected rust-ini/ini-roundtrip on byte-fidelity grounds (zero new crates)"

patterns-established:
  - "KIND_INI replay copies replay_purge's body verbatim, swapping ONLY the target resolution to steam::my_games_path — the Data/-root guard is byte-for-byte untouched"
  - "Atomic INI write via sibling temp + fs::rename (never in-place fs::write)"

requirements-completed: [SFINI-01, SFINI-02, SFINI-03, SFINI-04]

coverage:
  - id: D1
    description: "Surgical std-only merge preserves BOM/CRLF/comments/section+key order, converges to exactly one [Archive], new file = CRLF+no-BOM, idempotent re-merge byte-identical (SFINI-03)"
    requirement: "SFINI-03"
    verification:
      - kind: unit
        ref: "crates/deploy/src/gameconfig.rs#editor_tests (11 tests: detect_bom, new_file_is_crlf_no_bom_exact, merge_into_existing_archive_preserves_everything_else, merge_without_archive_appends_one_block_with_detected_eol, idempotent_remerge_is_byte_identical_single_archive, crlf_bom_noop_roundtrip_is_byte_for_byte_identical)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Non-empty user sResourceDataDirsFinal surfaces as a typed conflict and is never clobbered under Block; overwritten only under UseNexTwist after whole-file backup (SFINI-03)"
    requirement: "SFINI-03"
    verification:
      - kind: unit
        ref: "crates/deploy/src/gameconfig.rs#editor_tests::nonempty_user_value_is_a_conflict_under_block, wrapper_tests::ensure_blocks_nonempty_user_value_and_use_nextwist_overwrites"
        status: pass
    human_judgment: false
  - id: D3
    description: "Provenance-driven restore: PreExisting INI restored byte-for-byte; CreatedByNexTwist INI deleted + NexTwist-created dirs pruned; never-activated user INI left untouched (SFINI-02)"
    requirement: "SFINI-02"
    verification:
      - kind: unit
        ref: "crates/deploy/src/gameconfig.rs#wrapper_tests::restore_preexisting_restores_original_bytes, restore_created_deletes_file_and_prunes_created_dir, restore_is_a_safe_noop_when_never_activated"
        status: pass
    human_judgment: false
  - id: D4
    description: "preview_ini_activation reports will_create vs will_edit + the exact two lines + any conflict without writing or touching the store (SFINI-01)"
    requirement: "SFINI-01"
    verification:
      - kind: unit
        ref: "crates/deploy/src/gameconfig.rs#wrapper_tests::preview_absent_reports_will_create_and_writes_nothing, preview_existing_empty_reports_will_edit_no_conflict, preview_conflict_reports_value_no_write"
        status: pass
    human_judgment: false
  - id: D5
    description: "The op rides KIND_INI on a bare StarfieldCustom.ini sentinel; replay_ini resolves via my_games_path (asserted under prefix, never Data/) and rolls a pending row back to provenance idempotently; ensure leaves no pending rows (SFINI-04)"
    requirement: "SFINI-04"
    verification:
      - kind: unit
        ref: "crates/deploy/src/journal.rs#ini_replay_tests::replay_ini_resolves_under_prefix_not_data_and_rolls_back; gameconfig.rs#wrapper_tests::ensure_creates_new_ini_and_is_idempotent_no_pending"
        status: pass
    human_judgment: false
  - id: D6
    description: "Write-site security: drive_c containment re-verified at the write site (T-08-01) and a foreign symlink at the INI target is refused (T-08-02); INI write is atomic temp+rename (T-08-04)"
    requirement: "SFINI-04"
    verification:
      - kind: unit
        ref: "crates/deploy/src/gameconfig.rs#wrapper_tests::verify_contained_refuses_an_escaping_target, ensure_refuses_to_write_through_a_symlink"
        status: pass
    human_judgment: false
  - id: D7
    description: "Exact CE2 loose-file INI-key recipe (Assumption A1) actually loads loose files in-game"
    requirement: "SFINI-01"
    verification: []
    human_judgment: true
    rationale: "MEDIUM-confidence recipe — requires the real game build on Proton hardware; deferred to the Phase 9 on-hardware gate (SFVER-01). Kept in one labelled constants block so Phase 9 corrects it without touching the machinery."

# Metrics
duration: 40min
completed: 2026-07-08
status: complete
---

# Phase 8 Plan 01: Reversible StarfieldCustom.ini Activation Engine Summary

**Std-only surgical byte-fidelity StarfieldCustom.ini editor plus journaled ensure/restore/preview op wrappers that reuse the vanilla-backup ledger and operation journal verbatim, resolving the target only via steam::my_games_path (the Data/-root guard untouched) — with three-valued provenance so restore never deletes a user's untouched INI. Zero new crates.**

## Performance

- **Duration:** ~40 min
- **Tasks:** 2 (both TDD)
- **Files modified:** 5 (1 created, 4 modified)

## Accomplishments
- A ~60-line std-only surgical INI byte editor that merges the two owned `[Archive]` keys (`bInvalidateOlderFiles=1`, empty `sResourceDataDirsFinal=`) in place, preserving BOM, dominant EOL, comments, blank lines, section + key order, and every untouched byte; a new file is CRLF + no-BOM; re-merge converges to exactly one `[Archive]` byte-identically.
- Journaled op wrappers `ensure_ini_active` / `restore_ini` / `preview_ini_activation` + shared `restore_ini_at`, reusing `backup::{backup_vanilla_if_absent, restore_vanilla}` and the intent-before-act journal verbatim; the target resolves ONLY via `steam::my_games_path`, so the `Data/`-root guard (`resolve_target`/`guard_within_root`) is never involved.
- `journal::KIND_INI` + `begin_ini` (bare `StarfieldCustom.ini` sentinel, no `Data/` prefix) + `replay_ini`, which copies `replay_purge`'s body and swaps ONLY the target resolution — proven by a test that the replayed path is under the prefix and never leaves a stray `Data/StarfieldCustom.ini`.
- Typed `DeployError::IniConflict { current_value }`; a non-empty user `sResourceDataDirsFinal` returns `Ok(IniOutcome::Blocked)` (never an overwrite, never an Err) under `Block`, overwritten only under `UseNexTwist` after a whole-file backup.
- Security mitigations wired: `drive_c` containment re-verified at the write site (T-08-01), foreign-symlink refusal (T-08-02), atomic temp+rename write (T-08-04), provenance-driven restore that never deletes a user's untouched INI (T-08-05/T-08-06).

## Task Commits

Each task was committed atomically:

1. **Task 1: Surgical std-only INI byte editor + IniConflict + module decl** — `a6fc34b` (feat)
2. **Task 2: Journaled ensure/restore/preview wrappers + KIND_INI replay** — `2c387b1` (feat)

_Both tasks are TDD; tests are co-located `#[cfg(test)]` modules committed with their implementation._

## Files Created/Modified
- `crates/deploy/src/gameconfig.rs` (NEW) — CE2 recipe constants (Assumption A1, one labelled block); the `editor` submodule (BOM/EOL detect, surgical merge, conflict detect); op wrappers `ensure_ini_active`/`restore_ini`/`preview_ini_activation`/`restore_ini_at`; public serde types `IniActivationPreview`/`IniConflictResolution`/`IniOutcome`; 21 unit tests.
- `crates/deploy/src/error.rs` — `DeployError::IniConflict { current_value }` arm.
- `crates/deploy/src/lib.rs` — `pub mod gameconfig;` + re-exports of the public gameconfig symbols and `INI_FILENAME`.
- `crates/deploy/src/journal.rs` — `KIND_INI`, `begin_ini`, `replay_ini`, the `KIND_INI` dispatch arm, and a replay-under-prefix test.
- `crates/store/src/vanilla.rs` — `store::remove_vanilla` (DELETE facade; no schema/migration).

## Decisions Made
- **Three-valued provenance (the crux).** The plan's must-have says restore is "driven by the vanilla_backup row" AND restore_ini must "no-op cleanly when the target was never activated." A two-valued row (present=PreExisting / absent=CreatedByNexTwist) makes "never activated" and "CreatedByNexTwist" indistinguishable, which would make restore delete a user's untouched INI (T-08-05). The only self-consistent reading records a reserved `ABSENCE_MARKER` in the existing `vanilla_backup.hash` for the created case, giving: no row → never activated (safe no-op); `ABSENCE_MARKER` → CreatedByNexTwist (delete+prune); real blake3 hash → PreExisting (restore bytes). No new column, no migration.
- **`store::remove_vanilla`.** Restore drops the provenance row at the end of `restore_ini_at` (crash-safe: the preceding remove/restore/prune are idempotent while the row persists, and once dropped the state is already final). This resets provenance to "never activated" so a *future* user-created INI is never mistaken for ours.
- **AlreadyActive short-circuits before journaling/backup** — otherwise a second activation would `backup_vanilla_if_absent` our own created file and mis-record it as a pre-existing vanilla original.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Three-valued provenance + `store::remove_vanilla` to make restore's "never-activated" gate safe**
- **Found during:** Task 2 (restore_ini / restore_ini_at design).
- **Issue:** The plan's action text described a two-valued provenance (vanilla row present → PreExisting; absent → CreatedByNexTwist → delete+prune) while ALSO requiring `restore_ini` to "gate/no-op cleanly when the target was never activated." Under two values, "never activated" == "CreatedByNexTwist" (both have no row), so `restore_ini` would delete a user's untouched StarfieldCustom.ini during a purge — a T-08-05 data-loss bug.
- **Fix:** Record a reserved `ABSENCE_MARKER` in the existing `vanilla_backup.hash` for the created case (three-valued provenance, no schema change), branch restore on it, and add a small `store::remove_vanilla` DELETE facade to reset provenance after restore. This satisfies BOTH must-haves ("driven by the vanilla_backup row" and "no-op when never activated") and never infers provenance from disk.
- **Files modified:** `crates/deploy/src/gameconfig.rs`, `crates/store/src/vanilla.rs`.
- **Verification:** `wrapper_tests::restore_is_a_safe_noop_when_never_activated` (user INI survives), `restore_created_deletes_file_and_prunes_created_dir`, `restore_preexisting_restores_original_bytes`; full workspace + clippy green.
- **Committed in:** `2c387b1` (Task 2 commit).

---

**Total deviations:** 1 auto-fixed (1 missing-critical, security/data-integrity).
**Impact on plan:** Necessary to uphold the phase's core reversibility guarantee; stays within the plan's `store` "confirm — likely no change" note (a DELETE facade, no migration) and the "no new column, no migration" constraint. No scope creep.

## Issues Encountered
None — both tasks executed as planned aside from the provenance deviation above.

## User Setup Required
None — code + local-filesystem only; zero external crates, no `cargo-deny`/MSRV/AppImage surface change.

## Next Phase Readiness
- **Plan 08-02 (choke-point wiring)** can now call `ensure_ini_active` at the tail of `deploy_winners`, `restore_ini` inside `purge`, gated on `game.appid == steam::STARFIELD`; `recover_on_launch` already drives `journal::replay`, which picks up the new `KIND_INI` arm automatically. It must also add the `is_starfield`-gated INI-drift arm to `verify()`/`repair()` (SFINI-05).
- **Plan 08-03 (Tauri/UI)** consumes `preview_ini_activation` (→ `IniActivationPreview`) and `ensure_ini_active(.., IniConflictResolution)` (→ `IniOutcome`) as thin adapters.
- **Phase 9** corrects the CE2 recipe (Assumption A1, D7) on real hardware by editing the one labelled constants block only — the reversibility machinery is recipe-agnostic.

---
*Phase: 08-reversible-starfieldcustom-ini-activation*
*Completed: 2026-07-08*

## Self-Check: PASSED
- All created/modified files present on disk (gameconfig.rs, error.rs, lib.rs, journal.rs, vanilla.rs, 08-01-SUMMARY.md).
- Both task commits present in git history (a6fc34b, 2c387b1).
- Full workspace `cargo test --workspace --locked` green (0 failures); `cargo clippy --workspace --all-targets -- -D warnings` clean; zero new external crates.
