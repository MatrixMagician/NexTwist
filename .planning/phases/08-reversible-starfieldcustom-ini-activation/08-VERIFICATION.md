---
phase: 08-reversible-starfieldcustom-ini-activation
verified: 2026-07-08T00:00:00Z
status: passed
score: 5/5 must-haves verified
behavior_unverified: 0
overrides_applied: 0
deferred:
  - truth: "The CE2 loose-file recipe (Assumption A1: [Archive] bInvalidateOlderFiles=1 + empty sResourceDataDirsFinal) actually loads loose files in the running game"
    addressed_in: "Phase 9"
    evidence: "Phase 9 on-hardware gate (SFVER-01); recipe isolated in one labelled constants block so Phase 9 corrects it without touching the machinery. Recorded as human_judgment D7 in 08-01-SUMMARY."
  - truth: "End-to-end UI interaction on Proton hardware (preview modal renders the exact lines + correct provenance, activation-state refreshes, conflict box triggers on a real non-empty user value)"
    addressed_in: "Phase 9"
    evidence: "human_judgment D3/D4 in 08-03-SUMMARY, deferred to Phase 9 on-hardware UAT. Code path + copy are grep-verified; only the live visual/interaction needs a running Starfield+Proton session."
---

# Phase 8: Reversible StarfieldCustom.ini Activation — Verification Report

**Phase Goal:** Users can enable loose-file loading for Starfield through a reversible `StarfieldCustom.ini` edit that upholds the non-destructive, byte-for-byte reversible safety guarantee. Highest-risk new safety surface of the milestone.
**Verified:** 2026-07-08
**Status:** passed
**Re-verification:** No — initial verification (post-REVIEW, verifying CR-01/WR-01/WR-02 fixes landed in code)

## Goal Achievement

The safety-critical crux of the phase — a byte-for-byte reversible, journaled, provenance-driven INI edit outside the `Data/` deploy root — is fully implemented and proven by the headless reversibility suite. The three REVIEW findings (1 blocker CR-01 + 2 warnings WR-01/WR-02) are each present in code with dedicated regression tests. Both authoritative gates are green.

### Observable Truths

| #   | Truth (requirement) | Status | Evidence |
| --- | ------------------- | ------ | -------- |
| 1 | SFINI-01 — User can enable loose-file loading; NexTwist writes the required `[Archive]` keys, via a no-silent-edit preview | ✓ VERIFIED | `preview_ini_activation` (gameconfig.rs:502, read-only, writes nothing — test `ini_preview_reports_intent_without_writing`); Tauri `preview_ini_activation`/`apply_ini_activation` adapters; Svelte preview modal (`Enable` opens modal only, `onActivateIni` is the sole Block writer — grep -a confirmed) |
| 2 | SFINI-02 — Provenance recorded (pre-existing vs created); purge restores original bytes OR deletes+prunes | ✓ VERIFIED | Three-valued provenance (no row / `ABSENCE_MARKER` / real hash) at gameconfig.rs:57-63; `restore_ini_at` driven only by the ledger row (gameconfig.rs:556-576); tests `ini_restore_preexisting_byte_for_byte`, `ini_restore_absence_prunes_created_dirs` pass |
| 3 | SFINI-03 — Surgical merge preserves byte fidelity (line endings, BOM, comments, order), no clobber of user keys | ✓ VERIFIED | `editor::plan_merge`/`merge_body` (raw-line/EOL/BOM-preserving, gameconfig.rs:295-416); non-empty user value blocks (`IniOutcome::Blocked`); UTF-16/32 refused (WR-02); tests `ini_merge_nonclobber`, `ini_crlf_bom_preserved`, `ini_conflict_blocks`, `ini_utf16_bom_is_refused_no_write` |
| 4 | SFINI-04 — INI activation journaled + idempotent; interrupted/repeated deploy/purge is recoverable | ✓ VERIFIED | `KIND_INI` token + `begin_ini` + `replay_ini` (via `my_games_path`, never `resolve_target`); atomic temp+rename write; tests `ini_idempotent_one_archive`, `ini_kind_path_under_prefix`, `ini_crash_recovery_consistent` |
| 5 | SFINI-05 — INI participates in the same reversibility guarantee: `recover_on_launch` + verify/repair | ✓ VERIFIED | `verify()` folds `ini_drift` into `pristine` (verify.rs:121-129, Starfield-gated); `repair()` re-activates (verify.rs:216-223); `recover_on_launch` drives `journal::replay` → `KIND_INI` arm; tests `ini_verify_detects_removed_ini`, `ini_verify_detects_key_stripped_ini`, `ini_purge_crash_window_recovers_via_journal` |

**Score:** 5/5 truths verified (0 present-behavior-unverified)

### REVIEW Fix Verification (BLOCKER + WARNINGS resolved in code)

| Finding | Fix present in code | Regression test |
| ------- | ------------------- | --------------- |
| CR-01 (BLOCKER) — re-activating an absent PreExisting INI downgraded provenance → purge deletes user file | ✓ Guard at gameconfig.rs:472 — `if !pre_existing && store.vanilla_for(...)?.is_none()` — only stamps `ABSENCE_MARKER` when NO row exists; authoritative real-hash row never downgraded | `ini_reactivation_of_absent_preexisting_preserves_original_on_purge` (ini_activation.rs:572) — seeds PreExisting, deletes on disk, repairs, purges, asserts original bytes restored byte-for-byte |
| WR-01 (WARNING) — interrupted-purge window left INI activated with no journal-recoverable intent | ✓ `purge_inner` journals `KIND_INI` restore intent BEFORE the `Data/` loop (engine.rs:455-467); actual restore + `mark_done` at the tail (engine.rs:509-513); `purge_with_abort_before_ini` crash seam | `ini_purge_crash_window_recovers_via_journal` (ini_activation.rs:416) |
| WR-02 (WARNING) — UTF-16 BOM detected but body parsed as bytes → silent clobber/corruption | ✓ `is_unsupported_bom` (gameconfig.rs:167-171) → `MergePlan::Unsupported` → `IniOutcome::UnsupportedEncoding`; writes nothing, takes no row (gameconfig.rs:450-452, 316-318); `ini_drift` treats it as not-drift (gameconfig.rs:543) | `ini_utf16_bom_is_refused_no_write`; editor unit `utf16_and_utf32_bom_are_refused_utf8_is_editable` |

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `crates/deploy/src/gameconfig.rs` | Surgical editor + op wrappers + provenance | ✓ VERIFIED | 1000 lines; std-only; CE2 recipe in one labelled block; `ensure_ini_active`/`restore_ini`/`preview_ini_activation`/`ini_drift`; drive_c re-verify + symlink refusal + atomic write |
| `crates/deploy/src/journal.rs` | `KIND_INI` + `begin_ini` + `replay_ini` | ✓ VERIFIED | `replay_ini` resolves via `my_games_path`, never `resolve_target`/`guard_within_root`; test proves no stray `Data/StarfieldCustom.ini` |
| `crates/deploy/src/verify.rs` | Starfield-gated INI-drift in verify/repair | ✓ VERIFIED | `VerifyReport.ini_drift` folded into `pristine`; `RepairReport.restored_ini`; orphan walk unchanged; no `my_games_path`/`resolve_target` in verify.rs for the INI |
| `crates/deploy/src/engine.rs` | is_starfield-gated hooks at deploy/deploy_winners/purge tails | ✓ VERIFIED | Hooks only on PUBLIC `deploy`/`deploy_winners` tails + `purge` (not `deploy_inner`/`deploy_with_abort`); WR-01 pre-journaling in `purge_inner` |
| `crates/deploy/tests/ini_activation.rs` | Reversibility suite, both provenance branches | ✓ VERIFIED | 18 tests (grew from 15 with CR-01/WR-01/WR-02 regressions); DIR_SENTINEL byte-for-byte assertions |
| `src-tauri/src/commands/gameconfig.rs` | Thin adapter, zero safety logic | ✓ VERIFIED | 35 lines; two commands, each `require_game` + one `deploy::` call + `boundary_err` |
| `frontend/src/lib/api.ts` | INI TS types + bindings | ✓ VERIFIED | `IniActivationPreview`/`IniOutcome`/`IniConflictResolution` + `previewIniActivation`/`applyIniActivation` |
| `frontend/src/routes/+page.svelte` | Three Starfield surfaces | ✓ VERIFIED (grep -a) | Activation-state tag, no-silent-edit preview modal, amber conflict box; all copy strings present; wired to live engine data |

### Key Link Verification

| From | To | Via | Status |
| ---- | -- | --- | ------ |
| `engine::deploy`/`deploy_winners` tail | `gameconfig::ensure_ini_active` | is_starfield gate (engine.rs:125,296) | ✓ WIRED |
| `engine::purge` | `gameconfig::restore_ini_at` + pre-journaled `KIND_INI` intent | engine.rs:455-513 | ✓ WIRED |
| `recover_on_launch` | `journal::replay` → `KIND_INI` → `replay_ini` | journal.rs:152,214 | ✓ WIRED |
| `verify`/`repair` | `gameconfig::ini_drift` / `ensure_ini_active` | steam::STARFIELD gate (verify.rs:121,216) | ✓ WIRED |
| Tauri `apply_ini_activation` | `deploy::ensure_ini_active` | gameconfig.rs adapter:34 | ✓ WIRED |
| `+page.svelte` Enable/modal | `api.previewIniActivation`/`applyIniActivation` | grep -a confirmed | ✓ WIRED |

### Behavioral Spot-Checks / Gate Results

| Gate | Command | Result | Status |
| ---- | ------- | ------ | ------ |
| Deploy crate suite (incl. ini_activation 18 tests) | `~/.cargo/bin/cargo test -p nextwist-deploy --locked` | exit 0, 0 failed | ✓ PASS |
| Full workspace test gate | `~/.cargo/bin/cargo test --workspace --locked` | exit 0, 0 FAILED across run | ✓ PASS |
| Full workspace clippy gate | `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings` | exit 0, `Finished` clean | ✓ PASS |

### Requirements Coverage

| Requirement | Source Plan(s) | Status | Evidence |
| ----------- | -------------- | ------ | -------- |
| SFINI-01 | 08-01, 08-03 | ✓ SATISFIED | preview no-silent-edit engine + adapter + modal (truth 1) |
| SFINI-02 | 08-01 | ✓ SATISFIED | three-valued provenance restore (truth 2); CR-01 hardened |
| SFINI-03 | 08-01, 08-03 | ✓ SATISFIED | surgical byte-fidelity merge + conflict block + UTF-16 refusal (truth 3) |
| SFINI-04 | 08-01, 08-02 | ✓ SATISFIED | KIND_INI journal + idempotency + atomic write (truth 4) |
| SFINI-05 | 08-02 | ✓ SATISFIED | recover_on_launch + verify/repair + WR-01 crash-window (truth 5) |

All 5 declared requirement IDs cross-referenced against REQUIREMENTS.md (lines 31-35, 71-75) — each present, mapped to Phase 8, and marked Complete. No orphaned requirements.

### Anti-Patterns Found

None material. No unreferenced `TBD`/`FIXME`/`XXX` debt markers in the phase files. The one `ponytail:`-style deliberate-simplification discipline and the Assumption-A1 recipe block are documented and Phase-9-scoped. IN-01 (dead `DeployError::IniConflict` variant) and IN-02 (frontend `VerifyReport` omits `ini_drift`) are Info-level, explicitly left per REVIEW — IN-02 is Phase-9-scoped; neither affects reversibility or the phase goal.

### Deferred Items (Phase 9)

| # | Item | Addressed In | Evidence |
| - | ---- | ------------ | -------- |
| 1 | CE2 recipe (Assumption A1) actually loads loose files in-game | Phase 9 | On-hardware gate SFVER-01; recipe isolated for a machinery-free correction (08-01-SUMMARY D7) |
| 2 | End-to-end UI visual/interaction on Proton hardware | Phase 9 | 08-03-SUMMARY D3/D4 human_judgment; code path + copy grep-verified, live session needed |

Per the verification boundary, on-hardware CE2-key validation and SFVER-02 (deployed-vs-loaded UI) are Phase 9 scope and are NOT flagged as gaps here.

### Gaps Summary

None. The phase goal — a reversible, byte-for-byte, journaled, provenance-driven `StarfieldCustom.ini` activation that never destroys or clobbers a user's file — is achieved and proven by a passing headless reversibility suite plus green workspace test + clippy gates. The one BLOCKER and two WARNINGs from deep REVIEW are each fixed in code with reproducing regression tests. The only remaining validation (real-game recipe correctness + on-hardware UI feel) is intentionally deferred to Phase 9.

---

_Verified: 2026-07-08_
_Verifier: Claude (gsd-verifier)_
