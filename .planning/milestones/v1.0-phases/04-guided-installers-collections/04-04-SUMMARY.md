---
phase: 04-guided-installers-collections
plan: 04
subsystem: collections-lifecycle
tags: [collections, fomod-replay, switch_profile, purge, reversible-uninstall, premium-gate, coll-02, coll-03, coll-04, coll-05]

# Dependency graph
requires:
  - phase: 04-01
    provides: "crates/fomod headless parse->resolve engine + Selection type"
  - phase: 04-03
    provides: "nexus::Collection parser, resolve_collection/ResolveReport, V5 store collection facade"
  - phase: 02
    provides: "deploy::switch_profile / purge (reversible deploy), store profile_mod rank model, testkit blake3 pristine harness"
  - phase: 03
    provides: "run_download_to_window (stream->extract->stage->persist), shared governor limiter, UserInfo.is_premium"
provides:
  - "nexus::replay_choices (IChoices -> fomod::Selection, name-matched, stale-safe)"
  - "nexus::map_rules_to_ranks (modRules after/before/conflicts + fileOverrides -> rank model)"
  - "nexus::is_auto_fetchable (nexus/bundle vs off-Nexus classification)"
  - "commands/collections.rs: resolve_collection/download_collection/deploy_collection/uninstall_collection adapters"
  - "crates/deploy/tests/collection_round_trip.rs (COLL-04 deploy + COLL-05 install->uninstall pristine, no network)"
  - "Collections UI surface (api.ts wrappers + +page.svelte §B/§C)"
affects: [phase-05, any-collection-or-deploy-work]

# Tech tracking
tech-stack:
  added: ["futures-util (src-tauri only — bounded-concurrency buffer_unordered for bulk download)"]
  patterns:
    - "Collections add ZERO new engine primitives: bulk download reuses run_download_to_window VERBATIM; deploy = create_profile->set_profile_mod->switch_profile; uninstall = purge->clear-active->delete_profile->remove staged trees"
    - "Headless IChoices replay drives the SAME fomod::resolve the interactive wizard uses (no parallel install engine)"
    - "Stale-pin safety: a manifest choice no longer matching ModuleConfig.xml returns NexusError::Replay, never a silent mis-install (A3/Pitfall 4)"
    - "Premium gate is a pure testable decision (premium_gate) checked BEFORE any download (T-04-16)"
    - "Owned download coordinates (idx,name,mod_id,file_id) carried into buffer_unordered so no manifest borrow over-constrains the async-command lifetimes"

key-files:
  created:
    - crates/nexus/src/replay.rs
    - src-tauri/src/commands/collections.rs
    - crates/deploy/tests/collection_round_trip.rs
  modified:
    - crates/nexus/src/lib.rs
    - crates/nexus/src/error.rs
    - crates/nexus/src/resolve.rs
    - crates/nexus/Cargo.toml
    - src-tauri/src/commands/mod.rs
    - src-tauri/src/lib.rs
    - src-tauri/Cargo.toml
    - frontend/src/lib/api.ts
    - frontend/src/routes/+page.svelte

key-decisions:
  - "replay_choices returns NexusError::Replay (a NEW, distinct error variant) on a stale step/group/option name so the UI surfaces 'this mod changed — run its installer manually' rather than mis-installing the stale plan."
  - "map_rules_to_ranks is a key_for-parameterized pure function returning RankAdjustment deltas (after ⇒ +1, before ⇒ -1, conflicts recorded no-delta, fileOverrides force-win); the orchestrator seeds a baseline rank and applies the deltas. A rule with an unresolved endpoint is skipped, not fatal."
  - "nexus::ResolveReport/ResolvedMod/ModStatus now derive Serialize so the resolve report crosses the Tauri IPC boundary directly (the report IS the §B.3 UI contract) rather than re-mirroring into a DTO."
  - "resolve_collection / download_collection adapters take the already-fetched collection.json manifest JSON; the live GraphQL collectionRevision.downloadLink archive fetch is the remaining network seam, exercised under human UAT (no client method added this plan)."
  - "Bulk download uses futures buffer_unordered (concurrency 3) borrowing &state/&window; the shared governor limiter is the true global rate gate (WR-03), the semaphore-equivalent only caps in-flight sockets."
  - "uninstall_collection purges to pristine FIRST, then clears the active flag (since delete_profile rejects an active profile, CR-02), then delete_profile, then removes staged trees + V5 rows — the ordering CONTRACT regression-locked by collection_round_trip."

patterns-established:
  - "Anti-Pattern-4 thin adapters: collections.rs only locks AppState + calls headless nexus/fomod/deploy/store + boundary_err + reuses run_download_to_window."
  - "Off-Nexus (direct/browse/manual) sources are partitioned to manual-steps and NEVER requested (T-04-12); a per-mod download failure is collected, the batch continues (Pitfall 4)."

requirements-completed: [COLL-02, COLL-03, COLL-04, COLL-05]

# Metrics
duration: 16min
completed: 2026-06-21
status: complete
---

# Phase 04 Plan 04: Collection Lifecycle Summary

**The full Collection lifecycle end-to-end out of EXISTING primitives — headless FOMOD-choice replay + rule→rank mapping, Premium-gated bulk download reusing `run_download_to_window`, deploy via `switch_profile`, and a byte-for-byte reversible uninstall regression-locked by a no-network blake3 round-trip test.**

## Performance

- **Duration:** 16 min
- **Started:** 2026-06-21T18:11:01Z
- **Completed:** 2026-06-21T18:27:02Z
- **Tasks:** 3 auto + 1 auto-approved human-verify checkpoint
- **Files modified:** 13 (3 created, 10 modified)

## Accomplishments
- **Headless FOMOD replay (COLL-03):** `nexus::replay_choices` converts a Collection's `IChoices` manifest into the SAME `fomod::Selection` the interactive wizard builds, by name-matching every step→group→option against the parsed `FomodModule` and accumulating each chosen option's `conditionFlags`. A stale name returns the new `NexusError::Replay` — never a silent mis-install.
- **Rule→rank mapping (COLL-03):** `nexus::map_rules_to_ranks` translates `modRules[]` (`after`⇒+1 rank, `before`⇒−1, `conflicts` recorded) + per-mod `fileOverrides` (force-win) onto the existing Phase-2 rank model — no new rules engine; an unresolved rule endpoint is skipped, not fatal.
- **Lifecycle adapters (COLL-02/03/04/05):** thin `resolve_collection` / `download_collection` / `deploy_collection` / `uninstall_collection` adapters orchestrating EXISTING primitives only — Premium gate first, bulk download reusing `run_download_to_window` under bounded concurrency with the shared governor, FOMOD replay per mod, `switch_profile` deploy, and `purge`→`delete_profile`→remove-staged uninstall.
- **Pristine round-trip test (COLL-05):** `crates/deploy/tests/collection_round_trip.rs` proves install→deploy→uninstall leaves the game byte-for-byte vanilla via the testkit blake3 `DIR_SENTINEL` harness, with NO network — the core reversibility guarantee regression-locked in CI.
- **Collections UI (UI-SPEC §B/§C):** the free-account Premium notice, the resolve-report HARD GATE before any download, per-mod + overall bulk progress (reusing the `download://progress` stream), per-mod Retry, the persistent manual-steps panel, Accent Deploy, and Destructive-red Uninstall behind a confirm modal — copy verbatim from the Copywriting Contract.

## Task Commits

1. **Task 1: Headless FOMOD choice replay (IChoices → Selection) + rule→rank** — `97516b1` (feat, TDD)
2. **Task 2: Collection adapters + pristine round-trip test** — `a3b501b` (feat)
3. **Task 3: Collections UI — resolve report, bulk progress, deploy/uninstall** — `3eeffac` (feat)
4. **Task 4: Human-verify checkpoint** — AUTO-APPROVED (see "Deferred to human UAT")

## Files Created/Modified
- `crates/nexus/src/replay.rs` (created) — `replay_choices`, `map_rules_to_ranks`, `RankAdjustment`, `is_auto_fetchable` + 6 inline tests
- `crates/nexus/src/error.rs` — new `NexusError::Replay` variant (stale-pin, distinct from a parse error)
- `crates/nexus/src/resolve.rs` — `ResolveReport`/`ResolvedMod`/`ModStatus` now derive `Serialize` (IPC contract)
- `crates/nexus/src/lib.rs`, `crates/nexus/Cargo.toml` — `replay` module re-export + `fomod` dep (stays headless)
- `src-tauri/src/commands/collections.rs` (created) — the four thin lifecycle adapters + premium-gate/off-Nexus unit tests
- `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` — module registration + 4 commands in `generate_handler!`
- `src-tauri/Cargo.toml` — `futures-util` for the bounded-concurrency bulk download
- `crates/deploy/tests/collection_round_trip.rs` (created) — COLL-04 deploy + COLL-05 install→uninstall pristine
- `frontend/src/lib/api.ts` — typed Collection wrappers + report mirrors
- `frontend/src/routes/+page.svelte` — the Collections §B/§C surface + uninstall confirm modal + scoped CSS

## Decisions Made
See frontmatter `key-decisions`. The load-bearing ones: a distinct `NexusError::Replay` for stale pins (never mis-install); `map_rules_to_ranks` as a `key_for`-parameterized pure delta function; deriving `Serialize` on the resolve report so it IS the §B.3 UI contract; and the uninstall ordering (purge → clear-active → delete_profile → remove staged) regression-locked by the round-trip test.

## Deviations from Plan

None — plan executed as written. Three Claude's-discretion refinements within plan scope:
- **`NexusError::Replay` added** — the plan called for "a specific Err" on a stale choice; a dedicated variant (vs. overloading `Http`) lets the UI key the "run the installer manually" hint precisely.
- **`map_rules_to_ranks` returns `RankAdjustment` deltas via a `key_for` closure** — the plan named the `after/before/conflicts/fileOverrides` semantics; modeling them as additive deltas keyed by a caller-supplied identity keeps the helper pure and unit-testable without committing to one mod-identity scheme.
- **`Serialize` on the nexus resolve types** — required for the report to cross the Tauri IPC boundary (the plan's §B.3 contract); chosen over re-mirroring into a shell DTO to keep one source of truth.

## Issues Encountered
- **`buffer_unordered` lifetime over-constraint:** the first cut borrowed `&CollectionMod` into the bounded-concurrency futures, which the async Tauri command rejected (`FnOnce is not general enough`). Resolved by carrying owned `(idx, name, mod_id, file_id)` coordinates into the stream and re-looking-up the manifest mod by index for persistence/replay. No behavior change.
- **Clippy `-D warnings`:** collapsed two nested `if let` chains (let-chains, Rust 1.96), used `?` for the premium gate, dropped a redundant `.into_iter()`, and rewrote the source-partition `if/else if` as a `match` on `SourceType`. All cosmetic.

## Deferred to human UAT

**The live end-to-end on a real Premium account remains a manual UAT item** (relates to the NEXUS-01 live-account gate deferred since Phase 3). This plan does NOT claim a human ran a live Premium Collection. Specifically deferred:
- Real Premium login → real `collectionRevision.downloadLink` archive fetch → real `collection.json` → bulk download → deploy → in-game launch → uninstall.
- The GraphQL collection-archive fetch (which yields the `collection.json` the adapters parse) — no client method was added this plan; the adapters take the already-fetched manifest JSON, leaving the network fetch as the UAT seam.
- The free-account live check (confirming the Premium notice blocks a real free session in-app).

**What IS proven headlessly (no network), regression-locked in CI:**
- Replay mapping + stale-pin error: `cargo test -p nextwist-nexus replay` (6 tests).
- Premium gate + off-Nexus-never-fetched: `commands::collections::tests` (2 tests).
- Resolve gate / off-Nexus classification (zero downloads): the Plan-03 `collection_mock` suite.
- Deploy via `switch_profile` + install→uninstall **byte-for-byte pristine**: `cargo test -p nextwist-deploy --test collection_round_trip` (testkit blake3).

## Verification

| Gate | Result |
|------|--------|
| `cargo test -p nextwist-nexus replay` | 6 passed |
| `cargo test -p nextwist-deploy --test collection_round_trip` | 1 passed (pristine) |
| `cargo test -p nextwist --locked` (incl. premium-gate + off-Nexus) | 15 passed |
| `cargo test --workspace --locked` | all 42 test binaries ok |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo deny check advisories bans licenses sources` | advisories/bans/licenses/sources ok (no new package — T-04-SC) |
| `npm --prefix frontend run check` | 142 files, 0 errors, 0 warnings |

## Threat Model Adherence
- **T-04-12 (off-Nexus auto-fetch / SSRF):** off-Nexus sources are partitioned to `manual_steps` from `source.type` alone; the download loop iterates only the `nexus` available set — off-Nexus URLs are never requested.
- **T-04-13 (zip-slip/symlink payloads):** every mod archive routes through `run_download_to_window` → `extract::install_archive` (defenses unchanged); no new file primitive.
- **T-04-14 (non-pristine uninstall):** deploy = journaled `switch_profile`, uninstall = `purge`; `collection_round_trip` asserts byte-for-byte pristine.
- **T-04-16 (Premium gate bypass):** `premium_gate(is_premium)` checked before any download; non-Premium returns the notice and starts nothing (no `nxm://` fallback).
- **T-04-SC (package installs):** the only new dep is `futures-util` (already a workspace dep, audited); `cargo deny` green.

## Known Stubs
None. The off-Nexus `bundle` archive-extract path and the live GraphQL collection-archive fetch are the documented human-UAT network seams (see "Deferred to human UAT"), not stubs — every headlessly-provable path is implemented and tested.

## Next Phase Readiness
- The full Collection lifecycle is implemented and headlessly regression-locked; the remaining work is live-account UAT (Premium fetch→deploy→launch→uninstall).
- COLL-02..05 satisfied; COLL-01 (browse/select) is wired in the UI (slug/revision/manifest input) and gated by the resolve report.

## Self-Check: PASSED
- FOUND: crates/nexus/src/replay.rs
- FOUND: src-tauri/src/commands/collections.rs
- FOUND: crates/deploy/tests/collection_round_trip.rs
- FOUND commit: 97516b1 (Task 1)
- FOUND commit: a3b501b (Task 2)
- FOUND commit: 3eeffac (Task 3)

---
*Phase: 04-guided-installers-collections*
*Completed: 2026-06-21*
