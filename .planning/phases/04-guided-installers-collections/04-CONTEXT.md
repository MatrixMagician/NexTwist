# Phase 4: Guided Installers & Collections - Context

**Gathered:** 2026-06-21
**Status:** Ready for planning
**Mode:** Smart discuss (autonomous) — 16 decisions across 4 areas; 2 user overrides (full FOMOD spec; Premium-only Collections)

<domain>
## Phase Boundary

This phase adds the two highest-complexity acquisition/install features on top of the proven Phase 1-3 engine (staging → conflict/priority → profiles → safe deploy/purge-to-pristine, + the Phase-3 NexusMods download client). A user can:

1. **Install a mod through a guided FOMOD option wizard** — parse the mod's `fomod/ModuleConfig.xml`, present a step-by-step wizard, and apply the conditional/option-driven file selection correctly into staging (FOMOD-01, FOMOD-02).
2. **Browse and select a NexusMods Collection** for a managed game and **download all pinned mods** in the chosen revision per its manifest, **resolving/reporting archived or unavailable mods before touching disk** (COLL-01, COLL-02).
3. **Automatically apply the Collection's FOMOD choices, load order, and rules** — replaying each mod's recorded FOMOD selections headlessly and mapping the Collection's rules onto the existing conflict/priority + plugin load-order model (COLL-03).
4. **Deploy an installed Collection so the modded game launches, and cleanly + reversibly uninstall the whole Collection** restoring the game to pristine (COLL-04, COLL-05).

**Requirements covered:** FOMOD-01..02, COLL-01..05 (7 requirements).

**Explicitly out of scope for this phase:** AppImage packaging + license audit (Phase 5); Collection *authoring/publishing* (COLLV2-01, v2); mod-update notifications (NEXV2-01); a free-user (non-Premium) bulk-Collection download path (**deferred — Collections are Premium-only in v1 per the override below**); fetching off-Nexus/externally-hosted dependencies (detect + surface only).

</domain>

<decisions>
## Implementation Decisions

### FOMOD Engine & Parsing (FOMOD-01/02) — USER OVERRIDE: full spec
- **Implement the FULL FOMOD `ModuleConfig.xml` specification**, not a core subset (user override). This includes: install steps + ordering, all option group types (`SelectExactlyOne`, `SelectAtMostOne`, `SelectAtLeastOne`, `SelectAll`, `SelectAny`), option type descriptors and **conditional type states** (`type` vs `typeDescriptor`/`dependencyType` — Optional/Required/Recommended/NotUsable/CouldBeUsable), **flag set/conditions**, the full **`conditionalFileInstalls`** pattern engine, and **composite dependency operators** (`And`/`Or`, nested, `fileDependency`/`flagDependency`/`gameDependency`/`fommDependency`). Genuinely malformed/unsupported constructs fail with a clear, specific error rather than silently mis-installing. (Plan-phase research must enumerate the spec + edge cases; this is the single largest technical surface in the phase.)
- **The FOMOD engine lives in a new headless `crates/fomod` crate** (Tauri-free, pure): parse `ModuleConfig.xml` → expose the ordered steps/groups/options + their visibility/type conditions → given a set of user choices (+ accumulated flags), **resolve the concrete file-install plan**. The wizard UI lives in the shell; no FOMOD logic in the adapter.
- **XML parsing uses a pure-Rust crate** (`quick-xml` with serde, or `roxmltree`) — no native dependency, rustls/AppImage-friendly, cargo-deny-clean. Exact crate confirmed by plan research; FOMOD XML is case/whitespace/namespace-quirky, so the parser choice + a corpus of real `ModuleConfig.xml` samples is a research task.
- **Dry-run-resolve-then-apply is a hard safety gate**: resolve the full file-install plan (which staged source files land at which destinations, incl. conditional installs) and surface any conflicts **before touching staging**, then apply. Mirrors the STATE Phase-4 blocker ("dry-run resolve before touching disk") and the project safety ethos. Applying still routes every file through the validated staging/extract path; the round-trip-pristine guarantee is untouched.

### FOMOD Wizard UX (FOMOD-01)
- **Step-by-step wizard** with Back/Next: one install step per screen. `SelectExactlyOne`/`SelectAtMostOne` render as **radio** groups; `SelectAny`/`SelectAtLeastOne`/`SelectAll` as **checkbox** groups, honoring min/max and required/notusable states.
- **Each option shows its image + description** loaded from the staged archive (FOMOD authors rely on these to guide choices). Missing image degrades gracefully to text.
- **Live conditional re-evaluation**: as choices set/unset flags, step + option **visibility and type state** (e.g. an option becoming Required/NotUsable) re-evaluate live, and the resolved conditional file installs update.
- **Re-installing the FOMOD installer re-stages** with fresh choices (replace/new staged version); editing choices in place on an already-staged mod is deferred (nice-to-have).

### Collections — Download & Resolve (COLL-01/02) — USER OVERRIDE: Premium-only
- **Parse the NexusMods Collection revision manifest** (the collection's pinned mod list — game, mod id, file id, version — plus per-mod FOMOD choices, load order, rules, and any bundled config/patch files). Exact manifest/bundle/patch format is the second-largest unknown and a dedicated plan-research item (flagged in STATE as "known recurring bugs").
- **Resolve the FULL manifest first and report archived / unavailable / off-Nexus mods BEFORE any download or disk write** (success criterion 2). The user sees what's missing and can proceed with the available set + manual notes for the rest. No partial disk mutation before the user accepts the resolution report.
- **Collections are Premium-only in v1 (user override).** Bulk in-app Collection download requires a Premium NexusMods account (the API direct-download path from Phase 3). A free (non-Premium) user attempting a Collection gets a clear **"Collections require a NexusMods Premium account"** notice — there is **no** per-mod free-user `nxm://` fallback for Collections this phase (that path is deferred). (Single-mod free-user `nxm://` from Phase 3 is unaffected.)
- **Download orchestration reuses the Phase-3 client + the single shared `governor` rate limiter** (WR-03), with bounded concurrency and per-mod + overall progress. Off-Nexus / externally-hosted dependencies (script extenders, etc.) are **detected and surfaced as required manual steps**, never auto-fetched.

### Collections — Apply, Deploy, Uninstall (COLL-03/04/05)
- **A Collection installs into its own dedicated Phase-2 profile** — isolated, switchable, and cleanly removable. Each pinned mod's **FOMOD choices are replayed headlessly from the manifest** through the `crates/fomod` resolver (no interactive wizard per mod during a Collection install).
- **The Collection's rules map onto the existing Phase-2 conflict/priority + plugin load-order model** — no new parallel rules engine. "X loads after Y" / explicit file overrides translate to mod rank + load order; the deterministic winner resolution from Phase 2 applies.
- **Deploying a Collection = activating its profile + deploy-winners + apply-load-order via the existing Phase-2 profile-switch path** — no new deploy primitive. The Collection launches the modded game through the same safe engine.
- **Uninstalling a Collection is fully reversible**: a Collection = its profile + its staged mods + their Nexus provenance. Uninstall = **purge-to-pristine via the deploy engine** + drop the profile + remove the Collection's staged mods, leaving the game byte-for-byte vanilla (reuse the Phase-1/2 guarantee + testkit pristine harness as the regression check).

### Claude's Discretion
- Exact `crates/fomod` module split, the chosen XML crate, the precise V5 (or later) store schema for Collection + FOMOD-choice persistence, the manifest/bundle/patch parsing details, and the wizard component structure are at Claude's discretion, to be settled by plan-phase research (FOMOD spec corpus + a real Collection revision).

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`crates/extract`** + the Phase-1 staging path — FOMOD installs and Collection mods stage through the same validated extract→staging pipeline (zip-slip/symlink defense unchanged).
- **`crates/nexus`** (Phase 3) — the OAuth/API-key auth, the hybrid REST-v1/v2 client, **`download_link` + the REST-v1 file-info metadata** (note: v2 GraphQL `modFile` does NOT exist — use v1, per bug 21f6784), the shared `governor` `RateLimiter` in `AppState`, and the streaming download core (`run_download_to_window`). Collection bulk download reuses this verbatim. `appid_for_domain` (skyrimspecialedition=489830, fallout4=377160) is the shared domain→appid map.
- **`crates/store`** — rusqlite (bundled 0.39) + refinery; current highest migration is **V4** (`V4__nexus_provenance.sql`). Phase 4 adds a **V5+ migration** for Collection records + per-mod FOMOD-choice persistence. Hard invariant: no rusqlite type in the public API.
- **Phase-2 profiles + conflict/priority + plugin load order** (`crates/store/src/profiles.rs`, the conflict resolver, `crates/loadorder`) — a Collection is modeled as a profile; its rules map to these.
- **Phase-1/2 deploy/purge-to-pristine + `crates/testkit`** — Collection deploy = profile-switch deploy; Collection uninstall = purge-to-pristine, asserted via the testkit blake3 pristine harness.
- **Tauri command + Svelte UI patterns** — thin adapters in `src-tauri/src/commands/*`; Phase 4 adds `commands/{fomod,collections}.rs` + a FOMOD wizard view and a Collections browse/resolve/progress view (`frontend/src/`). UI hint: yes → UI-SPEC generated before planning.

### Established Patterns
- Headless engine crates with ZERO Tauri deps; thin command adapters; `thiserror` in libs / `anyhow` at the boundary; `reqwest` rustls-only; `cargo-deny` load-bearing (new deps — the XML crate — must pass). New crate `crates/fomod` follows the `crates/nexus`/`crates/loadorder` shape (own `error.rs`).
- Additive refinery migrations `V*.sql`; `core` model types extended additively.
- The round-trip-pristine guarantee is the invariant every new write-path must preserve (FOMOD apply, Collection install/deploy/uninstall all assert it).

### Integration Points
- New crate `crates/fomod` (parse + resolve); new `crates/store` V5+ migration + query module for Collections/FOMOD choices; new core model types.
- New shell adapters `commands/{fomod,collections}.rs`; FOMOD wizard + Collections views in the frontend; Collection install wires through Phase-3 download + Phase-2 profile/conflict/load-order + Phase-1/2 deploy/purge.

</code_context>

<specifics>
## Specific Ideas

- **FOMOD full-spec is the largest single technical surface** — plan research must gather a corpus of real `ModuleConfig.xml` files (incl. tricky conditional/dependency ones) and pick an XML crate that survives their quirks. Dry-run resolve is the safety net before any staging write.
- **The Collection manifest/bundle/patch format is the second-largest unknown** and is STATE-flagged as having "known recurring bugs" — resolve + report the full manifest before touching disk; a real Collection revision is the research fixture.
- **Collections are Premium-only in v1 (user override)** — design the download path to require Premium and show a clear notice otherwise; do not build the free-user per-mod fallback for Collections this phase.
- **Maximum reuse, minimum new primitives** — FOMOD and Collections are orchestration on top of Phases 1-3; the only genuinely new engine code is the `crates/fomod` parser/resolver and the Collection manifest resolver. Deploy/purge/profiles/conflict/download are all reused.
- **Reversibility is in-game-observable and the hard UAT** — installing then uninstalling a Collection must leave the game pristine (testkit assertion + a manual deploy→launch→purge UAT analogous to prior phases).

</specifics>

<deferred>
## Deferred Ideas

- **Free-user (non-Premium) bulk Collection download** (per-mod `nxm://` orchestration) — deferred; v1 Collections are Premium-only.
- **Edit-FOMOD-choices-in-place** on an already-staged mod — v1 re-installs to change choices.
- **Collection authoring/publishing** (COLLV2-01) — v2.
- **Auto-fetching off-Nexus dependencies** — detect + surface as manual steps only.
- **Mod-update / Collection-revision-update tracking** (NEXV2-01) — v2.

</deferred>
