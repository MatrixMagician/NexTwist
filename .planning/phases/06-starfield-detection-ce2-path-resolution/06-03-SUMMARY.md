---
phase: 06-starfield-detection-ce2-path-resolution
plan: 03
subsystem: tauri-adapter
tags: [starfield, tauri, ipc, svelte, first-launch, version-drift]

requires:
  - phase: 06-01 (steam)
    provides: steam::starfield_status / StarfieldStatus / Ce2ConfigState / DriftNotice, installed_build, resolve_game
  - phase: 06-02 (loadorder)
    provides: Starfield allow-list arms consumed by the existing plugin/load-order section
provides:
  - "steam::starfield_status_for(appid) — engine resolver forwarding the CE2 aggregate (all path construction in-engine)"
  - "commands::games::starfield_status Tauri command (thin forwarder, registered in generate_handler!)"
  - "api.starfieldStatus + StarfieldStatus/Ce2ConfigState/DriftNotice TS types"
  - "Starfield game-view: selectable in add/detect (SFDET-01), first-launch gate + Re-check (SFDET-02), persistent drift notice (SFDET-03)"
affects:
  - Phase 7 (load order — the section this plan gates on Ready)
  - Phase 8 (reversible INI — same first-launch gate)
  - Phase 9 (sets VALIDATED_BUILD → lights up the currently-dormant drift notice; on-hardware visual confirmation)

tech-stack:
  added: []
  patterns:
    - "Thin Tauri command forwards an engine aggregate verbatim; the engine owns ALL path construction (T-06-04)"
    - "Externally-tagged serde enum (Ce2ConfigState) surfaced in TS as a `{ Ready: string } | { FirstLaunchPending: string }` union, discriminated with an `in` check"
    - "First-launch state gates a whole UI section via {#if pending}guidance{:else}controls{/if} — Re-check re-invokes on explicit click only"

key-files:
  created: []
  modified:
    - crates/steam/src/resolve.rs
    - crates/steam/src/lib.rs
    - src-tauri/src/commands/games.rs
    - src-tauri/src/lib.rs
    - frontend/src/lib/api.ts
    - frontend/src/routes/+page.svelte

key-decisions:
  - "Added steam::starfield_status_for(appid) rather than deriving the Steam library root in the adapter, so the command constructs NO paths (honors the plan's engine-boundary hard rule + threat T-06-04). Library root = install_dir ancestor, computed in-engine with a runnable unit check."
  - "Ce2ConfigState surfaced to TS as an externally-tagged union matching serde's default newtype-variant representation; pending is detected with `\"FirstLaunchPending\" in ce2_state`."
  - "First-launch gate replaces the load-order/INI section body with guidance + Re-check (blocks management until Ready); the drift notice is a separate, always-non-blocking advisory block at the top of the game view."
  - "Drift notice is built but DORMANT this phase (VALIDATED_BUILD == 0 → engine returns drift: null); it lights up when Phase 9 seeds the baseline. Build numbers render as plain text (no {@html}, T-06-05)."

requirements-completed: [SFDET-01, SFDET-02, SFDET-03]

coverage:
  - id: D1
    description: "Thin starfield_status command forwards the engine aggregate and is registered; src-tauri compiles and the value serializes for IPC (SFDET-01/02/03 backend surface)."
    requirement: "SFDET-01"
    verification:
      - kind: build
        ref: "cargo build -p nextwist --locked"
        status: pass
      - kind: unit
        ref: "crates/steam/src/ce2.rs#starfield_status_aggregates_and_is_serializable"
        status: pass
      - kind: unit
        ref: "crates/steam/src/resolve.rs#library_root_is_install_dir_grandparent"
        status: pass
    human_judgment: false
  - id: D2
    description: "Starfield game view: FirstLaunchPending shows launch-once guidance + a Re-check action and disables load-order/INI management; Ready enables it. Game stays addable/detectable (SFDET-02)."
    requirement: "SFDET-02"
    verification:
      - kind: typecheck
        ref: "npm --prefix frontend run check (0 errors) — starfieldPending gates the section, Re-check calls starfieldStatus"
        status: pass
      - kind: build
        ref: "npm --prefix frontend run build"
        status: pass
    human_judgment: true
    rationale: "Live in-app confirmation on a real Proton install (the pending→Ready transition after launching Starfield once) is Phase 9's on-hardware gate; Phase 6's automated gate is svelte-check + build + the engine tests."
  - id: D3
    description: "Persistent, non-blocking installed-vs-validated drift notice shown ONLY when drift is present; never disables management (SFDET-03). Dormant this phase (VALIDATED_BUILD == 0)."
    requirement: "SFDET-03"
    verification:
      - kind: typecheck
        ref: "npm --prefix frontend run check — notice keyed on isStarfield && starfield?.drift, independent of the pending gate"
        status: pass
      - kind: unit
        ref: "crates/steam/src/ce2.rs#drift_notice_only_fires_when_installed_newer_than_nonzero_validated (drift suppressed at VALIDATED_BUILD=0)"
        status: pass
    human_judgment: true
    rationale: "The notice cannot fire until Phase 9 seeds a non-zero VALIDATED_BUILD, so its visible appearance is only confirmable on-hardware then; the surfacing + suppression logic are unit/type verified now."

metrics:
  duration: ~15 min
  completed: 2026-07-07

status: complete
---

# Phase 6 Plan 03: Starfield Tauri Adapter & Frontend Surfacing Summary

Surfaced Phase 6's headless Starfield detection to the user: a thin `starfield_status`
Tauri command that forwards the `steam::StarfieldStatus` aggregate verbatim, plus a Starfield
game-view treatment that renders first-launch guidance + a Re-check action (blocking
load-order/INI management until the CE2 config dir exists) and a persistent, non-blocking
installed-vs-validated version-drift notice. All detection/path/drift logic stayed in the
engine; the adapter and UI only present the typed state.

## Accomplishments

- **SFDET-01 — Starfield selectable end-to-end.** Added Starfield (1716740) to the frontend
  add-by-folder title list; detection already lists it via the Plan 01 allow-list. The whole
  add/manage flow treats it as just another supported Bethesda AppID.
- **SFDET-02 — first-launch gate (UI half).** When `Ce2ConfigState::FirstLaunchPending`, the
  "Plugins & load order" section renders "Launch Starfield once via Steam so it creates its
  config folder, then Re-check" (with the expected config path) and a Re-check button, and
  hides the management controls. `Ready` restores the normal plugin list/actions. The game is
  still fully addable/detectable while pending. Re-check re-invokes `starfieldStatus` on
  explicit click only — no write-on-focus, no surprise writes.
- **SFDET-03 — persistent drift notice.** A separate advisory block at the top of the Starfield
  game view shows "installed build N vs validated build M" only when `drift` is present. It is
  always non-blocking — it never disables management. Build numbers render as plain text.
- **Thin adapter + engine resolver.** `commands::games::starfield_status(appid)` forwards
  `steam::starfield_status_for(appid)`, registered in `generate_handler!`. The new engine
  helper re-resolves the Steam library + Proton prefix on every call (never cached) and keeps
  ALL path construction in `steam` (the library root is the install-dir ancestor, computed
  in-engine) so the command builds no paths (threat T-06-04).

## Key Implementation Details

- **`Ce2ConfigState` in TS.** serde's default newtype-variant encoding yields
  `{ "Ready": "<path>" }` / `{ "FirstLaunchPending": "<path>" }`; the UI models it as that
  union and discriminates with `"FirstLaunchPending" in status.ce2_state`, deriving `ce2Path`
  from whichever key is present.
- **Section gating.** The load-order/INI section body is wrapped
  `{#if isStarfield && starfieldPending} …guidance+Re-check… {:else} …existing controls… {/if}`
  so pending blocks every management action at once (rather than disabling buttons piecemeal).
- **Drift independence.** The drift notice renders off `isStarfield && starfield?.drift`,
  wholly independent of the pending gate, so it can never block management (SFDET-03).

## Deviations from Plan

**[Rule 3 — Blocking issue / boundary adherence] Added `steam::starfield_status_for` instead of
deriving the library root in the adapter.**
- **Found during:** Task 1.
- **Issue:** `steam::resolve_game` returns `install_dir` + `prefix` but NOT the Steam library
  root that `steam::starfield_status(prefix, library_root, appid)` needs. Deriving it in the
  command would put path construction in the adapter, violating the plan's engine-boundary hard
  rule and threat T-06-04 ("No path construction … in the command").
- **Fix:** Added `steam::starfield_status_for(appid)` to `resolve.rs` (re-export in the steam
  `lib.rs`) that resolves the game, derives the library root in-engine, and returns the
  aggregate. The command is now a genuine one-line forwarder consistent with the other
  `games.rs` adapters.
- **Files modified:** `crates/steam/src/resolve.rs`, `crates/steam/src/lib.rs` (beyond the
  plan's stated file list).
- **Verification:** `library_root_is_install_dir_grandparent` unit test; `cargo build -p
  nextwist`; `cargo clippy -p nextwist -p nextwist-steam -- -D warnings` clean.

**Total deviations:** 1 auto-fixed (Rule 3). **Impact:** keeps the adapter path-construction-free
per the plan's central boundary rule; adds one small, unit-tested engine function.

## Threat Mitigations Applied

- **T-06-04 (Tampering/EoP):** the command forwards the engine value verbatim; all path
  construction (library root, CE2 walk, traversal guards) is in `steam`. The adapter builds no
  paths and writes nothing.
- **T-06-05 (Info disclosure / injection):** `installed`/`validated` build numbers and the CE2
  path render as escaped plain text (Svelte default) — no `{@html}`, no eval of prefix-derived
  strings.
- **T-06-SC:** no npm or crate dependency added — supply-chain surface unchanged.

## Known Stubs

None. The drift notice is intentionally **dormant** this phase: the engine's `VALIDATED_BUILD`
is `0`, so `steam::starfield_status` returns `drift: null` and the notice never renders until
Phase 9 seeds the real validated build. The surfacing is fully built and will light up then.

## Verification

- `cargo build -p nextwist --locked` — succeeds with `starfield_status` registered (WebKitGTK
  4.1 dev libs ARE present on this host; the src-tauri build was NOT environment-blocked).
- `cargo test -p nextwist-steam --locked starfield_status` — green; `library_root` test green.
- `cargo clippy -p nextwist -p nextwist-steam --all-targets --locked -- -D warnings` — clean.
- `npm --prefix frontend run check` — 142 files, 0 errors, 0 warnings.
- `npm --prefix frontend run build` — built OK (adapter-static → `frontend/build`).

## Notes for Later Phases

- **Phase 9** must set `steam::ce2::VALIDATED_BUILD` (currently `0`) to activate the drift
  notice and perform the real in-app visual confirmation of the first-launch pending→Ready
  transition on a live Proton install.

## Commits

- `3b09486` feat(06-03): thin starfield_status Tauri command + engine resolver
- `cbfe9e9` feat(06-03): Starfield first-launch gate + Re-check + drift notice

## Self-Check: PASSED

- Files verified on disk: all 6 modified files present; `06-03-SUMMARY.md` written.
- Commits verified in git log: `3b09486`, `cbfe9e9`.
