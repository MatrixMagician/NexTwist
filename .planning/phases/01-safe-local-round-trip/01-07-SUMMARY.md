---
phase: 01-safe-local-round-trip
plan: 07
subsystem: deploy
tags: [gap-closure, deploy, purge, recovery, verify, repair, pristine, directory-cleanup]
status: complete
gap_closure: true
requirements: [DEPLOY-03, DEPLOY-07]
dependency_graph:
  requires:
    - "store::list_deployed_files (manifest cleanup set source)"
    - "deploy::deploy_root / resolve_target (deploy-root boundary)"
    - "testkit::snapshot_tree / assert_trees_identical (pristine assertion)"
  provides:
    - "engine::remove_emptied_dirs (manifest-derived, deploy-root-bounded empty-dir cleanup)"
    - "journal::ReplayOutcome { replayed, purged_rels } (recovery cleanup set)"
    - "verify::VerifyReport.orphan_dirs + RepairReport.removed_orphan_dirs (DEPLOY-07 dir coverage)"
    - "testkit directory-aware TreeSnapshot (DIR_SENTINEL)"
  affects:
    - "purge() / recover_on_launch() now directory-pristine"
    - "verify()/repair() now classify+remove orphan empty dirs"
tech_stack:
  added: []
  patterns:
    - "Manifest/journal-derived cleanup set (never a blind disk scan)"
    - "Bottom-up std::fs::remove_dir as a safety net (refuses non-empty dirs)"
    - "Reserved non-hex sentinel to distinguish directory entries from blake3 file hashes"
key_files:
  created: []
  modified:
    - crates/testkit/src/lib.rs
    - crates/deploy/src/engine.rs
    - crates/deploy/src/journal.rs
    - crates/deploy/src/verify.rs
    - crates/deploy/tests/verify_drift.rs
decisions:
  - "Track directory shape in testkit via a reserved DIR_SENTINEL marker (non-hex, can never collide with a 64-char blake3 digest) rather than a separate dir-set field — keeps the existing key-based MUTATED/MISSING/ORPHAN diff intact."
  - "Derive the empty-dir cleanup set from the manifest rows purge already iterates (and from journal-replayed purge relpaths for recovery), never from a disk scan — preserves the core-value safety invariant."
  - "journal::replay returns ReplayOutcome (purged_rels) so recover_on_launch reuses the exact same cleanup helper as purge() — one code path, no disk scan."
  - "repair removes orphan empty dirs to a fixed point so a nested empty-dir chain is fully cleaned in one call, leaving the tree pristine; file orphans remain strictly report-only (T-01-16 preserved)."
metrics:
  duration: ~25 min
  tasks_completed: 3
  files_modified: 5
  tests_added: 5
  completed: 2026-06-20
---

# Phase 1 Plan 07: Close GAP-01 (purge leaves orphan empty directories) Summary

Manifest-derived, deploy-root-bounded empty-directory cleanup in `purge()` and the crash-recovery purge branch, plus a directory-aware pristine assertion and verify/repair coverage of orphan empty dirs — so "byte-for-byte pristine" now means the directory tree too, not just file contents.

## What Was Built

GAP-01 (blocker, DEPLOY-03) was a real-world Skyrim SE repro: after install→deploy→purge, `Data/` retained 3 orphan empty directories that `deploy()` created but `purge()` never removed. The automated `round_trip_pristine` proptest missed it because `testkit::snapshot_tree` hashed file contents only and was blind to empty directories. Fixed RED→GREEN across three coordinated moves:

1. **Task 1 (RED)** — `testkit::snapshot_tree` now records every descendant directory keyed by its root-relative path with a reserved `DIR_SENTINEL` (`"<dir>"`, non-hex so it can never collide with a 64-char blake3 digest). The root itself is not recorded, so an empty tree snapshots empty and self-equality holds. `assert_trees_identical` (unchanged in shape) now flags a leftover empty directory as `ORPHAN`. This made `round_trip_pristine` FAIL on a mod-subdir case (minimal: `Data/textures/a.esp` leaves orphan `Data/textures`) — proving the bug.

2. **Task 2 (GREEN)** — `engine::remove_emptied_dirs(install_dir, removed_rels)`: for each removed `target_rel`, collect the ancestor chain strictly below the deploy root, dedupe, sort deepest-first, `std::fs::remove_dir` each. `DirectoryNotEmpty`/`NotFound` are benign skips (the safety net protecting vanilla dirs and dirs holding unmanaged orphans); any other IO error propagates. A defence-in-depth guard drops any candidate equal-to/ancestor-of the deploy root. `purge()` calls it after its per-file loop. `journal::replay` now returns `ReplayOutcome { replayed, purged_rels }`, and `recover_on_launch` runs the SAME helper over the journal-replayed purge relpaths — so a crash-mid-purge converges to a directory-pristine tree without any disk scan.

3. **Task 3 (DEPLOY-07)** — `VerifyReport.orphan_dirs: Vec<PathBuf>` + `RepairReport.removed_orphan_dirs: usize`. `verify::walk_orphan_dirs` collects EMPTY dirs under the deploy root that provenance does not explain (not an ancestor of a managed target nor a vanilla original); a non-empty dir is never an orphan dir. `repair` removes exactly those, iterating to a fixed point so a nested empty-dir chain is fully cleaned in one call. File orphans remain strictly report-only — repair never deletes a file (T-01-16 / T-01-20).

## Key Implementation Details

- **Safety argument (documented in code at `remove_emptied_dirs`):** candidates are manifest/journal-derived (never the vanilla tree); `remove_dir` refuses non-empty dirs; the candidate set is constructed strictly below the deploy root so the game `Data/` boundary is never crossed.
- **No new dependencies** — uses `std::fs` + existing `walkdir`. No schema migration (cleanup set derives from the existing manifest). `cargo deny check bans` unaffected (`bans ok`).
- **Report consumers safe** — `commands/deploy.rs` and `src-tauri/src/lib.rs` access the reports via serde/field access only; both reports derive `Default`, so the additive fields are non-breaking.

## Tasks Completed

| Task | Name | Commit | Files |
| ---- | ---- | ------ | ----- |
| 1 | testkit directory-aware + reproduce orphan-empty-dir (RED) | `fa0152a` | crates/testkit/src/lib.rs |
| 2 | remove deploy-created empty dirs in purge + recovery (GREEN) | `29cda71` | crates/deploy/src/engine.rs, crates/deploy/src/journal.rs |
| 3 | detect+remove orphan empty dirs in verify/repair (DEPLOY-07) | `ff94898` | crates/deploy/src/verify.rs, crates/deploy/tests/verify_drift.rs |

## Deviations from Plan

None — plan executed exactly as written. The RED→GREEN→DEPLOY-07 ordering held; the RED commit was preserved per the TDD plan.

One minor in-task correction (not a plan deviation): the initial Task-3 `repair` did a single pass, leaving a nested empty-dir chain's parent behind. Corrected to iterate to a fixed point so one `repair` call fully cleans the chain and leaves the tree pristine — matching the `<behavior>` "leaving the tree pristine" requirement.

## Verification Results

Phase gate — all green:

- `cargo test --workspace` (incl. doctests): **82 passed, 0 failed** (up from the 77 baseline; deploy lib 10, verify_drift 8 incl. 3 new, testkit 8 incl. 2 new).
- `cargo clippy --workspace --all-targets -- -D warnings`: **0 warnings** (fixed two `sort_by` → `sort_by_key(Reverse)` lints introduced by the new code).
- `cargo deny check bans`: **bans ok** (unrar ban active; no new deps).
- Integration suite (`round_trip_pristine` + `crash_recovery` + `verify_drift`) green on **tmpfs (st_dev 49)** AND the **dev btrfs filesystem (st_dev 50, `TMPDIR` under repo)** per VALIDATION.md — the btrfs run exercises the reflink/hardlink primitives tmpfs lacks.
- **GAP-01 repro intent enforced automatically:** `round_trip_pristine`'s proptest now FAILS without the fix (RED, captured at `fa0152a`) and PASSES with it (GREEN) — a mod whose files live in a mod-introduced subdir leaves ZERO leftover directories after purge.

## RED Confirmation (Task 1)

```
ORPHAN   Data/textures (in actual, not in expected)
summary: 0 mutated, 0 missing, 1 orphan
minimal failing input: vanilla = [], modfiles = [ GenFile { rel: "Data/textures/a.esp", bytes: [] } ]
```
The `proptest-regressions` seed is gitignored (per Plan 04) and was not committed.

## Self-Check: PASSED

- Files exist: crates/testkit/src/lib.rs, crates/deploy/src/engine.rs, crates/deploy/src/journal.rs, crates/deploy/src/verify.rs, crates/deploy/tests/verify_drift.rs — all FOUND.
- Commits exist: `fa0152a`, `29cda71`, `ff94898` — all FOUND in git log.
- No file deletions in any task commit; no stubs in modified source.
