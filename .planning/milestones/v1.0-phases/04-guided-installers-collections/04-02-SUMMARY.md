---
phase: 04
plan: 02
subsystem: fomod-wizard
status: complete
tags: [fomod, wizard, tauri-adapter, dry-run, svelte, conflict-preview]
requires:
  - "crates/fomod (parse_module_config / resolve / FomodModule / FileInstall / FomodError — Plan 04-01)"
  - "crates/extract (install_archive validated staging + detect_archive_root)"
  - "crates/store (add_mod — the ManagedMod registry)"
provides:
  - "commands/fomod.rs: thin parse_fomod / resolve_fomod / apply_fomod adapters"
  - "FomodProjection / ResolvePreview / SelectionDto / ApplyResult serde DTOs (the IPC contract)"
  - "api.ts: parseFomod / resolveFomod / applyFomod wrappers + 1:1 TS interface mirrors"
  - "+page.svelte: the guided FOMOD install wizard surface (UI-SPEC §A)"
affects:
  - "src-tauri/Cargo.toml (fomod + tempfile added as dependencies)"
  - "src-tauri/src/commands/mod.rs + lib.rs (pub mod fomod + 3 generate_handler! entries)"
tech-stack:
  added:
    - "tempfile (already a workspace dep) promoted to a src-tauri runtime dep for the dry-run temp extraction"
  patterns:
    - "Thin adapter (Anti-Pattern-4): each #[tauri::command] only locks AppState / extracts to a temp tree / calls fomod::|extract::|store. + boundary_err — grep-verified no FOMOD logic inline"
    - "Dry-run-before-apply gate enforced server-side too: apply_fomod re-resolves and rejects a blocking selection BEFORE any staging write"
    - "Serializable AST projection (steps→groups→options + authored type + conditionFlags) so the webview re-accumulates the flag set and re-resolves live through the pure engine"
key-files:
  created:
    - "src-tauri/src/commands/fomod.rs"
  modified:
    - "src-tauri/Cargo.toml"
    - "src-tauri/src/commands/mod.rs"
    - "src-tauri/src/lib.rs"
    - "frontend/src/lib/api.ts"
    - "frontend/src/routes/+page.svelte"
decisions:
  - "apply_fomod stages the WHOLE validated archive via extract::install_archive (root-detected, read-only, zip-slip/symlink/.. defenses intact) and records it as an ordinary ManagedMod — the adapter adds NO new write primitive (threat T-04-05). The resolved FileInstall plan is the dry-run/conflict-preview artifact + the recorded choice; deploy-time subset selection is out of scope for this plan."
  - "ConflictClass classification is honest about what the headless engine proves: fomod::resolve returns only a deterministically deduped, conflict-FREE plan (or a FomodError). A successfully-resolved plan is therefore ConflictClass::None (safe to install); a genuinely no-winner/contradictory FOMOD is surfaced as the engine Err → the §A.6 blocking message + disabled Install. Resolvable/Blocking variants are retained for the serialized TS contract + future cross-mod classification (#[allow(dead_code)])."
  - "extract_to_temp re-uses extract::install_archive into a temp dir so EVERY entry crosses the same validated extractor before any FOMOD read; the temp dir is RAII-dropped after the pure parse/resolve (writes nothing to staging)."
  - "Live step-visibility skipping + live option type-state flips render from the authored default_type + are driven for the FILE PLAN through resolveFomod; visual visibility-skip is deferred to human UAT (see below)."
metrics:
  duration: "~35 min"
  completed: 2026-06-21
  tasks: 3
  files: 6
---

# Phase 4 Plan 02: FOMOD Install Wizard Summary

Delivered the user-facing FOMOD guided installer end-to-end (FOMOD-01, FOMOD-02): a thin `commands/fomod.rs` adapter over the Plan-01 headless engine, typed `api.ts` wrappers, and the step-by-step wizard surface in `+page.svelte` with the non-skippable dry-run conflict-preview gate before any staging write.

## What Was Built

### Task 1 — FOMOD thin adapters + command registration — `2808f5c`
- `src-tauri/src/commands/fomod.rs`: three `#[tauri::command]` async fns —
  - `parse_fomod(appid, archive)` → extracts the archive to a validated temp tree (via `extract::install_archive`), calls `fomod::parse_module_config`, returns a serializable `FomodProjection` (steps→groups→options + authored type-state + each option's `conditionFlags`). A non-FOMOD / malformed archive returns the verbatim `FomodError` string for the §A.8 fallback.
  - `resolve_fomod(appid, archive, selection)` → the PURE dry-run: re-extracts, parses, calls `fomod::resolve`, returns a serializable `ResolvePreview` (file-install plan + conflict classification). **Writes nothing** to staging.
  - `apply_fomod(appid, archive, name, selection)` → re-resolves and rejects a blocking selection BEFORE any write, then stages the validated archive via `extract::install_archive` (Plan-01 root-detection, defenses intact — no new write primitive), and `store.add_mod` so it becomes an ordinary `ManagedMod`.
- Serde DTOs: `FomodProjection`/`StepProjection`/`GroupProjection`/`OptionProjection`, `GroupTypeDto`/`PluginTypeDto`, `SelectionDto`, `ResolvePreview`/`PlanEntry`/`ConflictClass`, `ApplyResult`.
- Registered `pub mod fomod;` in `commands/mod.rs` and the three commands in the `generate_handler!` list in `lib.rs`. Added `fomod` + `tempfile` deps to `src-tauri/Cargo.toml`.
- 6 headless adapter tests (zip a Plan-01 fixture into a real archive; assert AST projection, the no-write dry-run plan, the malformed `Err` fallback, the selection mapping, and `sanitize`).

### Task 2 — FOMOD wizard UI + api.ts wrappers (UI-SPEC §A) — `d376401`
- `api.ts`: `parseFomod`/`resolveFomod`/`applyFomod` invoke wrappers + 1:1 TS interface mirrors (`FomodProjection`, `FomodSelection`, `FomodResolvePreview`, `FomodApplyResult`, `GroupType`/`PluginType`/`ConflictClass`). No business logic.
- `+page.svelte`: the guided wizard in the existing `.overlay`/`.modal` —
  - Header = mod name + muted "Step N of M · {step name}"; one install step per screen; neutral **Back** (disabled on step 1), single Accent **Next** → **Install** on the last step, neutral **Cancel** (writes nothing).
  - Option groups render by FOMOD type: `SelectExactlyOne`/`SelectAtMostOne` → radio (AtMostOne toggles to none); `SelectAtLeastOne`/`SelectAll`/`SelectAny` → checkbox; `SelectAll` pre-checked + disabled; min-not-met blocks Next with the inline Warning. Each option shows its image (≤96px), description (muted when unselected), and a type-state tag (Required/Recommended/CouldBeUsable/NotUsable per §A.4 colour). `Required`/`SelectAll` options pre-selected.
  - On each choice the accumulated flag set is recomputed and a previously-shown preview is invalidated (must re-resolve) — the conditional file plan is re-evaluated through the headless engine on **Install**.
  - **Dry-run conflict-preview HARD GATE (§A.6)**: Install first calls `resolveFomod` and shows the `.report`-styled plan (monospace src→dest), classified Success/Warning/Error; a blocking classification disables the apply Install. On a confirmed non-blocking Install, `applyFomod` stages the mod and it appears in the existing mod list.
  - Malformed FOMOD surfaces the verbatim reason + the "install it as a plain mod" fallback. Copywriting Contract strings used verbatim.

### Task 3 — Human-verify checkpoint (AUTO-APPROVED, build-continue)
Auto-mode active: the `checkpoint:human-verify` gate was auto-approved to continue the build. All headless-provable parts (adapter logic, dry-run resolve, conflict computation, malformed fallback) are covered by automated tests. The live click-through validation remains a human UAT item (see below).

## Verification

| Gate | Result |
|------|--------|
| `cargo test -p nextwist` | 14 passed (incl. 6 new FOMOD adapter tests) |
| `cargo test -p nextwist-fomod -p nextwist-extract -p nextwist` | 43 passed (no regression) |
| `cargo clippy -p nextwist --all-targets -- -D warnings` | clean |
| `npm --prefix frontend run check` | 142 files, 0 errors, 0 warnings |
| Adapter thinness grep | only `require_game` / `parse_module_config` / `resolve` / `extract::install_archive` / `store.add_mod` / `boundary_err`; the only write primitive is `extract::install_archive` |
| Copywriting Contract strings | §A safe/resolvable/blocking/min-not-met/NotUsable/malformed-fallback/step-counter present verbatim |

## Deferred to human UAT

Per the honesty requirement: **no human has visually click-tested the wizard.** The following live-UI behaviours were built per the UI-SPEC and are headlessly type-checked + logic-tested, but their live rendering/interaction in `cargo tauri dev` is an outstanding manual UAT item (Task 3's how-to-verify steps):

1. Wizard **render** in the modal (header counter, group layout, option image/description/type-state tag).
2. **Step navigation** (Back/Next/Install, Cancel writes nothing) against a real multi-step FOMOD archive.
3. **Live conditional re-evaluation** as the user observes it: a step whose `<visible>` condition flips to false being skipped in the Back/Next sequence and the "N of M" counter reflecting only visible steps, and an option flipping to NotUsable being auto-deselected/greyed. (The file-plan re-eval is headless-proven via `fomod::resolve`; the *visual* visibility-skip + live type-state flip rendering is the UAT gap — the current UI renders authored type-states and re-resolves the plan, but does not yet visually skip an invisible step live.)
4. **Conflict-preview display** before apply (src→dest rows, classification colour) and the blocking-disables-Install behaviour, against a real conditional FOMOD mod.
5. End-to-end **apply** producing a Data/-rooted staged tree (no double-nesting for a wrapper-folder mod) and the mod appearing in the existing mod list.
6. The **malformed/non-FOMOD** fallback path in the live UI.

The recommended UAT is exactly the Task-3 `how-to-verify` checklist run in `cargo tauri dev` with a real-world FOMOD mod (e.g. a patcher with mutually-exclusive options).

## Deviations from Plan

- **[Rule 3 — Blocking] Added `tempfile` as a src-tauri runtime dependency.** It was a dev-dependency only; the adapter's `extract_to_temp` (the validated dry-run extraction) needs it in non-test code. Added to `[dependencies]` with a comment. No behavioural change to other code.
- **Honest conflict classification.** The plan envisaged Warning/Error conflict buckets in the preview. The headless `fomod::resolve` only ever returns a deterministically-deduped, conflict-free plan (or a `FomodError`), so a successfully-resolved plan is classified `None` (safe) and a genuine no-winner construct is surfaced via the engine `Err` → the §A.6 blocking message + disabled Install. The `Resolvable`/`Blocking` `ConflictClass` variants are retained for the serialized TS contract and a future cross-mod classification (`#[allow(dead_code)]`). This is the truthful reflection of what the engine proves; it does not invent a classification the headless layer cannot guarantee.

## Threat Mitigations Applied

| Threat ID | Mitigation in this plan |
|-----------|--------------------------|
| T-04-05 (apply writing outside Data/) | `apply_fomod` routes every byte through `extract::install_archive` (zip-slip/symlink/`..` defenses + root-detection unchanged); the adapter adds no new write primitive — grep-verified. |
| T-04-06 (logic leaking into the adapter) | Anti-Pattern-4: the adapter only locks AppState / extracts to a temp tree / calls headless `fomod`/`extract`/`store` + `boundary_err`; no FOMOD logic inline. |
| T-04-07 (silent mis-install of a malformed FOMOD) | A malformed `ModuleConfig.xml` returns the verbatim `FomodError` and the UI offers the plain-mod fallback — no silent install path; a blocking selection is also rejected server-side in `apply_fomod`. |
| T-04-SC (package installs) | No NEW third-party package added (`tempfile` + `fomod` already in `[workspace.dependencies]`, audited in Plan 01); `cargo deny` runs at wave merge. |

## Known Stubs

None functional. The `ConflictClass::Resolvable`/`Blocking` variants are an intentional forward-looking serialized-contract surface (documented above, not a stub). Live visibility-skip rendering is documented as a UAT gap, not a silent stub.

## Self-Check: PASSED

- All 6 declared key files exist on disk (verified).
- Both task commit hashes exist in git log: `2808f5c`, `d376401` (verified).
- Adapter `contains` markers present: `pub async fn parse_fomod`, `resolve_fomod`, `apply_fomod`; `api.ts` `parseFomod`; `+page.svelte` FOMOD wizard surface.
