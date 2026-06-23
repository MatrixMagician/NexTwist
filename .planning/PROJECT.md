# NexTwist

## What This Is

NexTwist is a Rust + Tauri desktop application that brings Vortex/Mod-Organizer-2-class mod management to Linux gamers. It lets users log into NexusMods, download and install individual mods (including FOMOD scripted installers and curated Collections), and manage them for Windows PC games that run on Linux via Steam Proton / Wine — with safe, fully reversible deployment. It is for Linux gamers (and modding power users migrating from Windows) who want a first-class mod manager that "just works" on their platform.

**Shipped in v1.0 (2026-06-23):** Steam/Proton game detection (Skyrim SE, Fallout 4); a reversible deployment engine (non-destructive deploy, byte-for-byte pristine purge, crash-safe journaled recovery, verify/repair); file-level conflict resolution, plugin load order via libloot/LOOT, and per-game profiles; NexusMods OAuth2/API-key login with secure keyring storage, in-app Premium downloads and `nxm://` one-click handoff; guided FOMOD installers; the Collection lifecycle (apply pinned choices + load order + deploy + reversible uninstall, over a fetched manifest); and a license-clean Linux AppImage as the distribution channel.

## Core Value

Mods must install and uninstall **safely**: deployment is non-destructive (the base game install is never directly corrupted), fully reversible (any mod or the whole load order can be removed leaving the game pristine), and conflict-aware (the user always knows and controls which mods overwrite which files). If everything else fails, this must hold.

## Requirements

### Validated (v1.0 — shipped 2026-06-23, all 40 v1 requirements satisfied)

- ✓ Game & environment detection — v1.0 (Steam/Proton auto-detect, install-dir + prefix resolution, Bethesda managed games, unsafe-FS warnings; ENV-01..04)
- ✓ Mod staging & extraction — v1.0 (local .zip/.7z install, zip-slip rejection, .rar via system tool with no non-free code bundled; STAGE-01..03)
- ✓ Reversible deployment engine (core safety) — v1.0 (non-destructive deploy, per-game ledger, byte-for-byte pristine purge, backup-before-overwrite, reflink→hardlink→symlink→copy ladder, crash-safe journal, verify/repair, Wine case-folding; DEPLOY-01..08)
- ✓ Conflicts & load order — v1.0 (file-level conflict view, rank-based winner choice, deterministic deploy; CONF-01..03)
- ✓ Plugin management — v1.0 (enable/disable plugins, plugins.txt at the Proton-prefix AppData location, LOOT auto-sort; PLUGIN-01..03)
- ✓ Profiles — v1.0 (multiple per-game profiles, active-profile switching, per-profile enabled set + load order; PROF-01..03)
- ✓ NexusMods integration — v1.0 (OAuth2+PKCE login with API-key fallback, keyring token storage, Premium in-app download, free-user nxm:// handoff, rate-limit compliance, auto-extract to staging; NEXUS-01..06, NXM-01)
- ✓ Guided FOMOD installers — v1.0 (scripted installer wizard with conditional/option-driven file install; FOMOD-01..02)
- ✓ Collections — v1.0 (browse/select, manifest-driven download, apply FOMOD choices + load order + rules, deploy, fully reversible uninstall; COLL-01..05) — see Known Limitations re: live Nexus ingest
- ✓ Distribution — v1.0 (license-clean Linux AppImage, cargo-deny + bundled-binary audit; DIST-01..02; nxm:// handler auto-registered, verified on hardware 2026-06-22)

### Active (v2 / next candidates)

- [ ] Nexus-policy-compliant Collection ingest — manifest import / supported acquisition path so a Collection can be fetched without the Vortex client (engine already operates on a fetched manifest; see Known Limitations)
- [ ] Profile-management UI + switch/uninstall confirmation modals — surface the engine-enforced protections (multi-profile create/switch/delete, destructive-action confirms) as first-class UI
- [ ] In-game re-test of visible mod content now that the install-archive double-nesting bug is fixed (2fa9821) — confirm deployed mods are actually loaded/visible in-game
- [ ] Support for non-Bethesda games in the NexusMods catalog (GAME-01) and game-agnostic generic deployment profiles (GAME-02)
- [ ] Mod update notifications / version tracking (NEXV2-01); advanced download manager — queue, pause/resume, bandwidth limits (NEXV2-02)
- [ ] Experimental overlayfs/VFS deployment method for Proton (DEPV2-01); reflink (CoW) validated per-game as a first-class default (DEPV2-02)
- [ ] Collection authoring/publishing (COLLV2-01)
- [ ] OAuth2 live login activation once a public NexusMods client_id + nxm://oauth/callback redirect is registered (transport is code-complete; API-key path is the works-today login)
- [ ] Automated failure-injection tests for the two reasoned-through failure paths (profile-switch post-purge step failure WR-02; plugins.txt write-failure must-not-persist WR-05)

### Out of Scope

- Native Windows/macOS builds — the project's reason to exist is Linux/Proton; cross-platform is a deliberate non-goal for v1
- Hosting/authoring mods or Collections — NexTwist consumes the NexusMods catalog, it is not a mod-hosting platform
- Non-NexusMods mod sources (ModDB, GameBanana, etc.) — single-source focus for v1 to keep the safety model tight
- A built-in game launcher replacement — NexTwist manages mods; Steam still launches the game
- MO2-style USVFS virtual filesystem on Linux — USVFS is Windows-only API hooking; the Vortex-model real (link-based) deployment is the viable path on Linux

## Current State

**v1.0 "MVP" shipped 2026-06-23** — 5 phases, 21 plans, 26 tasks, ~196 commits (2026-06-20 → 2026-06-23). All 40 v1 requirements satisfied; milestone audit `tech_debt` (0 critical blockers, 6/6 integration seams WIRED, 7/7 E2E flows intact). Core value held: non-destructive, byte-for-byte reversible, conflict-aware deployment.

- **Size:** ~20k Rust LOC (7 headless `crates/*` engine crates + a thin Tauri shell) + ~2.9k frontend LOC.
- **Tech stack:** Rust (2024 edition, MSRV 1.89 pinned by libloot) + Tauri v2 (2.11) shell + SvelteKit/Svelte 5 static SPA; rusqlite (bundled, pinned 0.39 for refinery) + refinery additive migrations (V1→V5); libloot 0.29.5 for plugin load order; reqwest with rustls-only TLS; oauth2 + keyring for NexusMods auth.
- **Security:** four of five phases carry a SECURITY.md with `threats_open: 0` (Phases 1/2/4/5 — 24 + 18 + 17 + 8 threats closed); Phase 3's auth/keyring/`nxm://` boundaries were threat-modeled and verified inline in its VERIFICATION (live OAuth/keyring confirmed on hardware 2026-06-21).
- **Distribution:** license-clean Linux AppImage via a tag-triggered `release.yml`, validated on hardware (commits 98b5321, bbaaa49).

## Known Limitations (v1.0)

- **Live Collection ingest from nexusmods.com is not possible.** NexusMods restricts Collection-archive download to its own Vortex client; a third-party client cannot fetch the collection file (the GraphQL `collectionRevision.downloadLink` seam was intentionally left unimplemented once the policy was understood). The headless Collection engine — apply pinned FOMOD choices + load order, deploy, byte-for-byte reversible uninstall — is fully verified against an **already-fetched manifest**. v2 candidate: a Nexus-policy-compliant ingest / manifest-import path.
- **Profile-management UI is not yet built.** Multi-profile create/switch/delete and destructive-action confirmation are **engine-enforced and tested**, but not surfaced as a first-class UI (switch is confirmation-gated at the engine; a profile panel + modals remain a v2 candidate).
- **In-game visible-content re-test pending.** The install-archive double-nesting bug (wrapper-folder siblings leaking into the staged root) is fixed (commit 2fa9821); a confirmation pass that deployed mod content is actually loaded/visible in-game is a v2 follow-up.
- **OAuth2 live login is code-complete but inactive** until a public NexusMods client_id + `nxm://oauth/callback` redirect is registered; the API-key-paste path is the works-today login.
- **Two failure paths are reasoned-through but not failure-injection-tested** (profile-switch post-purge step WR-02; plugins.txt write-failure must-not-persist WR-05).

## Context

- **Inspiration / prior art**: NexusMods.App (https://github.com/Nexus-Mods/NexusMods.App) and its documentation, and the Vortex mod manager. NexTwist aims for feature parity with Vortex's core loop (login → download → deploy → manage order) on Linux.
- **Platform reality**: Target games are Windows binaries executed on Linux through Steam Proton (Wine). Deployment must account for Proton prefixes, case-sensitivity differences, and Steam library layout on Linux.
- **Deepest modding ecosystem**: Bethesda Creation Engine games have the most mature modding tooling and conventions, making them the right beachhead.
- **Safety model is the differentiator**: reversible + non-destructive + conflict-aware deployment is what earns user trust on a platform where modding tooling is historically fragile.

## Constraints

- **Tech stack**: Rust backend + Tauri (webview frontend) — chosen by the project owner for a small, fast, native-feeling desktop app
- **Platform**: Linux desktop only for v1; manages Windows games run via Steam Proton / Wine
- **Mod source**: NexusMods only for v1 (API/auth, downloads, Collections)
- **Distribution**: AppImage (single portable binary) as the primary v1 channel
- **Deployment strategy**: To be determined during research — evaluate symlink vs hardlink vs overlay/VFS approaches for correctness and reversibility under Proton

## Key Decisions

Outcome legend: ✓ Good (validated, would do again) · ⚠️ Revisit (worked but carries a caveat) · — Pending.

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust + Tauri v2 stack | Owner preference; small native desktop app, precise filesystem control for safe deployment | ✓ Good — ~20k Rust + thin shell, AppImage validated on HW |
| Linux-only, Proton-focused for v1 | The product's whole reason to exist; avoids cross-platform scope sprawl | ✓ Good |
| Bethesda Creation Engine games first (Skyrim SE, Fallout 4) | Deepest, most mature modding ecosystem — best beachhead | ✓ Good |
| **Headless `crates/*` engine with ZERO Tauri deps; thin 3–5-line command adapters** | Keeps the entire safety-critical engine unit/property-testable in CI without a webview; downstream phases compose, not fork | ✓ Good — the highest-leverage decision; 6/6 seams wired, engine CI-tested |
| **Intent-before-act operation journal** (`pending` before syscall → `done` after) + idempotent file ops | A syscall and its DB row can't be atomic; journaled + idempotent replay is the real crash-safety story (not WAL alone) | ✓ Good — crash-recovery test + `recover_on_launch` before UI served |
| **`reflink → hardlink → symlink → copy` method ladder**, chosen per-target via empirical FS probe | Best primitive per filesystem; reflink (independent inode) preferred so deployed edits can't corrupt staging; EXDEV/`CrossesDevices` fallback | ✓ Good — EXDEV exercised on real btrfs/tmpfs boundary |
| Per-file deploy only — never a directory symlink | Guards against Steam updates writing through a directory symlink into staging | ✓ Good |
| Manifest/journal-derived purge & verify (never a blind disk scan) | Reversibility must operate from recorded state, bounded to the deploy root; orphans reported, not deleted | ✓ Good |
| Content-addressed (blake3) vanilla backup; backup-before-overwrite | Restores any overwritten original byte-for-byte; testkit `DIR_SENTINEL` snapshot regression-locks pristine (incl. empty dirs) | ✓ Good (after GAP-01 made the snapshot directory-aware) |
| Single SQLite store, no `rusqlite` type in its public API; additive-only migrations | All SQL stays in `store`; callers speak `core` types; schema evolved V1→V5 without touching prior tables | ✓ Good |
| **`rusqlite` pinned to 0.39 (not 0.40)** | `refinery 0.9.2` caps its rusqlite feature there; avoids a double `links = "sqlite3"` resolution | ⚠️ Revisit — version-coupling to unpin when refinery catches up |
| **`nextwist_core` alias (never bare `core`)** | A bare `core` dep shadows std `::core` that `thiserror`/Tauri macros expand to | ✓ Good (hit repeatedly; aliasing is the fix) |
| **rustls-only TLS** (reqwest `rustls` feature) | No system OpenSSL → self-contained, portable AppImage across distros | ✓ Good — dist audit confirms no app-path OpenSSL |
| **`cargo-deny` bans non-free UnRAR**; RAR via system `unrar`/`7z` shell-out | UnRAR source is non-free/GPL-incompatible — a licensing liability to bundle; keep it out of the distributed binary | ✓ Good — clean license audit at Phase 5 |
| libloot 0.29.5 for plugin load order (`Game::with_local_path`) | Don't hand-roll Bethesda load-order/LOOT logic; the Proton-prefix AppData seam is the supported Linux path | ✓ Good — de-risked in its own plan (02-02) before building on it |
| OAuth2 + PKCE login + **API-key paste fallback** + keyring storage | Forward-looking auth, but works-today without a registered client_id; keyring hard-fails rather than write plaintext | ⚠️ Revisit — OAuth live activation pending Nexus client_id registration |
| Headless `fomod` engine (quick-xml) with a **pure dry-run resolver** | Conflict preview / FOMOD plan computed with zero disk writes — the locked safety gate before any staging write | ✓ Good |
| Collections add **zero new engine primitives** (compose download + FOMOD-replay + profile switch + purge) | Reuse the proven safe paths; one core per capability so entry points can't diverge | ✓ Good — pristine round-trip regression-locked, no network |
| AppImage distribution via tag-triggered `release.yml`; code-signing deferred | Portable, no install friction; signing is a v2 cost | ✓ Good (signing ⚠️ deferred to v2) |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-06-23 after v1.0 milestone (MVP shipped — 5 phases, 40/40 requirements; see RETROSPECTIVE.md)*
