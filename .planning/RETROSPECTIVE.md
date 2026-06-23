# Retrospective

Milestone retrospectives for NexTwist. Newest first. Each section is written at
`/gsd-complete-milestone` and is grounded in the milestone's phase artifacts
(SUMMARY / VERIFICATION / UAT / SECURITY / MILESTONE-AUDIT).

---

## Milestone: v1.0 — MVP

**Shipped:** 2026-06-23
**Scope:** 5 phases / 21 plans / 26 tasks · ~196 commits · 2026-06-20 → 2026-06-23
**Size:** ~20k Rust LOC (7 headless crates + thin Tauri shell) + ~2.9k frontend LOC (SvelteKit/Svelte 5 SPA)
**Outcome:** All 40 v1 requirements satisfied. Milestone audit: `tech_debt` (0 critical blockers; 6/6 integration seams WIRED; 7/7 E2E flows intact). Core value held — non-destructive, byte-for-byte reversible, conflict-aware deployment.

### What Was Built

- **Phase 1 — Safe Local Round-Trip:** the crown-jewel reversible deployment engine — multi-crate headless workspace (`store`/`steam`/`extract`/`deploy`/`core`/`testkit`), WAL SQLite persistence (manifest + intent-before-act operation journal + content-addressed vanilla backup ledger), a per-target `reflink → hardlink → symlink → copy` method ladder with EXDEV fallback, crash-safe journaled deploy/purge with launch-time recovery, verify/repair drift detection, case-folding for Wine, and a thin Tauri 2.11 + Svelte 5 shell driving the full detect → install → deploy → purge round-trip. (Walking Skeleton closed.)
- **Phase 2 — Multi-Mod Management:** file-level conflict detection + deterministic rank-based winner resolution, plugin scan/enable/reorder with masters-first asterisk `plugins.txt` written at the Proton-prefix AppData location via `libloot`, LOOT masterlist fetch + propose→review→apply sort, and per-game profiles (create/switch/delete) that reconcile on-disk deployment through the existing journaled engine — pristine round-trip regression-locked across profile switches.
- **Phase 3 — NexusMods Login & Download:** a headless rustls-only `nexus` crate — OAuth2 Authorization-Code + PKCE (S256) login with an API-key-paste fallback, keyring storage with a hard-fail-no-plaintext invariant, a hybrid REST-v1/GraphQL-v2 client, a `governor` token-bucket rate limiter reading `X-RL-*` headers, streaming (never full-buffer) downloads with progress, and a strict `nxm://` deep-link parser + single-instance forwarding so website "Mod Manager Download" one-clicks route to the live app and auto-extract into staging.
- **Phase 4 — Guided Installers & Collections:** a headless `fomod` engine (full `ModuleConfig.xml` AST + dependency/flag evaluator + pure dry-run resolver), a step-by-step FOMOD wizard with a non-skippable conflict-preview gate, and the Collection lifecycle (parse → availability-resolve hard gate → bulk download → FOMOD-choice replay → rule→rank mapping → deploy → byte-for-byte reversible uninstall) composed entirely from existing Phase-1/2/3 primitives.
- **Phase 5 — AppImage Distribution:** a tagged-release GitHub Actions workflow producing a license-clean AppImage via `tauri-action`, a non-fatal `nxm://` registration self-test wired through the plugin's own API (no desktop-file-name drift), a regenerated ≥128×128 icon set, and a `cargo deny` + bundled-binary distribution audit (rustls-only, no app-path OpenSSL, UnRAR absent, WebKitGTK from host) — verified on real hardware.

### What Worked

- **Headless-engine / thin-adapter split paid off immediately.** The entire safety-critical engine lives in `crates/*` with zero Tauri deps, so the round-trip, crash-recovery, and pristine-tree guarantees were unit/property-testable in CI without a webview. Every later phase (downloads, FOMOD, Collections) plugged into the *unchanged* deploy path rather than forking it — Collections in particular shipped as pure orchestration over existing primitives.
- **Intent-before-act journal + idempotent file ops** gave a real, tested crash-safety story: the `round_trip_pristine` proptest and a dedicated crash-recovery test anchored the reversibility contract, and `recover_on_launch` was proven to replay interrupted operations before the UI is served.
- **De-risking the riskiest unknown early.** Phase 2 Plan 02 stood up and *proved* the `libloot` Linux seam against a fixture Proton prefix before building the plugin manager on top of it — so the plugin/LOOT slice was mechanical, not exploratory.
- **Reuse over re-implementation at every seam.** Free-user `nxm://` redemption shares the exact stream→extract→stage core as the IPC download command; Collection FOMOD replay builds the *same* `fomod::Selection` the interactive wizard produces. No parallel paths to drift.
- **Supply-chain discipline as a build gate.** `cargo deny` banning the non-free UnRAR source was load-bearing from Phase 1 and gave Phase 5's license audit a clean, reproducible pass.
- **Per-phase threat models verified at close.** Four of five phases carry a SECURITY.md with `threats_open: 0` (24 + 18 + 17 + 8 threats closed); the safety-critical engine and untrusted-input boundaries (zip-slip, `nxm://` parsing, keyring) were threat-modeled rather than assumed.

### What Was Inefficient

- **A correctness bug surfaced only at in-game UAT, not in CI.** The install-archive double-nesting bug (non-game wrapper siblings leaking into the staged mod root) was caught at hardware testing and fixed in `2fa9821`. The automated corpus didn't model the wrapper-folder shape that real mods use.
- **The pristine proptest was blind to empty directories.** GAP-01 (purge leaving 3 orphan empty dirs) was a real Skyrim SE repro that the `round_trip_pristine` proptest missed because `testkit::snapshot_tree` hashed file *contents* only. Fixed in Plan 01-07 by making the snapshot directory-aware (`DIR_SENTINEL`) — but it shipped a phase late because the assertion under-specified "pristine."
- **A hard external constraint was discovered late.** NexusMods restricts Collection-archive download to its own Vortex client, so live Collection ingest from nexusmods.com is **not possible** for a third-party client. The headless Collection engine (apply pinned choices + load order, deploy, reversible uninstall) is fully verified on an already-fetched manifest, but the live download seam (`collectionRevision.downloadLink`) was intentionally left unimplemented once the policy was understood. Surfacing this earlier would have shaped Phase 4's success criteria.
- **Verification/UAT statuses went stale before milestone close.** Several phases sat at `human_needed` / `testing` after their UAT had actually been run and passed on hardware; the audit had to reconcile stale frontmatter against the real outcomes (Phases 1 and 2 were re-stamped `human_verified: 2026-06-23` at close). The single source of truth drifted from the recorded status.
- **SUMMARY `requirements_completed` frontmatter was under-populated**, so per-requirement traceability lived authoritatively in the VERIFICATION.md tables rather than the summaries — a documentation-traceability gap (cosmetic, but it made the audit cross-reference more manual).

### Patterns Established

- **Headless engine + thin adapter:** all logic in `crates/*` (no `tauri`/`reqwest`/UI types); Tauri command adapters are 3–5 lines that lock `AppState` and delegate. Engine APIs speak `core` types only — no `rusqlite` type ever appears in `store`'s public API.
- **Intent-before-act operation journal:** write `pending` before the syscall, flip to `done` after, with idempotent file ops so replaying a half-finished op after a crash is always safe; `recover_on_launch` runs before the UI is served.
- **Manifest/journal-derived cleanup sets, never blind disk scans** — purge/verify operate from recorded state, bounded to the deploy root.
- **De-risk the unknown in its own plan** before building on it (the libloot seam).
- **One core path per capability** — extract the shared core so alternate entry points (IPC vs `nxm://`, wizard vs Collection replay) cannot diverge.
- **Per-phase threat model verified against implemented code at phase close** (`/gsd-secure-phase` → SECURITY.md with `threats_open: 0`), not just at design time.
- **TDD RED→GREEN gate with cited commit pairs** (e.g. 02-05 RED `e914ffe` → GREEN `7479652`) as the execution rhythm.

### Key Lessons

- **A "pristine" assertion is only as strong as what it hashes.** GAP-01 hid behind a file-content-only snapshot. When the guarantee is byte-for-byte tree restoration, the test must cover directories, permissions, and empty nodes — model the full invariant, not the convenient subset.
- **Build the test corpus from real-world archive shapes, not idealized ones.** Both the double-nesting bug and the wrapper-folder gap came from real mods having structure the synthetic fixtures didn't. A handful of actual Nexus archives in the corpus would have caught these before hardware UAT.
- **Validate external-platform assumptions before committing a phase's success criteria.** The Vortex-only Collection-download policy was a load-bearing constraint discovered mid-build; a short spike against the real Nexus endpoints up front would have reframed COLL-02 from the start.
- **Keep recorded status in lockstep with reality, or the audit pays the cost.** Stale `human_needed`/`testing` frontmatter forced manual reconciliation at close. Stamp UAT outcomes back into VERIFICATION frontmatter the moment they're observed.
- **The headless/thin-adapter boundary is the project's highest-leverage decision** — it is what made the engine CI-testable and let four downstream phases compose rather than re-implement. Defend it.

### Cost Observations

- **5 phases / 21 plans / 26 tasks delivered in ~3 days** (2026-06-20 → 2026-06-23), ~196 commits. Individual plans were small and fast (Phase 3/4/5 plan SUMMARYs report ~2–40 min execution windows each), reflecting the wave-based, vertical-slice execution model.
- **Roughly 20k Rust + 2.9k frontend LOC** for a full reversible mod manager — the thin-adapter discipline kept the Tauri/UI surface small relative to the engine.
- **Multi-agent verification/security/debug was concentrated at milestone close:** retroactive `/gsd-secure-phase` threat verification across phases (4 SECURITY.md, ≥67 threats closed), an integration checker confirming 6/6 seams + the 33↔33 IPC contract, and a milestone audit cross-referencing 40/40 requirements across three sources. The bulk of the late-stage cost was *verifying* the engine, not changing it.
- **Hardware-dependent UAT is an irreducible cost.** A real display + Steam/Proton + a Bethesda game (+ a Premium Nexus account for some paths) gate the in-game and live-download tests; these cannot be run autonomously and remain the milestone's standing manual-test surface.

---
*Retrospective started: 2026-06-23 at v1.0 milestone close.*
