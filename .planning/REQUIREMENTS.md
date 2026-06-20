# Requirements: NexTwist

**Defined:** 2026-06-20
**Core Value:** Mods install and uninstall safely — non-destructive, fully reversible, conflict-aware deployment into Proton/Wine games on Linux.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Game & Environment Detection

- [ ] **ENV-01**: User can have NexTwist auto-detect installed Steam games on Linux
- [ ] **ENV-02**: User can have NexTwist resolve a game's install directory and Proton/Wine prefix
- [ ] **ENV-03**: User can add and manage supported Bethesda Creation Engine games (Skyrim SE, Fallout 4) as managed games
- [ ] **ENV-04**: NexTwist detects the deployment filesystem's capabilities (same-device, case-folding) and warns about unsafe configurations

### Mod Staging & Extraction

- [ ] **STAGE-01**: User can install a mod from a local archive (.zip, .7z) into a managed staging store
- [ ] **STAGE-02**: NexTwist safely extracts archives, rejecting path-traversal (zip-slip) entries
- [ ] **STAGE-03**: User can install .rar mods via the system unrar/7z tool (no non-free code bundled)

### Deployment Engine (core safety)

- [ ] **DEPLOY-01**: User can deploy enabled mods into the game without modifying original game files
- [ ] **DEPLOY-02**: NexTwist records every deployed file in a per-game manifest/ledger
- [ ] **DEPLOY-03**: User can purge/uninstall mods, restoring the game folder to its pristine vanilla state
- [ ] **DEPLOY-04**: NexTwist backs up any overwritten original game file before deployment so it can be restored
- [ ] **DEPLOY-05**: NexTwist selects a safe deployment method per target (reflink → hardlink → symlink → copy) accounting for filesystem boundaries
- [ ] **DEPLOY-06**: Deployment and purge are crash-safe (journaled) so an interrupted operation can be recovered
- [ ] **DEPLOY-07**: User can run a verify/repair that detects manifest-vs-disk drift (files changed outside NexTwist)
- [ ] **DEPLOY-08**: NexTwist resolves case-sensitivity mismatches so mods load correctly under Proton

### Conflicts & Load Order

- [ ] **CONF-01**: User can see which mods overwrite which files (file-level conflicts)
- [ ] **CONF-02**: User can set mod priority/order to control which mod wins a conflict
- [ ] **CONF-03**: Deployment applies the user's conflict-winner choices deterministically

### Plugin Management

- [ ] **PLUGIN-01**: User can enable and disable individual game plugins (.esp/.esm/.esl)
- [ ] **PLUGIN-02**: User can view and adjust plugin load order, written to plugins.txt in the correct prefix location
- [ ] **PLUGIN-03**: User can auto-sort plugins via LOOT

### Profiles

- [ ] **PROF-01**: User can create multiple independent mod profiles per game
- [ ] **PROF-02**: User can switch the active profile, changing which mods/plugins/order are deployed
- [ ] **PROF-03**: Each profile preserves its own enabled-mod set and load order

### NexusMods Integration

- [ ] **NEXUS-01**: User can log into their NexusMods account via OAuth2
- [ ] **NEXUS-02**: NexTwist stores auth tokens securely in the system keyring
- [ ] **NEXUS-03**: Premium users can download a mod directly from NexusMods within the app
- [ ] **NEXUS-04**: Free users can install mods via the website "Mod Manager Download" (nxm://) handoff
- [ ] **NEXUS-05**: NexTwist respects NexusMods API rate limits
- [ ] **NEXUS-06**: A downloaded mod is auto-extracted into staging ready to deploy
- [ ] **NXM-01**: User can one-click install from an nxm:// link via a deep-link handler registered on Linux

### Installers (FOMOD)

- [ ] **FOMOD-01**: User can install mods with FOMOD scripted installers, making option choices through a guided UI
- [ ] **FOMOD-02**: FOMOD conditional/option-driven file installation is applied correctly to staging

### Collections

- [ ] **COLL-01**: User can browse and select a NexusMods Collection for a managed game
- [ ] **COLL-02**: NexTwist downloads all mods in a Collection revision per its manifest
- [ ] **COLL-03**: NexTwist applies the Collection's FOMOD choices, load order, and rules automatically
- [ ] **COLL-04**: User can deploy an installed Collection so the modded game launches
- [ ] **COLL-05**: User can cleanly uninstall a Collection (fully reversible)

### Distribution

- [ ] **DIST-01**: NexTwist is packaged and runnable as a Linux AppImage
- [ ] **DIST-02**: The distributed build passes a license-compliance audit (no non-free bundled code, e.g. UnRAR)

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Games

- **GAME-01**: Support for non-Bethesda games in the NexusMods catalog
- **GAME-02**: Game-agnostic generic deployment profiles

### Deployment

- **DEPV2-01**: Experimental overlayfs/VFS deployment method for Proton
- **DEPV2-02**: Reflink (CoW) deployment validated per-game as a first-class default

### NexusMods

- **NEXV2-01**: Mod update notifications and version tracking
- **NEXV2-02**: Advanced download manager (queue, pause/resume, bandwidth limits)

### Collections

- **COLLV2-01**: Collection authoring/publishing

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Native Windows/macOS builds | The product exists to serve Linux/Proton; cross-platform is a deliberate non-goal |
| Mod or Collection hosting/authoring | NexTwist consumes the NexusMods catalog; it is not a hosting platform |
| Non-NexusMods mod sources (ModDB, GameBanana) | Single-source focus keeps the safety model tight for v1 |
| Built-in game launcher replacement | NexTwist manages mods; Steam still launches the game |
| MO2-style USVFS virtual filesystem on Linux | USVFS is Windows-only API hooking; Vortex-model real deployment is the viable path on Linux |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| (populated during roadmap creation) | — | Pending |

**Coverage:**
- v1 requirements: 36 total
- Mapped to phases: 0 (pending roadmap)
- Unmapped: 36 ⚠️

---
*Requirements defined: 2026-06-20*
*Last updated: 2026-06-20 after initial definition*
