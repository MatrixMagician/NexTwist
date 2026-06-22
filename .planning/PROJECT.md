# NexTwist

## What This Is

NexTwist is a Rust + Tauri desktop application that brings Vortex/Mod-Organizer-2-class mod management to Linux gamers. It lets users log into NexusMods, download and install individual mods and curated Collections, and manage them for Windows PC games that run on Linux via Steam Proton / Wine — with safe, fully reversible deployment. It is for Linux gamers (and modding power users migrating from Windows) who want a first-class mod manager that "just works" on their platform.

## Core Value

Mods must install and uninstall **safely**: deployment is non-destructive (the base game install is never directly corrupted), fully reversible (any mod or the whole load order can be removed leaving the game pristine), and conflict-aware (the user always knows and controls which mods overwrite which files). If everything else fails, this must hold.

## Requirements

### Validated

- ✓ App is distributable as a Linux AppImage — Phase 5 (license-clean, nxm:// handler auto-registered; verified on hardware 2026-06-22)

### Active

- [ ] User can authenticate with their NexusMods account
- [ ] User can download individual mods from NexusMods through the app
- [ ] User can install a NexusMods Collection (curated mod list) end-to-end
- [ ] User can deploy mods into a Proton/Wine game non-destructively
- [ ] User can fully uninstall mods, returning the game folder to a pristine state
- [ ] User can see and resolve file conflicts between mods (overwrite priority)
- [ ] User can create and switch between multiple mod profiles per game
- [ ] User can control mod load order / priority
- [ ] App detects Steam (Proton) game installations on Linux
- [ ] Bethesda Creation Engine games (e.g. Skyrim SE, Fallout 4) are supported first

### Out of Scope

- Native Windows/macOS builds — the project's reason to exist is Linux/Proton; cross-platform is a deliberate non-goal for v1
- Hosting/authoring mods or Collections — NexTwist consumes the NexusMods catalog, it is not a mod-hosting platform
- Non-NexusMods mod sources (ModDB, GameBanana, etc.) — single-source focus for v1 to keep the safety model tight
- A built-in game launcher replacement — NexTwist manages mods; Steam still launches the game

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

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust + Tauri stack | Owner preference; small native desktop app, strong filesystem control for safe deployment | — Pending |
| Linux-only, Proton-focused for v1 | The product's whole reason to exist; avoids cross-platform scope sprawl | — Pending |
| Bethesda Creation Engine games first | Deepest, most mature modding ecosystem — best beachhead | — Pending |
| Full Nexus integration incl. Collections in v1 | Collections are the modern Vortex-defining feature; users expect one-click curated lists | — Pending |
| Profiles + load-order core to v1 | Multi-profile and priority control are must-haves for serious modding (MO2 parity) | — Pending |
| Deployment method deferred to research | Linux/Proton filesystem nuances make this a decision to ground in research, not assume | — Pending |
| AppImage distribution | Portable, no install friction, broad distro coverage | — Pending |

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
*Last updated: 2026-06-22 after Phase 5 (AppImage Distribution) — milestone v1.0 phases all complete*
