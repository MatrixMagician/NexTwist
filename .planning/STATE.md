---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
current_phase: 1
current_phase_name: Safe Local Round-Trip
status: executing
stopped_at: "Completed 01-03-PLAN.md (crates/extract: safe archive extraction)"
last_updated: "2026-06-20T20:01:46.976Z"
last_activity: 2026-06-20
last_activity_desc: Completed Plan 01-02 (crates/steam)
progress:
  total_phases: 5
  completed_phases: 0
  total_plans: 6
  completed_plans: 5
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-06-20)

**Core value:** Mods install and uninstall safely — non-destructive, fully reversible, conflict-aware deployment into Proton/Wine games on Linux.
**Current focus:** Phase 1 — Safe Local Round-Trip

## Current Position

Phase: 1 (Safe Local Round-Trip) — EXECUTING
Plan: 6 of 6
Status: Plan 01-02 complete; ready to execute next plan
Last activity: 2026-06-20 — Completed Plan 01-02 (crates/steam)

Progress: [███░░░░░░░] 33%

## Performance Metrics

**Velocity:**

- Total plans completed: 0
- Average duration: — min
- Total execution time: 0.0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

**Recent Trend:**

- Last 5 plans: —
- Trend: —

*Updated after each plan completion*
| Phase 01 P01 | 20 | 3 tasks | 18 files |
| Phase 01 P02 | 18 | 2 tasks | 8 files |
| Phase 01 P03 | 35 | 2 tasks | 10 files |
| Phase 01 P04 | 15 | 3 tasks | 17 files |
| Phase 01 P05 | 12 | 2 tasks | 8 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Roadmap]: Vertical-MVP structure — each phase delivers an end-to-end usable capability, not a horizontal technical layer.
- [Roadmap]: Safety-first, networking-last — the reversible deployment engine (staging → manifest → deploy → purge-to-pristine) is built and proven on LOCAL archives before any NexusMods networking (Phase 1 before Phase 3).
- [Roadmap]: Vortex model, not MO2 — real hardlink/symlink/reflink deployment + manifest; USVFS is Windows-only and rejected on Linux.
- [Phase ?]: Pinned rusqlite 0.39 + refinery 0.9.2 to resolve libsqlite3-sys links conflict; rusqlite-bundled+refinery decision preserved
- [Phase 1 P02]: steam crate aliases the shared-types dep as `nextwist_core` (not `core`) to avoid shadowing Rust's built-in `core` in the local thiserror derive.
- [Phase 1 P02]: Proton prefix derived manually as `compatdata/<appid>/pfx` (steamlocate has no compatdata API); honor `$STEAM_COMPAT_DATA_PATH`; re-resolve each session.
- [Phase 1 P02]: Snap Steam root not auto-detected (RESEARCH A2 low confidence); Snap users use the `add_game_by_folder` manual fallback.
- [Phase ?]: extract: validate the RAW archive entry name (not enclosed_name, which relativizes absolute entries) so absolute-path entries are explicitly rejected
- [Phase ?]: extract: single shared validate_entry is the only zip-slip/symlink path; zip/7z/system-rar all route through it; rar tool output is re-validated post-extraction
- [Phase ?]: Deploy reflink verdict is empirical on Linux (check_reflink_support is Windows-only/Unknown); probe runs a throwaway reflink + throwaway hard_link to catch btrfs-subvolume EXDEV
- [Phase ?]: Crash-recovery rolls a pending deploy forward when staging is at staging_dir/<rel>, else rolls back to pristine; journal does not persist the staging root (Phase-1)
- [Phase ?]: DEPLOY-08 casefold normalizes only directory components vs the steam CasingMap; leaf filenames and game-absent mod dirs preserved; normalized relpath recorded in manifest to keep purge pristine
- [Phase ?]: DEPLOY-07 verify is read-only; repair touches only manifest-recorded paths; orphans reported never deleted; recover_on_launch auto-runs verify after replay

### Pending Todos

[From .planning/todos/pending/ — ideas captured during sessions]

None yet.

### Blockers/Concerns

[Issues that affect future work]

- [Phase 1]: Safety-critical engine (crash-safe journaling, EXDEV probe, vanilla-backup, casefold) flagged for deeper research at plan time — see research/SUMMARY.md Research Flags.
- [Phase 3]: NexusMods API in flux (v1 REST → GraphQL v2); verify per-endpoint and confirm free-user nxm:// flow with a real non-Premium account. Register app under Nexus Acceptable Use Policy early.
- [Phase 4]: FOMOD ModuleConfig.xml conditional logic + Collections manifest/bundle/patch format is the largest single feature with known recurring bugs — dry-run resolve before touching disk.

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-06-20T20:01:46.963Z
Stopped at: Completed 01-03-PLAN.md (crates/extract: safe archive extraction)
Resume file: None
