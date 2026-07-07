---
phase: 06-starfield-detection-ce2-path-resolution
plan: 01
subsystem: detection
tags: [starfield, steam, proton, path-resolution, case-folding, wine, buildid]

# Dependency graph
requires:
  - phase: 05 (v1.0 engine)
    provides: steam::resolve allow-list + AppManifest ACF seam, entry_ci case-fold, testkit fake_proton_prefix
provides:
  - Starfield (AppID 1716740) fully allow-listed in steam::resolve (SFDET-01)
  - steam::ce2 CE2 My Games resolver + Ce2ConfigState first-launch typed state (SFDET-02)
  - steam::installed_build + ce2::drift_notice advisory version-drift compare (SFDET-03)
  - steam::starfield_status serde-Serialize aggregate for the Tauri adapter (Plan 03)
  - testkit::fake_my_games_prefix Documents/My Games fixture builder
affects: [06-02 (plugins.txt AppData arm), 06-03 (Tauri adapter), 07 (load order), 08 (reversible INI), 09 (hardware validation / VALIDATED_BUILD baseline)]

# Tech tracking
tech-stack:
  added: []   # No new external crate; steam gains testkit as an internal dev-dep only
  patterns:
    - "Typed success enum (Ce2ConfigState) instead of an Err for first-launch — game stays addable"
    - "Reuse entry_ci (lifted to pub(crate)) for the My Games walk — case-fold never re-implemented"
    - "Dormant advisory constant (VALIDATED_BUILD=0) suppresses the drift notice until a later phase seeds it"

key-files:
  created:
    - crates/steam/src/ce2.rs
  modified:
    - crates/steam/src/resolve.rs
    - crates/steam/src/lib.rs
    - crates/steam/Cargo.toml
    - crates/testkit/src/lib.rs

key-decisions:
  - "CE2 config path resolved by mirroring loadorder::appdata_local_path's join-chain with a Documents/My Games/Starfield tail, walking each existing component through the shared entry_ci case-fold (WR-07 determinism preserved)."
  - "First-launch is a typed SUCCESS enum Ce2ConfigState::{Ready, FirstLaunchPending}, NOT a thiserror arm — diverges deliberately from loadorder::LoadOrderError::NoLocalAppData so add-game never aborts."
  - "VALIDATED_BUILD seeded 0 → drift_notice always returns None (notice dormant until Phase 9 sets the real baseline). Advisory, non-blocking, fail-safe — never gates management."
  - "user.reg Personal redirect is defensive-only with the default steamuser/Documents path as the load-bearing fallback; mapped path is traversal-guarded to stay under <prefix>/drive_c."

patterns-established:
  - "Mirror-const / allow-list arm: Starfield behaves as just another allow-listed AppID across every match appid site."
  - "TDD RED→GREEN split: failing tests + stubbed fns committed first, real bodies second."

requirements-completed: [SFDET-01, SFDET-02, SFDET-03]

coverage:
  - id: D1
    description: "Starfield (1716740) allow-listed — is_supported/default_name/expected_exe accept it and resolve_from_root resolves install dir + Proton prefix (SFDET-01)."
    requirement: "SFDET-01"
    verification:
      - kind: unit
        ref: "crates/steam/src/resolve.rs#is_supported_allow_lists_the_three_supported_games, #resolve_from_root_resolves_starfield_positively"
        status: pass
    human_judgment: false
  - id: D2
    description: "resolve_ce2_config returns Ready/FirstLaunchPending, case-folded and redirection-aware; mandatory case-mismatch fixture resolves the real on-disk casing (SFDET-02)."
    requirement: "SFDET-02"
    verification:
      - kind: unit
        ref: "crates/steam/src/ce2.rs#my_games_case_mismatch_resolves_real_on_disk_casing, #first_launch_pending_when_absent_or_empty, #ready_when_config_dir_has_a_file, #documents_redirect_is_honored_when_in_prefix, #documents_redirect_traversal_is_rejected"
        status: pass
    human_judgment: false
  - id: D3
    description: "installed_build reads ACF buildid (fail-safe None on any error); drift_notice advisory and suppressed at VALIDATED_BUILD=0 (SFDET-03)."
    requirement: "SFDET-03"
    verification:
      - kind: unit
        ref: "crates/steam/src/resolve.rs#installed_build_reads_buildid_and_is_fail_safe, crates/steam/src/ce2.rs#drift_notice_only_fires_when_installed_newer_than_nonzero_validated"
        status: pass
    human_judgment: false
---

# Phase 6 Plan 01: Starfield Detection & CE2 Path Resolution Summary

Made NexTwist's headless `steam` engine Starfield-aware: allow-listed AppID 1716740 across the existing Bethesda detection flow, added the CE2 `Documents/My Games/Starfield` resolver with a typed first-launch state and Wine case-folding/redirection handling, and added a fail-safe installed-build read with a dormant advisory version-drift compare — all read-only, zero writes to any real prefix.

## Accomplishments

- **SFDET-01 — Starfield allow-listed.** `pub const STARFIELD = 1716740` joins `SUPPORTED_APPIDS`; `default_name`→"Starfield", `expected_exe`→"Starfield.exe". `resolve_from_root` now positively resolves 1716740's install dir + Proton prefix (not just the not-installed path).
- **SFDET-02 — CE2 My Games resolver.** New `steam::ce2` module: `my_games_path(prefix)` mirrors `appdata_local_path`'s join-chain with the `Documents/My Games/Starfield` tail, walking each existing component through the shared `entry_ci` case-fold. `resolve_ce2_config` returns the typed `Ce2ConfigState::{Ready, FirstLaunchPending}`. The mandatory case-mismatched-prefix fixture test passes.
- **SFDET-03 — installed build + drift.** `installed_build(library_root, appid)` reads `buildid` from `appmanifest_<appid>.acf` (fail-safe → `None` on any error); `drift_notice(installed, validated)` returns `Some` only when `installed > validated > 0` — dormant while `VALIDATED_BUILD == 0`.
- **`starfield_status` aggregate** (serde-`Serialize`) combines all three for the Plan 03 Tauri adapter to forward verbatim.
- **Testkit fixture** `fake_my_games_prefix` + `MyGamesOpts` seeds the canonical, mis-cased, marker, and `user.reg`-redirect prefix shapes the resolver tests assert against.

## Key Implementation Details

- **`entry_ci` reuse.** Lifted from private `fn` to `pub(crate) fn` in `resolve.rs`; body unchanged. `ce2::resolve_cased` calls it per existing component so the deterministic exact-case-wins-else-smallest choice (WR-07) is shared, not duplicated.
- **Redirection + traversal guard (T-06-01).** `user.reg` `"Personal"` is parsed leniently (un-escaping Wine's doubled backslashes), mapped from `C:\...` to in-prefix components rooted at `drive_c`, and rejected (→ default fallback) if the drive isn't `C:` or any segment is `.`/`..`/empty. The default `.../steamuser/Documents` path is the load-bearing case.
- **Symlink posture (T-06-02).** The walk resolves components against `read_dir` entries via `entry_ci` only — never follows a symlink out of the prefix.
- **Fail-safe parsing (T-06-03).** Both `installed_build` and the `user.reg` read return `None`/default on any absence or parse error; neither panics.

## Deviations from Plan

None — plan executed as written. (`steam` gained `testkit` as an internal workspace dev-dependency, which is how the plan intends the Task 1 fixtures to reach the Task 3 resolver tests; no new external crate, root `[workspace.dependencies]` untouched, no MSRV bump, no DB migration, no `core` change.)

## Threat Mitigations Applied

- **T-06-01 (Tampering/EoP):** `user.reg` `"Personal"` mapped into the prefix then verified to stay under `<prefix>/drive_c`; `..`/out-of-prefix → default fallback. Test: `documents_redirect_traversal_is_rejected`.
- **T-06-02 (Tampering):** `entry_ci`-only component resolution, no symlink follow-through.
- **T-06-03 (DoS):** lenient parse; `installed_build` + redirect read never panic. Tests: `installed_build_reads_buildid_and_is_fail_safe`, `malformed_user_reg_falls_back_to_default`.

## Verification

- `cargo test -p nextwist-steam -p nextwist-testkit --locked` — green (steam 16 lib + 3 integration + 9 ce2; testkit 11).
- `cargo clippy -p nextwist-steam -p nextwist-testkit --all-targets --locked -- -D warnings` — clean.
- Root `Cargo.toml [workspace.dependencies]` untouched (no new external dep, no MSRV bump).

## Notes for Later Phases

- **Phase 9** must set `VALIDATED_BUILD` (currently `0`) to the real validated Starfield build to activate the drift notice. Until then verify-work must NOT expect a user-visible drift warning — the notice is intentionally dormant.
- **Phase 6 Plan 02** owns the `AppData/Local/Starfield` `plugins.txt` arm (the other half of the two-path split).
- **Phase 6 Plan 03** consumes `starfield_status` verbatim through a thin Tauri adapter.

## Commits

- `d852da9` test(06-01): testkit My Games CE2 fixture builder
- `9c4ff52` feat(06-01): allow-list Starfield (1716740) + buildid read in steam resolve
- `3c5804f` test(06-01): failing ce2 tests (RED)
- `f03de1b` feat(06-01): implement ce2 resolver, first-launch state + drift (GREEN)

## Self-Check: PASSED

- Files verified on disk: `crates/steam/src/ce2.rs`, `06-01-SUMMARY.md`.
- Commits verified in git log: `d852da9`, `9c4ff52`, `3c5804f`, `f03de1b`.
