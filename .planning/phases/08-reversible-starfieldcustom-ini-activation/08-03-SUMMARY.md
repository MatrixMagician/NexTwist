---
phase: 08-reversible-starfieldcustom-ini-activation
plan: 03
subsystem: tauri-shell + frontend
tags: [tauri, svelte, frontend, ini, starfield, ui]

# Dependency graph
requires:
  - phase: 08-reversible-starfieldcustom-ini-activation
    plan: 01
    provides: "deploy::{preview_ini_activation, ensure_ini_active}, IniActivationPreview/IniOutcome/IniConflictResolution"
  - phase: 06-starfield-detection
    provides: "isStarfield / ce2_state / starfieldPending frontend derives + the Starfield notice stack"
provides:
  - "thin Tauri commands preview_ini_activation (read-only) + apply_ini_activation (Block/UseNexTwist) forwarding one engine call each"
  - "api.ts IniActivationPreview / IniOutcome / IniConflictResolution TS types + previewIniActivation / applyIniActivation bindings"
  - "three Starfield-view surfaces (activation-state tag, no-silent-edit preview modal, amber conflict box) assembled from existing +page.svelte classes"
affects: [09-hardware-validation]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Thin IPC adapter (deploy.rs analog): require_game + one deploy:: call + boundary_err — zero safety/format/path logic at the boundary"
    - "Activation-state line read off the read-only preview within a two-command engine surface (no verify() coupling in this plan)"
    - "New Starfield surfaces reuse existing classes only (.first-launch/.badge/.overlay/.modal/pre.txt-preview/.warn/.muted/.ok) — no new component, class, or CSS custom property"

key-files:
  created:
    - src-tauri/src/commands/gameconfig.rs
  modified:
    - src-tauri/src/commands/mod.rs
    - src-tauri/src/lib.rs
    - frontend/src/lib/api.ts
    - frontend/src/routes/+page.svelte

key-decisions:
  - "Activation-state indicator is derived from previewIniActivation (will_create => not active; conflict => blocked; will_edit && !conflict => active) — the reading this plan's two-command API supports; the precise already-active-vs-key-stripped oracle is verify().ini_drift (08-02), deliberately out of this plan's scope"
  - "apply_ini_activation carries the IniConflictResolution arg verbatim: Block for the normal Enable path, UseNexTwist only from the conflict box's explicit 'Use NexTwist's value' choice"
  - "Enable button opens the modal only; the modal confirm / 'Use NexTwist's value' are the ONLY writers (SFINI-01 no-silent-edit)"

requirements-completed: [SFINI-01, SFINI-03]

coverage:
  - id: D1
    description: "Two thin Tauri commands forward to the Plan-01 engine (preview read-only; apply with Block/UseNexTwist), registered in mod.rs + lib.rs invoke_handler, with typed api.ts bindings mirroring the engine serde shapes (SFINI-01/03 boundary)"
    requirement: "SFINI-01"
    verification:
      - kind: command
        ref: "~/.cargo/bin/cargo check -p nextwist && cargo clippy -p nextwist --all-targets -- -D warnings (clean); npm --prefix frontend run check (0 errors)"
        status: pass
    human_judgment: false
  - id: D2
    description: "The adapter holds no safety logic — each command is require_game + one deploy:: call + boundary_err (no path resolution, file I/O, or merge logic)"
    requirement: "SFINI-01"
    verification:
      - kind: command
        ref: "src-tauri/src/commands/gameconfig.rs — grep confirms only require_game + deploy::{preview_ini_activation,ensure_ini_active} + .map_err(boundary_err)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The Starfield view shows an activation-state tag + a no-silent-edit preview modal that authorizes the write ONLY on confirm; the Enable button never itself calls applyIniActivation (SFINI-01)"
    requirement: "SFINI-01"
    verification:
      - kind: command
        ref: "grep -a confirms Surface 1/2 gated by isStarfield && !starfieldPending; openIniModal sets iniModalOpen only; onActivateIni (modal confirm) is the sole Block writer; npm run build green"
        status: pass
    human_judgment: true
    rationale: "The no-silent-edit gate and copy render is code-verified, but the end-to-end visual/interaction (modal shows the exact lines, provenance reads correctly, state refreshes) needs a running Starfield+Proton session — deferred to the Phase 9 on-hardware UAT."
  - id: D4
    description: "A non-empty user sResourceDataDirsFinal surfaces as an amber .warn conflict box with the user's value + explicit Keep-mine / Use-NexTwist choices, never an auto-clobber (SFINI-03)"
    requirement: "SFINI-03"
    verification:
      - kind: command
        ref: "grep -a confirms the conflict box renders on iniConflict !== null with 'Keep mine' (no write, re-preview) + 'Use NexTwist's value' (applyIniActivation UseNexTwist); heading 'StarfieldCustom.ini already sets a loose-file path.' present"
        status: pass
    human_judgment: true
    rationale: "Conflict branch + copy are code-verified; triggering it requires a real pre-existing non-empty user value on Proton hardware — deferred to Phase 9 UAT."
  - id: D5
    description: "Zero new components/CSS/deps; the deployed-vs-loaded distinction (SFVER-02) is absent"
    requirement: "SFINI-01"
    verification:
      - kind: command
        ref: "git diff -a shows the +page.svelte <style> block untouched (surfaces reuse existing classes only); no new npm/cargo dependency; no deployed-vs-loaded copy present"
        status: pass
    human_judgment: false

# Metrics
duration: 18min
completed: 2026-07-08
status: complete
---

# Phase 8 Plan 03: Reversible StarfieldCustom.ini Activation — Tauri Adapter + Starfield UI Surfaces Summary

**Two thin Tauri commands (`preview_ini_activation` read-only + `apply_ini_activation` with a Block/UseNexTwist choice) forward one Plan-01 engine call each with zero boundary logic, typed end-to-end through `api.ts`; the Starfield game view gains three surfaces — an activation-state tag, a no-silent-edit preview modal that authorizes the write only on confirm, and an amber keep-mine/use-NexTwist conflict box — all assembled from existing `+page.svelte` classes per the 08-UI-SPEC, with no new component, CSS, or dependency.**

## Performance

- **Duration:** ~18 min
- **Tasks:** 2 (both `type="auto"`)
- **Files:** 5 (1 created, 4 modified)

## Accomplishments
- **Thin adapter (`gameconfig.rs`, NEW):** `preview_ini_activation` (`require_game` → `deploy::preview_ini_activation(&game)` → `boundary_err`, read-only, no store write) and `apply_ini_activation` (`require_game` → lock store → `deploy::ensure_ini_active(&store, &game, resolution)` → `boundary_err`). Both are 3-5 lines, mirroring `deploy.rs`; all path/format/write safety stays in the Plan-01 engine.
- **Registration:** `pub mod gameconfig;` in `commands/mod.rs`; both commands added to the `lib.rs` `tauri::generate_handler!` list beside `commands::games::starfield_status`.
- **Typed bindings (`api.ts`):** `IniConflictResolution` (`"Block" | "UseNexTwist"`), `IniActivationPreview` (`will_create`/`will_edit`/`lines`/`conflict`), the externally-tagged `IniOutcome` union (`"Activated" | "AlreadyActive" | { Blocked: { current_value } } | "Restored" | "NotActive"`), and `previewIniActivation` / `applyIniActivation` bindings mirroring `starfieldStatus`.
- **Three Starfield surfaces (`+page.svelte`):** all gated on the existing `isStarfield` derive, shown once CE2 is `Ready` (deferring to the Phase-6 first-launch gate while `starfieldPending`), placed in the drift/masterlist notice stack:
  - **Surface 1** — `.first-launch` info box + a `.badge .ok` (`● Loose-file loading active`) / `.badge .muted` (`○ … not active`) state tag + a neutral **Enable loose-file loading** button that ONLY opens the modal.
  - **Surface 2** — `.overlay`/`.modal` preview modal showing the exact `[Archive]` lines in `pre.txt-preview`, a `.muted` will-create/will-edit provenance line, a `button.cta` **Activate loose-file loading** (the sole Block writer) + a neutral **Cancel**.
  - **Surface 3** — amber `.warn` box with the user's current `sResourceDataDirsFinal` in `pre.txt-preview` + **Keep mine** (no write, re-previews) and **Use NexTwist's value** (`applyIniActivation(.., "UseNexTwist")`).

## Task Commits

1. **Task 1: thin adapter + registration + api.ts bindings** — `a902041` (feat)
2. **Task 2: Starfield activation surfaces in +page.svelte** — `864a2e1` (feat)

## Files Created/Modified
- `src-tauri/src/commands/gameconfig.rs` (NEW) — the two thin INI-activation adapters.
- `src-tauri/src/commands/mod.rs` — `pub mod gameconfig;`.
- `src-tauri/src/lib.rs` — both commands in the `invoke_handler` list.
- `frontend/src/lib/api.ts` — INI TS types + `previewIniActivation` / `applyIniActivation` bindings.
- `frontend/src/routes/+page.svelte` — the three Starfield-view surfaces + the `iniPreview`/`iniModalOpen` state, `iniActive`/`iniConflict` derives, and the `loadIniPreview`/`openIniModal`/`onActivateIni`/`onUseNexTwistValue` handlers; the preview is loaded when CE2 resolves Ready and reset on game switch. The `<style>` block was NOT touched.

## Decisions Made
- **Activation-state indicator is derived from `previewIniActivation`.** This plan's engine API surface is exactly two commands (preview + apply). Within that, `will_create` ⇒ INI absent ⇒ not active; a non-null `conflict` ⇒ blocked (Surface 3); `will_edit && conflict === null` ⇒ active. The precise "already-active vs key-stripped" oracle is `verify().ini_drift` (shipped in 08-02) — deliberately NOT wired into this status line, which would exceed the plan's two-command scope and couple the line to a full `Data/`-tree verify walk. See Deviations for the documented nuance.
- **`resolution` is passed through verbatim.** The adapter only selects `Block` (normal Enable path) vs `UseNexTwist` (the conflict box's explicit choice); all conflict/backup/reversibility logic stays in the engine (T-08-08).
- **The Enable button opens the modal only.** `openIniModal` sets `iniModalOpen`; the modal confirm (`onActivateIni`) and `onUseNexTwistValue` are the sole writers — SFINI-01 no-silent-edit, T-08-09.

## Deviations from Plan

### Known Limitation (documented, not a code change)

**1. [Known Limitation] Surface-1 "active" is a preview-derived proxy, not the exact drift oracle**
- **Context:** The 08-UI-SPEC Surface 1 wants a binary active/not-active tag. The plan scoped this plan's engine API to `previewIniActivation` + `applyIniActivation` only. `preview.will_edit` means "the INI exists", which cannot by itself distinguish an already-exactly-active INI from a key-stripped one (this exact nuance is called out in 08-02-SUMMARY's decisions). The authoritative oracle is `verify().ini_drift` (`Option<IniDrift>`, shipped 08-02), which is NOT exposed in `api.ts`'s `VerifyReport` and whose surfacing is out of this plan's scope.
- **Resolution:** Drove Surface 1 from the preview (`will_edit && !conflict` ⇒ active), matching the plan's two-command API and UI-SPEC copy. Every action re-previews and `apply` is idempotent + fully reversible, so the tag can never authorize an unsafe or irreversible action. Wiring `verify().ini_drift` into the status line is a small follow-up (add `ini_drift` to the TS `VerifyReport` + call `verify()` on Ready) tracked for Phase 9 alongside the on-hardware SFVER work.
- **Files:** none beyond the planned `+page.svelte` state line — this is a scoping note, not an extra change.
- **Verification:** svelte-check + build green; the imprecision is bounded (existence-with-no-conflict) and safe.

**Total deviations:** 0 code deviations. 1 documented known-limitation (activation-state precision, follow-up scoped to Phase 9). **Impact:** None to safety or reversibility — the engine remains the sole authority and every UI action is previewed + reversible.

## Known Stubs
None — all three surfaces are wired to live engine data (`previewIniActivation` / `applyIniActivation`); no hardcoded/placeholder values flow to the UI.

## Issues Encountered
None.

## User Setup Required
None — code only; zero external crates, no npm dependency added (frontend reuses existing classes), no `cargo-deny`/MSRV/AppImage surface change.

## Next Phase Readiness
- **Phase 9 (on-hardware validation)** owns the two human_judgment items (D3/D4 end-to-end visual/interaction on Proton) and the optional Surface-1 precision follow-up (wire `verify().ini_drift` into the status line). The engine + adapter + UI surface for SFINI-01/03 are complete and green.
- The phase's SFINI-01..05 requirement set is now fully wired end-to-end: engine (08-01) → choke-point/verify/repair (08-02) → Tauri adapter + Starfield UI (08-03).

---
*Phase: 08-reversible-starfieldcustom-ini-activation*
*Completed: 2026-07-08*

## Self-Check: PASSED
- All created/modified files present on disk (gameconfig.rs, mod.rs, lib.rs, api.ts, +page.svelte, 08-03-SUMMARY.md).
- Both task commits present in git history (a902041, 864a2e1).
- Adapter gate: `cargo check -p nextwist` + `clippy -p nextwist --all-targets -- -D warnings` clean.
- Frontend gate: `npm --prefix frontend run check` (0 errors) + `npm --prefix frontend run build` (built) green.
- Wave-merge gate: `cargo test --workspace --locked` (all suites 0 failed) + `cargo clippy --workspace --all-targets -- -D warnings` clean.
