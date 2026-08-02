---
phase: 7
slug: starfield-load-order
status: draft
shadcn_initialized: false
preset: none
created: 2026-07-07
---

# Phase 7 — UI Design Contract (Starfield Load Order)

> **Additive** contract. Extends the shipped v1.0 load-order/plugins view in
> `frontend/src/routes/+page.svelte` (§5 "Plugins & load order"). No new visual language —
> every new element reuses the existing tokens, badge/row/toolbar patterns detected below.
>

---

## Design System (detected from `+page.svelte`)

| Property | Value |
|----------|-------|
| Tool | none (hand-rolled CSS in a single `<style>` block; not React → no shadcn) |
| Preset | not applicable |
| Component library | none — plain Svelte 5 runes markup |
| Icon library | none — inline Unicode glyphs (`▲ ▼ ● ○ ✓ ⚠`) |
| Font | `system-ui, sans-serif`; `ui-monospace, monospace` for plugin/file names (`.mono`) |
| Theme | **light-only, hard-coded hex** — no CSS variables, no `prefers-color-scheme`, no dark mode. Additive elements MUST match (do NOT introduce theming). |
| Layout | single-column, `main { max-width: 820px }`, numbered `<section>` cards |

### Detected color roles (reuse verbatim — do not add new hex)

| Role | Hex | Existing usage |
|------|-----|----------------|
| Accent / primary CTA | `#0a66c2` (hover `#0857a6`) | `button.cta`, progress bars, active-profile indicator |
| Destructive | `#cf222e` (hover `#a40e26`) | `button.destructive`, `.danger-link`, keyring banner |
| Success | `#1a7f37` | `.ok`, done states, nxm-toast |
| Warn / caution | text `#9a6700`, bg `#fff8e5`, border `#e6c200` | `.warn`, `.drift-notice`, `.loot-proposal`, moved-tag |
| Muted text | `#777` / `#555` | `.muted`, secondary meta |
| Neutral surface | `#f3f3f3`; borders `#ccc` (strong) / `#eee` (row) | cards, badges, empty/first-launch boxes |

### Detected badge pattern (the anchor for the new badges)

`.badge` base: `font-size: 0.7rem; font-weight: 600; padding: 0.05rem 0.35rem; border-radius: 3px; border: 1px solid #ccc; color: #555;` with per-kind variants:
`.badge-esm` (blue `#eef3fb`/`#b8cdec`/`#0a4a8a`), `.badge-esl` (purple `#f0eefb`/`#c8bce8`/`#5a3a8a`), `.badge-esp` (grey `#f3f3f3`). **All badges carry text** (`ESM`/`ESL`/`ESP`) — color is never the only signal.

### Detected row pattern

`ol.priority.plugins li` — flex row, `min-height: 32px`, `gap: 0.5rem`: `[reorder ▲▼] [checkbox] [.mono name] [.badge kind]`. Disabled plugins get `li.disabled` (`color: #777`). Group dividers (`li.group-divider`) are uppercase grey labels ("Masters (load first)" / "Regular plugins"). Reorder `<button>`s already accept `disabled` + `aria-label`.

---

## Spacing Scale (detected convention — additive elements reuse it)

The v1.0 UI uses a **rem-based scale (0.25rem = 4px base)**, not a strict 8-point grid. Additive elements MUST reuse existing values, not introduce new ones.

| Token | Value | Existing usage to reuse |
|-------|-------|-------------------------|
| row padding | `0.25rem 0.5rem` | `ol.priority li` |
| row min-height | `32px` | plugin/priority rows (reorder hit target) |
| badge padding | `0.05rem 0.35rem` | `.badge` |
| gap | `0.5rem` | row/toolbar flex gap |
| box padding | `0.5rem 0.75rem` | `.warn`, `.loot-proposal`, `.drift-notice` |

Exceptions: none new. (The badge/note sizes below are the existing `.badge` / `.muted` values.)

---

## Typography (detected — additive elements reuse it)

| Role | Size | Weight | Line height |
|------|------|--------|-------------|
| Body | 1rem (16px) | 400 | 1.4 |
| Section heading (h2) | 1.15rem (~18px) | 600 | inherit |
| Sub-heading (h3) | ~1rem, `.muted` suffix 0.85rem/400 | 600 | inherit |
| Small / meta / note | 0.85rem (~13.6px) | 400 (muted) | inherit |
| Badge | 0.7rem (~11px) | 600 | 1 |

Weights in use: **400 and 600 only.** Do not add new weights.

---

## Color (60/30/10 for this view)

| Role | Value | Usage |
|------|-------|-------|
| Dominant (60%) | `#fff` / `#f3f3f3` surfaces | section cards, row/badge backgrounds |
| Secondary (30%) | `#eee`/`#ccc` borders, `#777`/`#555` text | row separators, muted meta |
| Accent (10%) | `#0a66c2` | primary CTAs only (`Save plugin order`, `Apply sorted order`, `Re-check`) |
| Destructive | `#cf222e` | not used in this additive scope (no destructive action added) |

Accent reserved for: the existing `button.cta` primary actions only. **New badges and the masterlist note MUST NOT use accent blue** — protected/medium badges use their own neutral/amber tints below; the note is muted grey.

---

## Additive Component Specs

### 1. Protected-master row (SFLO-03)

Rows for libloot-determined protected base masters (`Starfield.esm`, `SFBGS0xx.esm`,
`BlueprintShips-*`, …). **Never hard-coded — the row's protected flag comes from the engine.**

**Data dependency:** `PluginInfo` (api.ts) gains a `protected: boolean` (name TBD by planner);
UI renders, never decides.

**Rendering (replaces the interactive controls, does not just disable them):**
- Reorder ▲▼: rendered **disabled** (`disabled` attr, already supported) — OR omitted in favour of a static lock glyph. Either way NOT operable.
- Toggle checkbox: rendered `disabled` and `checked` (protected masters are always active).
- A `.badge.badge-protected` with text **`PROTECTED`** + a lock glyph `🔒` (icon + text, never color-only).
- Row tint: neutral, reuse `li.disabled` muted treatment (`color: #777`) plus a subtle left border or `#f3f3f3` fill — no new hue.

**Accessibility (required):**
- The row and its status MUST be announced. Because a `disabled` button is not focusable, the lock affordance is a focusable, non-interactive element carrying the reason: `aria-label` / `title` = the tooltip copy below. Do not rely on a bare disabled control to convey "protected".
- Badge text `PROTECTED` present in the accessibility tree.

**States:** protected (locked, default) — there is no enabled/disabled toggle state for these rows.

---

### 2. Medium-master tier badge (SFLO-03)

CE2 medium-master plugins (classified via the esplugin/libloot header flag), visually
distinct from full masters (`ESM`/`ESL`) and regular (`ESP`).

**Data dependency:** a tier signal on `PluginInfo` (e.g. `tier: "full" | "medium" | "regular"`,
name TBD by planner) — libloot-derived, never a hard-coded name list.

**Rendering:** new `.badge.badge-medium` with text **`MEDIUM`**, distinct tint (recommend a
teal/amber-neutral that is NOT the esm-blue, esl-purple, or esp-grey already taken — e.g.
bg `#eaf3f0`, border `#9cccb9`, text `#1a6b52`). Text label carries the meaning; the tint only
separates it at a glance. Shown **in addition to** the kind badge, or replacing it per planner's
choice — but a medium master MUST be distinguishable from a full master at a glance and in text.

Medium masters order between full masters and regular plugins, per libloot — the existing
`group-divider` pattern MAY gain a "Medium masters" divider (reuse `li.group-divider`, no new style).

---

### 3. LOOT sort preview + apply (SFLO-02) — **REUSE v1.0 VERBATIM**

The propose-then-apply preview **already exists** and satisfies SFLO-02's no-silent-apply
requirement. Do NOT build a new one.

- Action: existing `Sort with LOOT` toolbar button → `onSortWithLoot` → `sortProposal`.
- Preview: existing `.loot-proposal` box — `ol.proposed` numbered list, `movedByLoot`
  highlight + `moved-tag`, LOOT warnings in a `.warn` block.
- Apply gate: existing `Apply sorted order` (`button.cta`) + `Discard` — applies into the
  editable list only on explicit click (D-12); still requires the existing `Save plugin order`
  to write `plugins.txt`.

**Starfield-specific:** protected masters appearing in the proposal are shown but their final
position is owned by libloot (they cannot be dragged out of place). No new preview affordance.

---

### 4. Masterlist-age note (SFLO-02)

A small, **non-alarming** note near the `Sort with LOOT` action.

- Placement: inside `.conflict-toolbar` (Starfield only), immediately after/below the Sort button.
- Style: `.muted` (grey `#777`, 0.85rem). **NOT amber `.warn`** — staleness is informational, not a caution. Reuse the existing muted-note pattern.
- Copy: `Masterlist from {date} — may be stale` (date from the bundled Phase-6
  `masterlist.yaml` commit/snapshot date, forwarded by the engine).
- Only rendered when `isStarfield` and a masterlist date is available.

**Data dependency:** the masterlist snapshot date must reach the UI (extend `SortProposal` or a
small status field — planner's call).

---

### 5. On-launch-rewrite reconciliation surfacing (SFLO-04)

In the verify/repair surface (§4 "Deploy & verify" report), Starfield needs a reconciliation
state that distinguishes the game's **expected on-launch rewrite** from a **real discrepancy** —
intent derived from recorded plugin state, not a raw on-disk diff.

**Two states (only the second is alarming):**

| State | When | Style | Copy |
|-------|------|-------|------|
| In sync | On-disk `plugins.txt` differs from recorded state ONLY by the known on-launch deltas (`.ccc` entries, stripped implicit ESMs, `BlueprintShips-*`) | **Calm/success** — reuse `.ok` green (`#1a7f37`) or the muted "Pristine" line pattern already in §4. No red, no scary diff. | `In sync — Starfield applied its expected on-launch changes.` |
| Real discrepancy | Deltas **beyond** the known rewrite | `.warn` amber box (existing) — lists the unexpected entries | `Load order changed unexpectedly` + the beyond-expected plugin names + the existing verify/repair guidance |

**Data dependency:** the engine must return a **classification** (expected-delta vs real-drift),
not a raw diff, so the UI never renders a raw `plugins.txt` diff as if it were corruption. The
existing `VerifyReport` shape is insufficient — needs a Starfield reconciliation field (planner's call).

**Must NOT** reuse the alarming `.err`/red or `.drift-notice` styling for the in-sync case.

---

### Dependency: Phase-6 first-launch gate (do not duplicate)

When `isStarfield && starfieldPending` (Ce2ConfigState = `FirstLaunchPending`), §5 already
renders the `.first-launch` gate ("Launch Starfield once before managing load order") and the
whole load-order body is blocked. All Phase-7 additive elements live in the `{:else}` branch and
are only reachable once CE2 state is `Ready`. **Do not add a second gate or re-render this
guidance.**

---

## Copywriting Contract

| Element | Copy |
|---------|------|
| Primary CTA (existing) | `Save plugin order` / `Apply sorted order` / `Sort with LOOT` |
| Protected badge | `PROTECTED` (+ `🔒` glyph) |
| Protected tooltip / aria-label | `Protected master — Starfield requires this and manages its position. It can't be reordered or disabled.` |
| Medium-master badge | `MEDIUM` |
| Medium-master tooltip / aria-label | `Medium master (CE2) — loads in the medium-master tier, between full masters and regular plugins.` |
| Masterlist-age note | `Masterlist from {date} — may be stale` |
| Reconciliation — in sync | `In sync — Starfield applied its expected on-launch changes.` |
| Reconciliation — discrepancy | `Load order changed unexpectedly` (+ unexpected plugin names + existing repair guidance) |
| Empty state (existing, unchanged) | `No plugins found` / `No .esp/.esm/.esl files in the enabled mods or game Data folder…` |
| Destructive confirmation | none added in this scope |

---

## Registry Safety

| Registry | Blocks Used | Safety Gate |
|----------|-------------|-------------|
| none | not applicable (no component registry; hand-rolled CSS) | not required |

---

## Acceptance Criteria

- [ ] Protected rows render a `PROTECTED` badge **with text + lock icon** (not color-only) and have their reorder ▲▼ and toggle non-operable.
- [ ] Protected status is reachable in the accessibility tree (focusable element or aria-label carrying the tooltip reason) — not conveyed by a bare disabled control alone.
- [ ] Medium-master badge renders `MEDIUM` text and is visually + textually distinct from `ESM`/`ESL`/`ESP`; its tint is NOT accent-blue.
- [ ] The LOOT sort still requires an explicit `Apply sorted order` after preview (reuses `.loot-proposal`; no silent auto-apply).
- [ ] Masterlist-age note appears near the Sort action in **muted grey (not amber/red)**, with copy `Masterlist from {date} — may be stale`.
- [ ] Reconciliation "in sync" state uses calm/success styling (green/muted, no red, no raw diff); only the beyond-expected-delta case uses amber `.warn`.
- [ ] No new hex values, fonts, weights, or spacing tokens beyond those in the detected tables; light-only (no theming introduced).
- [ ] All additive elements live behind the existing Phase-6 first-launch gate; no duplicate gate added.

---

## Checker Sign-Off

- [ ] Dimension 1 Copywriting: PASS
- [ ] Dimension 2 Visuals: PASS
- [ ] Dimension 3 Color: PASS
- [ ] Dimension 4 Typography: PASS
- [ ] Dimension 5 Spacing: PASS
- [ ] Dimension 6 Registry Safety: PASS

**Approval:** pending
