---
phase: 8
slug: reversible-starfieldcustom-ini-activation
status: draft
shadcn_initialized: false
preset: none
created: 2026-07-08
---

# Phase 8 — UI Design Contract

> Visual and interaction contract for the reversible `StarfieldCustom.ini` activation UI.
>
>
> **Prime directive: extend, do not invent.** NexTwist already ships a complete
> hand-rolled design system (v1.0, extended in Phases 6–7). The three Phase-8 surfaces
> below are assembled entirely from existing classes in
> `frontend/src/routes/+page.svelte`. This spec catalogues the established tokens so the
> planner/executor reuse them verbatim — no new palette, no new spacing scale, no new
> component library.

---

## Design System

| Property | Value |
|----------|-------|
| Tool | none (hand-rolled, no component library) |
| Preset | not applicable |
| Component library | none — plain Svelte 5 + scoped `<style>` in `+page.svelte` |
| Icon library | none — text/Unicode glyphs (status dots `●`, arrows) only |
| Font | `system-ui, sans-serif`; mono `ui-monospace, monospace` |
| Styling approach | single scoped `<style>` block, literal hex + `rem`; **no CSS custom properties** |

**shadcn gate:** not applicable — this is a SvelteKit/Svelte 5 static SPA, not React/Next/Vite-React. No registry, no `components.json`. Registry safety gate does not apply.

---

## Spacing Scale

The established scale is `rem`-based. All values below are multiples of 4px (checker-compliant). Reuse these; do not introduce new steps.

| Token | rem | px | Usage (existing) |
|-------|-----|-----|------------------|
| xs | 0.25rem | 4px | Row gaps, badge padding, tight stacks |
| sm | 0.5rem | 8px | Flex `gap`, notice padding-y, control margins |
| — | 0.75rem | 12px | Notice/box padding, modal action gaps, `pre` padding-x |
| md | 1rem | 16px | Section padding, modal body |
| — | 1.25rem | 20px | Section `margin-bottom`, modal padding |
| lg | 1.5rem | 24px | `main` padding |

**Min-height / hit-target exceptions (already in the codebase, reuse as-is):**
- Interactive list rows (reorder / option rows): `min-height: 32px` (`ol.priority li`) and `40px` (`.fomod-option`, `.download-row`).
- Dense table cells: `padding: 4px 8px` (`table.conflicts`).
- Progress/track bars: `height: 8px`.

Phase-8 additions: **none.** The activation status line, preview modal, and conflict box use the existing `.first-launch` / `.drift-notice` / `.overlay`+`.modal` / `pre.txt-preview` padding verbatim.

---

## Typography

Established roles (system font stack). Exactly **2 weights** and **4 sizes** — do not add more.

| Role | Size | Weight | Line Height | Existing selector |
|------|------|--------|-------------|-------------------|
| Display (H1) | 24px (1.5rem) | 600 | 1.2 | `h1` |
| Heading (H2/H3) | 18px (1.15rem) | 600 | 1.2 | `h2`, section headers |
| Body | 16px (1rem) | 400 | 1.4 | `main` default |
| Label / meta | 14px (0.875rem) | 600 | 1.4 | `.dl-meta`, `.tier`, `.rate-notice` |

**Weights:** 400 (regular) + 600 (semibold). No others.
**Exception (existing):** micro-type at 11–12px (`0.7–0.75rem`) is used only inside `.badge` / `.type-tag` chips. Phase 8 reuses this for the activation-state tag; do not use it for body copy.

---

## Color

The established 60/30/10 split (literal hex, no tokens). Reuse exactly.

| Role | Value | Usage |
|------|-------|-------|
| Dominant (60%) | `#ffffff` | Page + card/section background, modal background |
| Secondary (30%) | `#f3f3f3` | Info boxes (`.first-launch`, `.empty`, `.confirm`), code/`pre` fills, disabled/muted rows; borders `#ccc` / `#eee` |
| Accent (10%) | `#0a66c2` | see reserved list below |
| Destructive | `#cf222e` | Delete/error only — **not used in Phase 8** (activation is fully reversible) |

**Semantic status colors (existing, reuse):**
| Semantic | Text | Background | Border | Phase-8 use |
|----------|------|-----------|--------|-------------|
| Success (active/ok) | `#1a7f37` | `#eaf6ed` | `#1a7f37` | "Loose-file loading active" state |
| Warning / caution | `#9a6700` | `#fff8e5` | `#e6c200` | Conflict box (existing `sResourceDataDirsFinal`); the "Use NexTwist's value" caution |
| Muted / inactive | `#777` | `#f3f3f3` | `#ccc`/`#eee` | "Not active" state, provenance sub-text |

**Accent reserved for (explicit — do NOT broaden):** the primary login CTA; the Deploy CTA when changes are pending; the active-profile indicator/dot and active-profile row border; the download progress-bar fill; the winner-provider dot; the ESM badge. **Phase 8 adds exactly one accent element:** the modal confirm button that commits the INI write (`button.cta`, styled identically to Deploy). Everything else in Phase 8 is neutral or a semantic status color — never accent.

---

## Component Inventory (all reused — zero new components)

| Phase-8 surface | Existing class / pattern to reuse | Source reference |
|-----------------|-----------------------------------|------------------|
| Activation status line | `.first-launch` (neutral info box) + a `.badge`-style state tag; success tag reuses `.ok`/`#1a7f37`, inactive reuses `.muted`/`#777` | `.first-launch`, `.badge`, `.ok`, `.muted` |
| "Enable loose-file loading" control | plain `button` (neutral); it opens the preview modal (does NOT write) | `button` base |
| No-silent-edit preview + confirm | `.overlay` + `.modal` confirmation dialog (the exact profile-switch / FOMOD dry-run gate discipline) | `.overlay`, `.modal`, `onConfirmSwitch` / `fomodShowPreview` flow |
| Exact-change display (the `[Archive]` lines) | `pre.txt-preview` (grey monospace block — same as the `plugins.txt` preview) | `pre.txt-preview` |
| Provenance line ("will create" vs "will edit existing") | `.muted` sub-text inside the modal | `.muted` |
| Commit-write button (in modal) | `button.cta` (accent primary — identical to Deploy) | `button.cta` |
| Cancel | plain neutral `button` | `button` base |
| Conflict block (non-empty user value) | `.warn` amber box (blocking, non-destructive caution) | `.warn`, `.drift-notice` |
| Conflict user-value display | `code` / `pre.txt-preview` showing the user's current `sResourceDataDirsFinal` | `code`, `pre.txt-preview` |
| Conflict choice buttons | "Keep mine" = plain neutral `button` (safe default); "Use NexTwist's value" = neutral `button` inside the amber caution box (NOT red — reversible) | `button` base |
| Result / status readout | `.report` region + `.ok`/`.err`/`.busy` text | `.report`, `.ok`, `.err`, `.busy` |

**Placement:** all three surfaces live on the Starfield game view, in the same stack as the Phase-6 first-launch / drift notices and the Phase-7 masterlist-age note, gated on `isStarfield` (existing `$derived`). Never shown for non-Starfield games.

---

## Interaction Contract (the three surfaces)

### 1. Activation state + control
- **State indicator** always visible on the Starfield view once CE2 is `Ready` (not `FirstLaunchPending`):
  - Active → success tag: `● Loose-file loading active`.
  - Inactive → muted tag: `○ Loose-file loading not active`.
- **Control:** an "Enable loose-file loading" button. Clicking it **never writes** — it opens the preview modal (surface 2). Per locked CONTEXT the write itself also rides the deploy choke point automatically; the button is the explicit user-initiated path and both routes must pass through the same no-silent-edit preview before any byte is written.

### 2. Reversible-change preview + confirm (SFINI-01 — no silent edit)
- Opens as an `.overlay`+`.modal` dialog **before** any write.
- Shows, in a `pre.txt-preview` block, the exact literal lines to be added under the `[Archive]` section:
  ```
  [Archive]
  bInvalidateOlderFiles=1
  sResourceDataDirsFinal=
  ```
- Shows provenance as a `.muted` line, one of:
  - "NexTwist will **create** StarfieldCustom.ini (it does not exist yet)."
  - "NexTwist will **edit your existing** StarfieldCustom.ini — only the two lines above are added; every other section, key, comment, and line ending is preserved."
- Buttons: `button.cta` **"Activate loose-file loading"** (commits) + neutral **"Cancel"**.
- The confirm is the ONLY thing that authorizes the write. Mirrors the v1.0 FOMOD dry-run gate and the profile-switch confirm modal.

### 3. Conflict surfacing (SFINI-03 — never silently clobber)
- Trigger: the user's existing `StarfieldCustom.ini` already has a **non-empty** `sResourceDataDirsFinal`.
- Activation **blocks**; render an amber `.warn` box (not red — the action is reversible) instead of the normal preview:
  - Heading: "StarfieldCustom.ini already sets a loose-file path."
  - Show the user's current value verbatim in a `code`/`pre.txt-preview` block.
  - Two explicit choices, no default write:
    - **"Keep mine"** (neutral, safe) — leaves the user's value untouched; loose-file activation stays off / uses their value.
    - **"Use NexTwist's value"** (neutral, inside the caution box) — replaces the value; NexTwist records the prior value so purge restores it byte-for-byte.
- Never auto-resolve. Only an empty/absent value auto-writes (through surface 2).

**Out of scope for this phase's UI (do not design):** the deployed-vs-actually-loaded-in-game distinction (SFVER-02 → Phase 9). Phase 8 shows INI *activation* state only.

---

## Copywriting Contract

| Element | Copy |
|---------|------|
| Primary CTA (opens preview) | **Enable loose-file loading** |
| Modal confirm (commits write) | **Activate loose-file loading** |
| Modal cancel | Cancel |
| State — active | ● Loose-file loading active |
| State — inactive | ○ Loose-file loading not active |
| Preview — provenance (create) | NexTwist will create StarfieldCustom.ini (it does not exist yet). |
| Preview — provenance (edit) | NexTwist will edit your existing StarfieldCustom.ini — only these two lines are added; every other section, key, comment, and line ending is preserved. |
| Empty / gated state | Loose-file loading isn't set up yet. Deploy a mod, or enable it here, to let Starfield load loose files. (When `FirstLaunchPending`, defer to the existing Phase-6 first-launch guidance instead — launch Starfield once.) |
| Error state | Couldn't update StarfieldCustom.ini: {reason}. Your game files are unchanged — retry, or check the config path under Documents/My Games/Starfield. |
| Conflict heading | StarfieldCustom.ini already sets a loose-file path. |
| Conflict body | Your file sets `sResourceDataDirsFinal` to: {current value}. NexTwist won't overwrite it silently. Keep your value, or replace it with NexTwist's (reversible — your original is restored on purge). |
| Conflict choice A | Keep mine |
| Conflict choice B | Use NexTwist's value |

**Destructive actions:** none in this phase. Every write is journaled and byte-for-byte reversible (provenance restore / restore-absence), so no red `button.destructive` and no "this cannot be undone" language. The conflict "Use NexTwist's value" path is a **caution** (amber), not a destructive confirm.

---

## Registry Safety

| Registry | Blocks Used | Safety Gate |
|----------|-------------|-------------|
| none | none — no component registry in use | not applicable (no shadcn / no third-party registry) |

---

## Checker Sign-Off

- [ ] Dimension 1 Copywriting: PASS
- [ ] Dimension 2 Visuals: PASS
- [ ] Dimension 3 Color: PASS
- [ ] Dimension 4 Typography: PASS
- [ ] Dimension 5 Spacing: PASS
- [ ] Dimension 6 Registry Safety: PASS

**Approval:** pending
