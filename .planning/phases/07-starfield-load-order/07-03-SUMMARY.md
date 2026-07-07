---
phase: 07-starfield-load-order
plan: 03
subsystem: frontend-load-order-surfacing
tags: [starfield, frontend, svelte, load-order, protected, medium, reconcile, sflo]
requires:
  - "07-01 (loadorder engine: PluginView.medium/.protected, SortProposal.masterlist_date, ReconcileState)"
  - "07-02 (Tauri wiring: list_plugins -> PluginView, reconcile_plugins command)"
  - "frontend v1.0 load-order/verify view (+page.svelte §4 verify, §5 plugins) + api.ts mirror layer"
provides:
  - "api.ts: PluginInfo.medium/.protected, SortProposal.masterlist_date, ReconcileState type, reconcilePlugins()"
  - "+page.svelte: protected locked rows, MEDIUM/PROTECTED badges, muted masterlist-age note, SFLO-04 InSync/Drift reconciliation block"
affects:
  - "Phase 9 (hardware validation) confirms the protected/medium/reconcile surfaces render correctly against the real game"
tech-stack:
  added: []
  patterns:
    - "Pure render layer: every additive element reads an engine-supplied boolean/enum; the UI never classifies"
    - "Defense-in-depth UI courtesy lock (protected reorder/toggle guards) atop the authoritative engine guard"
    - "Externally-tagged serde enum mirrored as a TS union ('InSync' bare string | { Drift: string[] })"
key-files:
  created: []
  modified:
    - frontend/src/lib/api.ts
    - frontend/src/routes/+page.svelte
decisions:
  - "reconcileState loaded inside the existing onVerify handler (Starfield-gated), following the verifyReport load pattern — no new action button"
  - "Protected badge is a focusable (tabindex=0) role=img status affordance carrying the reason via aria-label/title; the UI-SPEC-mandated a11y focusability required one scoped svelte-ignore a11y_no_noninteractive_tabindex"
  - "MEDIUM badge shown IN ADDITION to the kind badge (medium masters stay ESM in the masters group, so group-divider grouping is unchanged)"
  - "checked={p.enabled || p.protected} + disabled on protected toggle so protected masters always render active and locked"
  - "reconcile Drift repair copy points at the existing section-5 Save plugin order (reuse, no new repair action)"
metrics:
  duration: ~20m
  completed: 2026-07-07
status: complete
---

# Phase 7 Plan 03: Frontend Load-Order Surfacing Summary

Surfaced the 07-01/07-02 engine truth in the shipped `+page.svelte` load-order/verify view as a PURE render layer — protected masters now render as locked rows with an accessible PROTECTED lock badge, medium masters carry a distinct MEDIUM badge, a muted masterlist-age note sits near Sort with LOOT, and the SFLO-04 reconciliation shows a calm in-sync line vs. an amber drift list (never a raw diff). Every new element reflects an engine-supplied boolean/enum; the UI decides nothing. No new npm dependency, no new visual tokens, all additive elements behind the existing Phase-6 first-launch gate.

## What was built

**Task 1 — api.ts mirror types + reconcilePlugins binding (commit 7d811c8)**
- `PluginInfo` gains `medium: boolean` + `protected: boolean` (mirrors `loadorder::PluginView`); `SortProposal` gains `masterlist_date: string`.
- New `ReconcileState = "InSync" | { Drift: string[] }` type — matches 07-01's default externally-tagged serde (`"InSync"` bare string confirmed against `crates/loadorder/src/reconcile.rs`, NOT `{InSync:null}`).
- `reconcilePlugins(appid): Promise<ReconcileState>` binding for the `reconcile_plugins` command.
- `savePluginOrder` unchanged — the extra `medium`/`protected` fields are harmlessly ignored by the Rust `core::Plugin` deserialize (not stripped).

**Task 2 — protected rows / badges / masterlist note / reconciliation (commit 3903c8f)**
- **Protected locked rows (SFLO-03):** rows with `protected === true` render reorder ▲▼ `disabled`, toggle `disabled` + `checked`, an `li.protected` muted fill, and a focusable `role="img"` PROTECTED badge (🔒 + text) carrying the UI-SPEC tooltip via `aria-label`/`title`. `onPluginReorder`/`onPluginToggle` guard against moving/toggling a protected row (courtesy layer atop the engine's authoritative `ProtectedMaster` guard).
- **MEDIUM badge (SFLO-03):** `.badge-medium` (teal-neutral `#eaf3f0`/`#9cccb9`/`#1a6b52`, deliberately NOT accent-blue) with the medium tooltip aria-label, shown alongside the kind badge.
- **Masterlist-age note (SFLO-02):** a `.muted` (grey, not amber) `Masterlist from {date} — may be stale` note near Sort, rendered only when `isStarfield && sortProposal?.masterlist_date`.
- **SFLO-04 reconciliation:** a Starfield-only block in the verify surface driven by a new `reconcileState` `$state`, loaded inside `onVerify`. `InSync` → a calm `.ok` green line ("In sync — Starfield applied its expected on-launch changes."); `Drift(names)` → the existing amber `.warn` box listing the unexpected names + repair guidance. Never reuses `.err`/`.drift-notice` for the in-sync case.
- **CSS:** only `.badge-medium`, `.badge-protected`, `li.protected` added, reusing detected hex/spacing/weight tokens (light-only). The LOOT propose-then-apply preview is reused verbatim (no new preview affordance).

## Verification

- `npm --prefix frontend run check` (svelte-check): **142 files, 0 errors, 0 warnings** (the one a11y `noninteractive tabindex` notice on the deliberately-focusable lock badge is silenced with a scoped, documented `svelte-ignore`).
- `npm --prefix frontend run build` (adapter-static SPA): **built clean, site written to `build`**.
- All 16 new markup/type markers confirmed present via forced-text `grep -a` (the file carries non-UTF-8 bytes; plain grep/`git diff` report it binary — additive Edits preserved the original byte content, file grew 93771→97803 bytes consistent with the added markup).

## Deviations from Plan

### Auto-fixed / minor

**1. [Rule 3 - Blocking] Scoped `svelte-ignore` for the UI-SPEC-mandated focusable lock badge**
- **Found during:** Task 2 (first `<automated>` check gate).
- **Issue:** svelte-check flagged `a11y_no_noninteractive_tabindex` on the protected lock badge. The UI-SPEC (§1 Accessibility) and the plan explicitly require a *focusable, non-interactive* status affordance carrying the reason — so `tabindex="0"` on a `role="img"` span is the intended design, not a bug.
- **Fix:** Added a single documented `<!-- svelte-ignore a11y_no_noninteractive_tabindex -->` above that span, keeping the mandated focusability while restoring a 0-warning check. The aria-label independently satisfies the "reachable in a11y tree" acceptance criterion.
- **Files modified:** `frontend/src/routes/+page.svelte`.
- **Commit:** 3903c8f.

## Notes for downstream plans

- The reconcile block only renders after an explicit Verify (Starfield), matching the existing verify-on-demand pattern — no automatic polling.
- Protected/medium/reconcile all degrade gracefully: when the 07-02 live libloot probe fails, `protected` is empty and rows simply render unlocked (by design); when `reconcile_plugins` hasn't run, no reconciliation block shows.
- Phase 9 hardware validation should confirm the real Starfield implicit/protected set drives the lock and that the expected on-launch rewrite classifies as `InSync` on a real machine.

## Self-Check: PASSED

- FOUND: frontend/src/lib/api.ts (PluginInfo.medium/.protected, SortProposal.masterlist_date, ReconcileState, reconcilePlugins)
- FOUND: frontend/src/routes/+page.svelte (protected rows, MEDIUM/PROTECTED badges, masterlist note, reconcileState block, .badge-medium/.badge-protected/li.protected CSS)
- FOUND commits: 7d811c8, 3903c8f
