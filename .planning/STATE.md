---
gsd_state_version: 1.0
milestone: v1.1
milestone_name: Starfield Support
current_phase: 8
current_phase_name: Reversible StarfieldCustom.ini Activation
status: executing
stopped_at: Completed 08-01-PLAN.md
last_updated: "2026-07-08T09:32:11.628Z"
last_activity: 2026-07-08
last_activity_desc: Phase 8 execution started
progress:
  total_phases: 4
  completed_phases: 2
  total_plans: 9
  completed_plans: 7
  percent: 50
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-06-23 after v1.0)

**Core value:** Mods install and uninstall safely — non-destructive, fully reversible, conflict-aware deployment into Proton/Wine games on Linux. This guarantee must extend verbatim to Starfield and to the new `StarfieldCustom.ini` write target.
**Current focus:** Phase 8 — Reversible StarfieldCustom.ini Activation

## Current Position

Phase: 8 (Reversible StarfieldCustom.ini Activation) — EXECUTING
Plan: 2 of 3
Status: Ready to execute
Last activity: 2026-07-08 — Phase 8 execution started

## Accumulated Context

### Decisions

Full decision log lives in PROJECT.md (Key Decisions) and the archived `milestones/v1.0-ROADMAP.md`. Foundational decisions carried into future milestones:

- Vertical-MVP roadmap structure; safety-first, networking-last.
- Vortex model (real reflink/hardlink/symlink deployment + manifest), not MO2 USVFS.
- Headless `crates/*` engine with **zero Tauri deps**; `src-tauri/` is a thin adapter. Honor this boundary.
- Crash-safety = intent-before-act operation journal + idempotent file ops (not WAL alone).
- TLS is rustls-only; `cargo-deny` bans the non-free UnRAR source (RAR shells out).

**v1.1 roadmap decisions (2026-07-07):**

- **4-phase shape, continuing at Phase 6** (v1.0 ended at Phase 5). Coverage: SFDET→P6, SFLO→P7, SFINI→P8, SFVER→P9 (14/14 mapped).
- **Starfield is a data-and-mapping extension, not a re-architecture** — no `core` change, no DB migration, no dependency/MSRV bump (libloot 0.29.5 already exposes `GameType::Starfield`, esplugin 6.1.4 `GameId::Starfield`). Only new crate is a small INI editor (`rust-ini`, optional).
- **Reversible-INI work isolated in Phase 8** (highest-risk new safety surface): new `crates/deploy/src/gameconfig.rs` reusing `backup.rs` + journal verbatim, wired at one engine choke point; carries its own SECURITY.md + a testkit reversibility suite. Purge must restore **absence** (provenance: pre-existing vs created-by-NexTwist).
- **Phase 9 is an on-hardware validation gate**, not a code phase — closes the MEDIUM-confidence CE2 INI-key / loose-file / masterlist assumptions on the owner's live Proton install (`deployed OK ≠ loaded in-game`).
- [Phase ?]: 08-01: three-valued INI provenance in vanilla_backup (no row / ABSENCE_MARKER / real hash) so restore never deletes a user's untouched StarfieldCustom.ini (T-08-05); added store::remove_vanilla, no migration

### Deferred Items

Items acknowledged and deferred at the v1.0 milestone close on 2026-06-23:

| Category | Item | Status | Notes |
|----------|------|--------|-------|
| verification | Phase 04 — `04-VERIFICATION.md` | human_needed (accepted) | Live Premium Collection end-to-end unverifiable: NexusMods restricts Collection-archive download to its own Vortex client. |
| uat | Phase 04 — `04-UAT.md` | partial (accepted) | FOMOD wizard PASSED; live Collection download BLOCKED by the same external Nexus policy. Documented `known_limitation`. |

**Non-blocking follow-ups (carry to v2/next):**

- Nexus-policy-compliant Collection ingest / manifest-import path (the engine already works on an already-fetched manifest).
- Profile-management UI + confirmation modal (accidental-loss protections are already enforced in the headless engine).
- Optional: visible mod-content in-game re-test now that the install-archive double-nesting bug is fixed (commit 2fa9821). *(Now partially subsumed by v1.1 Phase 9's on-hardware in-game verification for Starfield.)*
- Optional: `/gsd-secure-phase 3` to add a standalone SECURITY.md for the NexusMods auth/download phase (its boundaries are already verified inline in 03-VERIFICATION + the milestone integration check; this is artifact parity, not a security gap).

### Blockers/Concerns

None blocking. v1.1-relevant watch items:

- **CE2 loose-file specifics are MEDIUM-confidence** (exact `StarfieldCustom.ini` keys, loose-file loading behavior, masterlist currency) — Phase 9 exists precisely to close these on real hardware before locking the merge logic.
- Starfield's loose-file mechanism has regressed across patches; treat the INI recipe as validated-against-a-build, not a constant (version-drift warning in Phase 6).
- NexusMods API remains in flux (v1 REST → GraphQL v2); the Vortex-only Collection-download restriction still shapes any future Collection work.
- Live OAuth2 round-trip (NEXUS-01) still gated on registering a public OAuth `client_id` (API-key path is the works-today login).

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 260623-m42 | Fix release.yml AppImage build (--locked), bump version to 1.0.0, add changelog to GitHub Release | 2026-06-23 | fce5c6f | [260623-m42-fix-release-yml-appimage-build-locked-fl](./quick/260623-m42-fix-release-yml-appimage-build-locked-fl/) |

## Operator Next Steps

- Plan the first Starfield phase with `/gsd-plan-phase 6` (Starfield Detection & CE2 Path Resolution).
- Phase 8 (reversible INI) is the highest-risk surface — plan it with `/gsd-plan-phase --research-phase 8` and expect its own SECURITY.md.

## Performance Metrics

| Phase | Plan | Duration | Notes |
|-------|------|----------|-------|
| Phase 08 P01 | 40min | 2 tasks | 5 files |

## Session

**Last session:** 2026-07-08T09:32:02.251Z
**Stopped at:** Completed 08-01-PLAN.md
**Resume file:** None
