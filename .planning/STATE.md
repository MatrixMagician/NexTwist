---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
current_phase: 05
current_phase_name: AppImage Distribution
status: executing
stopped_at: Phase 2 (Multi-Mod Management) BUILT + auto-verified (21/21 must-haves, code review 2 BLOCKERs fixed, 142 tests green); autonomous run STOPPED at user request. Phase 2 awaiting 4 manual/in-game UAT items (02-UAT.md).
last_updated: "2026-06-22T13:26:21.626Z"
last_activity: 2026-06-22
last_activity_desc: Phase 05 execution started
progress:
  total_phases: 5
  completed_phases: 4
  total_plans: 21
  completed_plans: 20
  percent: 80
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-06-20)

**Core value:** Mods install and uninstall safely — non-destructive, fully reversible, conflict-aware deployment into Proton/Wine games on Linux.
**Current focus:** Phase 05 — AppImage Distribution

## Current Position

Phase: 05 (AppImage Distribution) — EXECUTING
Plan: 2 of 2
Status: Ready to execute
Last activity: 2026-06-22 — Phase 05 execution started

Progress: [██░░░░░░░░] 20% (1 of 5 phases built; Phase 1 pending final manual UAT sign-off)

## Performance Metrics

**Velocity:**

- Total plans completed: 3
- Average duration: — min
- Total execution time: 0.0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 3 | 3 | - | - |

**Recent Trend:**

- Last 5 plans: —
- Trend: —

*Updated after each plan completion*
| Phase 01 P01 | 20 | 3 tasks | 18 files |
| Phase 01 P02 | 18 | 2 tasks | 8 files |
| Phase 01 P03 | 35 | 2 tasks | 10 files |
| Phase 01 P04 | 15 | 3 tasks | 17 files |
| Phase 01 P05 | 12 | 2 tasks | 8 files |
| Phase 01 P06 | 35 | 2 tasks | 23 files |
| Phase 01 P07 | 25 | 3 tasks | 5 files |
| Phase 02 P01 | 7 | 3 tasks | 11 files |
| Phase 02 P02 | 22 | 3 tasks | 9 files |
| Phase 02 P03 | 27 | 3 tasks | 10 files |
| Phase 02 P04 | 15 | 3 tasks | 18 files |
| Phase 02 P05 | 18 | 2 tasks | 11 files |
| Phase 03 P01 | 20min | 3 tasks | 19 files |
| Phase 03 P02 | 40min | 3 tasks | 21 files |
| Phase 03 P03 | ~9min | 3 tasks | 9 files |
| Phase 04 P01 | 12 | 3 tasks | 13 files |
| Phase 04 P04-03 | 7 | 2 tasks | 12 files |
| Phase 04 P04-02 | 35m | 3 tasks | 6 files |
| Phase 04 P04 | 16 | 3 tasks | 13 files |
| Phase 05 P01 | 15 | 2 tasks | 7 files |

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
- [Phase ?]: GAP-01 fix: empty-dir cleanup is manifest/journal-derived (never a disk scan); bottom-up remove_dir bounded strictly below the deploy root protects vanilla dirs.
- [Phase ?]: testkit snapshots directory shape via a reserved non-hex DIR_SENTINEL so the pristine assertion catches orphan empty dirs, not just file content.
- [Phase ?]: verify/repair (DEPLOY-07) classify+remove orphan EMPTY dirs to a fixed point; file orphans stay strictly report-only (T-01-16 preserved).
- [Phase ?]: [Phase 2 P01]: V2 migration additive-only (CREATE + one Default-profile INSERT); Phase-1 deployed_file membership NOT folded into managed_mod — live deployment stays on disk + reversible, Default profile starts empty (D-16).
- [Phase ?]: [Phase 2 P01]: migration test reaches a genuine V1-only state via refinery Target::Version(1); store upserts use ON CONFLICT DO UPDATE keyed by UNIQUE constraints; MSRV 1.85->1.89 for libloot, cargo-deny allows the libloot GPL-3.0 family (Phase-5 DIST-02).
- [Phase ?]: libloot 0.29.5: set_load_order persists internally (no Game::save); no active-plugin setter — active state enters via Plugins.txt, generated from DB plugin_state in Plan 04
- [Phase ?]: deny.toml allows bare GPL-3.0 (libloadorder/esplugin declare it, not -or-later); compatible in the GPL-3.0-or-later AppImage
- [Phase ?]: Conflict multi-root contract = Option A: new WinnerFile + deploy_winners; StagedFiles/deploy unchanged (02-03)
- [Phase ?]: Conflict resolver = pure fold emitting one winner per target_rel (UNIQUE-safe); reused by Plan 02-05 profile switch deploy half (02-03)
- [Phase 02]: Plan 02-05: profile switch wiring = deploy->loadorder direct call (apply_load_order inside switch_profile); acyclic, loadorder depends only on store+core
- [Phase 02]: Plan 02-05: SwitchReport = {purged, deployed, plugins_txt}; switch = purge(old)->deploy_winners(new)->apply_load_order->set_active, never diff-deploy (D-15)
- [Phase ?]: oauth2 5.0 binds to reqwest 0.12; workspace on 0.13 — token POST issued via the workspace reqwest 0.13 client (oauth2 still owns S256 PKCE + CSRF); added reqwest form feature
- [Phase ?]: Chose webbrowser over tauri-plugin-opener for the OAuth browser launch (no plugin/capability wiring; deferred to Plan 03)
- [Phase ?]: API-key paste is the works-today login (NEXUS-01) until an OAuth client_id is registered; OAuth code path implemented + mockito-verified, live round-trip deferred
- [Phase 03]: Premium-download client uses explicit status branching (429->RateLimited, keyed-4xx->Redeem, else Http) so the UI distinguishes 'link expired' from 'download failed'
- [Phase 03]: NexusClient::download keeps reqwest out of src-tauri — the headless crate owns all HTTP and the rustls/redirect policy
- [Phase 03]: V4 nexus_source migration is strictly additive (FK CASCADE on managed_mod); the downloaded mod is an ordinary ManagedMod indistinguishable from a local-archive mod
- [Phase ?]: Plan 03-03: nxm:// parser is a dependency-free hand-split (not the url crate); strict fixed-grammar validation keeps crates/nexus minimal and cargo-deny clean.
- [Phase ?]: Plan 03-03: single-instance plugin registered BEFORE deep-link (load-bearing); free-user nxm:// redemption reuses the shared run_download_to_window core (no parallel download path).
- [Phase ?]: [Phase 04 P03]: collection.profile_id FK is ON DELETE SET NULL — Plan-04 profile teardown keeps the collection + resolve history, only NULLs the link.
- [Phase ?]: [Phase 04 P03]: resolve-before-download gate is download-free by construction — added client.file_availability (metadata-only v1 file-info: 404=>Unavailable, ARCHIVED=>Archived); off-Nexus sources classified Manual from source.type alone (no request; T-04-08 SSRF mitigation).
- [Phase ?]: FOMOD apply stages the whole validated archive via extract::install_archive — no new write primitive (T-04-05)
- [Phase ?]: FOMOD dry-run conflict classification is honest: a resolved plan is ConflictClass::None; a no-winner construct is surfaced via the engine Err with Install disabled
- [Phase ?]: Collections add ZERO new engine primitives: deploy=switch_profile, uninstall=purge->delete_profile, bulk download reuses run_download_to_window (04-04)
- [Phase ?]: replay_choices returns NexusError::Replay on a stale FOMOD choice (mod changed since pin) — never a silent mis-install (04-04)
- [Phase ?]: Premium gate enforced before any Collection download via UserInfo.is_premium; no nxm:// free-user fallback; live Premium round-trip deferred to human UAT (04-04)
- [Phase ?]: nxm_self_test is generic over the error type so the headless test drives all three Result arms without the plugin's non-public Error or a live OS session
- [Phase ?]: Phase 05 icons generated via ImageMagick (cargo tauri unavailable on host); no bundle.linux.appimage object (YAGNI); version 0.1.0 unchanged as the v0.1.0 tag source of truth

### Pending Todos

[From .planning/todos/pending/ — ideas captured during sessions]

None yet.

### Blockers/Concerns

[Issues that affect future work]

- [Phase 1]: Safety-critical engine (crash-safe journaling, EXDEV probe, vanilla-backup, casefold) flagged for deeper research at plan time — see research/SUMMARY.md Research Flags.
- [Phase 3]: NexusMods API in flux (v1 REST → GraphQL v2); verify per-endpoint and confirm free-user nxm:// flow with a real non-Premium account. Register app under Nexus Acceptable Use Policy early.
- [Phase 4]: FOMOD ModuleConfig.xml conditional logic + Collections manifest/bundle/patch format is the largest single feature with known recurring bugs — dry-run resolve before touching disk.
- Live NexusMods OAuth round-trip (NEXUS-01) gated on a registered OAuth client_id + nxm://oauth/callback redirect + real account — Task 4 human-verify deferred; API-key fallback unblocks Phase 3

## Deferred Items

Items acknowledged and carried forward from previous milestone close:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-06-22T13:25:37.154Z
Stopped at: Phase 2 (Multi-Mod Management) BUILT + auto-verified (21/21 must-haves, code review 2 BLOCKERs fixed, 142 tests green); autonomous run STOPPED at user request. Phase 2 awaiting 4 manual/in-game UAT items (02-UAT.md).
Resume file: .planning/phases/02-multi-mod-management/02-UAT.md
Resume command: `/gsd-autonomous --from 2` to continue the milestone (UAT-1/UAT-2 done + GAP-01 fixed). Optionally finish UAT-3/UAT-4 first.
Outstanding (non-blocking): TODO(A2) in crates/steam/src/discover.rs — convert to plain comment or tracked issue (intentional Snap-detect deferral, tested fallback exists).
