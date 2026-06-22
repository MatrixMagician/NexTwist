---
phase: 05-appimage-distribution
plan: 01
subsystem: infra
tags: [tauri, appimage, deep-link, nxm, icons, startup-hook]

# Dependency graph
requires:
  - phase: 03 (nexus-auth / nxm wiring)
    provides: "register_all() + on_open_url nxm:// deep-link wiring in src-tauri/src/lib.rs setup hook"
provides:
  - "Non-fatal nxm:// handler self-test wired after register_all() (DIST-01 'self-test passes')"
  - ">=128x128 icon set + bundle.icon pointing at it (clears the linuxdeploy 32x32 bundling blocker)"
  - "Headless test asserting the self-test path is non-fatal on all three Result arms"
affects: [05-02 (release pipeline + DIST-AUDIT), appimage-bundling]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Self-test via the plugin's own is_registered() (never a hand-rolled xdg-mime query) so the desktop-file name cannot drift"
    - "Extract a generic non-fatal helper from the setup closure so OS-shelling startup wiring is testable headlessly"

key-files:
  created:
    - src-tauri/tests/nxm_self_test.rs
    - src-tauri/icons/128x128.png
    - src-tauri/icons/128x128@2x.png
    - src-tauri/icons/32x32.png
  modified:
    - src-tauri/src/lib.rs
    - src-tauri/tauri.conf.json
    - src-tauri/icons/icon.png

key-decisions:
  - "nxm_self_test is generic over the error type (fn nxm_self_test<E: Display>(Result<bool, E>)) so the test exercises all three arms without constructing the plugin's non-public Error or needing a live OS session"
  - "Icon set generated from a 1024x1024 NexTwist mark via ImageMagick (cargo tauri unavailable on this host); icon.png set to 512x512, all 8-bit RGBA"
  - "No bundle.linux.appimage object added — default appimage bundling works (YAGNI, per 05-PATTERNS.md)"
  - "version 0.1.0 left untouched as the single source of truth for the v0.1.0 release tag"

patterns-established:
  - "Pattern: non-fatal warn-and-continue for OS-integration startup calls — every is_registered/register_all arm logs via tracing and returns (), never ?/unwrap/expect"
  - "Pattern: plugin-owned self-test — query is_registered() rather than reimplementing the xdg-mime lookup, eliminating desktop-file-name drift"

requirements-completed: [DIST-01]

# Metrics
duration: ~15min
completed: 2026-06-22
status: complete
---

# Phase 5 Plan 01: Buildable + Self-Testing AppImage Slice Summary

**Wired a strictly non-fatal `nxm://` handler self-test (plugin `is_registered("nxm")`) into the startup hook and regenerated a >=128x128 icon set so `cargo tauri build --bundles appimage` clears the linuxdeploy icon blocker — the first vertical slice of DIST-01.**

## Performance

- **Duration:** ~15 min
- **Completed:** 2026-06-22
- **Tasks:** 2
- **Files modified:** 7 (3 created, 1 regenerated, 3 modified)

## Accomplishments
- `nxm_self_test(Result<bool, _>)` helper logs PASS/WARN on all three arms and returns `()` — startup never aborts when `xdg-mime` is absent (locked warn-and-continue, T-05-02).
- Call site wired via `app.deep_link().is_registered("nxm")` immediately after `register_all()` and before `on_open_url`, inside the existing `#[cfg(any(windows, target_os = "linux"))]` block; uses the plugin's own method (no hand-rolled xdg-mime query → no desktop-file-name drift, T-05-01).
- Regenerated a real >=128x128 icon set (32x32, 128x128, 128x128@2x=256, icon.png=512, all 8-bit RGBA) from a 1024x1024 NexTwist mark; `bundle.icon` now lists the full set, removing the 32x32 linuxdeploy bundling blocker.
- Added `src-tauri/tests/nxm_self_test.rs` proving the self-test path is non-fatal on Ok(true)/Ok(false)/Err — green against `nextwist_lib`, needs no live OS or installed `xdg-mime`.

## Task Commits

Each task was committed atomically (Task 1 is TDD: RED → GREEN):

1. **Task 1 (RED): failing non-fatal self-test wiring test** - `021a73c` (test)
2. **Task 1 (GREEN): wire non-fatal nxm:// self-test into startup hook** - `2fe611d` (feat)
3. **Task 2: regenerate >=128x128 icon set, point bundle.icon at it** - `69a503b` (feat)

_No REFACTOR commit — the GREEN implementation was already minimal and clippy-clean._

## Files Created/Modified
- `src-tauri/tests/nxm_self_test.rs` (NEW) - headless test: self-test is non-fatal on all three `Result` arms.
- `src-tauri/src/lib.rs` (MODIFIED) - added `nxm_self_test` helper + call site after `register_all()`; updated the Phase-5 marker comment (single-instance-before-deep-link ordering comment preserved).
- `src-tauri/tauri.conf.json` (MODIFIED) - `bundle.icon` now lists `["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.png"]`; `version` unchanged.
- `src-tauri/icons/128x128.png`, `128x128@2x.png`, `32x32.png` (NEW) - regenerated icon set.
- `src-tauri/icons/icon.png` (REGEN) - now 512x512 (was 32x32).

## Decisions Made
- Made `nxm_self_test` generic over the error (`<E: Display>`) so the headless test drives the `Err` arm without the plugin's non-public `Error` and without a live OS session. The call site passes the plugin's `Result<bool>` directly.
- Used ImageMagick (`magick`) to generate icons because `cargo tauri` is not installed on this host (the plan explicitly permits this fallback). Font `Droid-Sans-Bold` used after `DejaVu-Sans-Bold` was unavailable.
- Did not add a `bundle.linux.appimage` object (YAGNI — default bundling works); left `version: "0.1.0"` as the single source of truth.

## Deviations from Plan

None - plan executed exactly as written. (No deviation rules triggered; the host-tooling icon fallback and the generic-helper extraction are both explicitly sanctioned by the plan's `<action>` text, not deviations.)

## Issues Encountered
- `cargo` was not on the agent shell `PATH` and the RTK proxy could not spawn it initially — resolved by prepending `$HOME/.cargo/bin` to `PATH` and routing through `rtk proxy cargo ...`.
- ImageMagick rejected `DejaVu-Sans-Bold`; switched to the available `Droid-Sans-Bold` so the glyph renders.

## TDD Gate Compliance
- RED (`021a73c`, `test(...)`) → GREEN (`2fe611d`, `feat(...)`) sequence present in git log. No REFACTOR needed. Gate satisfied.

## User Setup Required
None - no external service configuration required. (Real end-to-end `nxm://` registration on a built AppImage remains a manual UAT item, owned by Plan 02.)

## Next Phase Readiness
- The shell self-tests its `nxm://` handler on startup and the icon blocker is gone, so Plan 02's release pipeline can run `cargo tauri build --bundles appimage` and the DIST-AUDIT can package/verify the resulting AppImage.
- `cargo test --workspace --locked` is fully green (no engine regression); `cargo clippy -p nextwist --all-targets -- -D warnings` is clean.

## Self-Check: PASSED
All 7 created/modified files verified on disk; all 3 task commits (`021a73c`, `2fe611d`, `69a503b`) verified in git log.

---
*Phase: 05-appimage-distribution*
*Completed: 2026-06-22*
