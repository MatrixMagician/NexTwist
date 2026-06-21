# Roadmap: NexTwist

## Overview

NexTwist is built safety-first: the differentiating, irreplaceable value — non-destructive, fully reversible, conflict-aware deployment into Proton/Wine games — is delivered end-to-end before any NexusMods networking exists. Each phase is a vertical MVP slice that leaves the user with a usable capability. Phase 1 proves the entire safety round-trip on a *local* archive (detect a Bethesda game under Proton → install a local mod → deploy non-destructively → uninstall leaving the game byte-for-byte pristine). Phase 2 makes the manifest meaningful with many mods (conflicts, load order, plugins) and adds per-game profiles. Only then does Phase 3 add NexusMods login + download, Phase 4 adds guided FOMOD installers and one-click Collections, and Phase 5 packages it all as a distributable, license-clean AppImage. The API surface is replaceable; deployment correctness is the reason the product exists, so it ships first and is the most heavily tested.

## Phases

**Phase Numbering:**

- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Safe Local Round-Trip** - Detect a Bethesda Proton game, install a LOCAL archive, deploy non-destructively, purge to pristine (completed 2026-06-20)
- [x] **Phase 2: Multi-Mod Management** - Many mods with conflict resolution, load order, plugin ordering, and per-game profiles (completed 2026-06-21)
- [ ] **Phase 3: NexusMods Login & Download** - OAuth login, secure tokens, in-app + nxm:// one-click downloads into staging
- [ ] **Phase 4: Guided Installers & Collections** - FOMOD wizard installs and end-to-end NexusMods Collection install/deploy/uninstall
- [ ] **Phase 5: AppImage Distribution** - License-clean, single-file Linux AppImage with registered nxm:// handler

## Phase Details

### Phase 1: Safe Local Round-Trip

**Goal**: A user can take a Bethesda game running under Steam Proton and a local mod archive, install and deploy that mod without touching original game files, then fully uninstall it and have the game folder return byte-for-byte to its vanilla state.
**Mode:** mvp
**Depends on**: Nothing (first phase)
**Requirements**: ENV-01, ENV-02, ENV-03, ENV-04, STAGE-01, STAGE-02, STAGE-03, DEPLOY-01, DEPLOY-02, DEPLOY-03, DEPLOY-04, DEPLOY-05, DEPLOY-06, DEPLOY-07, DEPLOY-08
**Success Criteria** (what must be TRUE):

  1. User can have NexTwist auto-detect installed Steam games, add a supported Bethesda game (Skyrim SE / Fallout 4) as managed, and see its resolved install directory and Proton prefix paths
  2. User can install a local mod archive (.zip / .7z / .rar via system tool) into staging, with malicious (zip-slip / symlink / `..`) entries safely rejected
  3. User can deploy the enabled mod into the game with zero original game files modified in place, every deployed file recorded in a per-game manifest, and any overwritten vanilla file backed up first
  4. User can purge/uninstall and verify (hash-diff) the game folder is byte-for-byte pristine — no orphans, originals restored — even after an interrupted (crash-mid-deploy) operation
  5. User is warned before deploying when the filesystem configuration is unsafe (cross-device/EXDEV, case-folding), and NexTwist selects a safe method (reflink → hardlink → symlink → copy) and resolves case mismatches so the mod loads under Proton

**Plans**: 7/7 plans complete

Plans:

- [x] 01-01-PLAN.md — Workspace scaffold + toolchain + core/store persistence (manifest, op-journal, vanilla store) + testkit + cargo-deny ban
- [x] 01-02-PLAN.md — steam crate: Steam/Proton detection + prefix resolution + manual-add + canonical Data/ casing map
- [x] 01-03-PLAN.md — extract crate: safe zip/7z/system-rar extraction with zip-slip/symlink defense -> read-only staging
- [x] 01-04-PLAN.md — deploy engine (crown jewel): probe + method ladder + journal + vanilla backup + deploy/purge/recover; round_trip + crash_recovery tests
- [x] 01-05-PLAN.md — deploy integrity: case-sensitivity normalization (DEPLOY-08) + verify/repair drift (DEPLOY-07) + fs-warnings
- [x] 01-06-PLAN.md — Tauri shell + thin command adapters + functional-minimal Svelte 5 UI + startup recovery + CI; human-verify checkpoint
- [x] 01-07-PLAN.md — gap closure (GAP-01, DEPLOY-03/07): purge/recovery remove deploy-created empty dirs; directory-aware testkit pristine assertion; verify/repair orphan-empty-dir detection

### Phase 2: Multi-Mod Management

**Goal**: A user managing many mods can see and resolve file conflicts, control which mod wins via priority/load order, order game plugins correctly in the right Proton prefix, and maintain multiple independent profiles per game that switch the deployed mod set on demand.
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: CONF-01, CONF-02, CONF-03, PLUGIN-01, PLUGIN-02, PLUGIN-03, PROF-01, PROF-02, PROF-03
**Success Criteria** (what must be TRUE):

  1. User can see which mods overwrite which files (file-level conflicts) and set mod priority/order so a chosen mod deterministically wins, with deployment applying those winner choices
  2. User can enable/disable individual plugins (.esp/.esm/.esl) and adjust plugin load order, written to plugins.txt in the correct Proton-prefix location so it applies in-game
  3. User can auto-sort plugins via LOOT
  4. User can create multiple independent profiles per game, switch the active profile to change which mods/plugins/order are deployed, and each profile preserves its own enabled-mod set and load order

**Plans**: 5/5 plans complete
**UI hint**: yes

Plans:
**Wave 1**

- [x] 02-01-PLAN.md — Foundation: MSRV 1.89 + cargo-deny GPL allowance + core model (rank/Profile/Plugin/PluginKind/FileConflict) + V2 refinery migration (managed_mod/profile/profile_mod/plugin_state + Default-profile data migration, BLOCKING test) + store query modules
- [x] 02-02-PLAN.md — libloot spike (de-risk A1/A3): crates/loadorder scaffold + libloot dep behind legitimacy checkpoint + with_local_path round-trip against fixture Proton prefix + testkit fake_proton_prefix

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 02-03-PLAN.md — Conflict slice (CONF-01/02/03): pure-fold resolver → single-winner StagedFiles + multi-root contract + winner deploy + conflict Tauri commands + Conflict view + round-trip-pristine redeploy test
- [x] 02-04-PLAN.md — Plugin + LOOT slice (PLUGIN-01/02/03): plugin scan + libloot enable/order/plugins.txt write (asterisk, masters-first) + masterlist fetch/cache + LOOT propose-then-apply + plugin Tauri commands + Plugin manager view

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 02-05-PLAN.md — Profile slice (PROF-01/02/03): switch_profile reconcile (purge→deploy→plugins.txt) + profile Tauri commands + confirmation-gated Profile selector + cross-switch round-trip-pristine test

### Phase 3: NexusMods Login & Download

**Goal**: A user can log into their NexusMods account and pull mods straight into NexTwist's staging store — Premium users via in-app direct download, free users via the website "Mod Manager Download" nxm:// handoff — ready to deploy through the safe engine already proven in Phases 1-2.
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: NEXUS-01, NEXUS-02, NEXUS-03, NEXUS-04, NEXUS-05, NEXUS-06, NXM-01
**Success Criteria** (what must be TRUE):

  1. User can log into their NexusMods account via OAuth2, with auth tokens stored securely in the system keyring (never plaintext)
  2. Premium users can download a mod directly within the app; free users can install via the website "Mod Manager Download" (nxm://) handoff
  3. User can one-click install from an nxm:// link on nexusmods.com, routed to the running app via a registered Linux deep-link handler
  4. A downloaded mod is auto-extracted into staging, ready to deploy, while NexTwist respects NexusMods API rate limits (no UI freeze on large downloads)

**Plans**: 1/3 plans executed
**UI hint**: yes

Plans:
**Wave 1**

- [x] 03-01-PLAN.md — Auth spine slice (NEXUS-01/02): crates/nexus scaffold + error/model + OAuth2-PKCE & API-key auth (mockito) + shell keyring hard-fail-no-plaintext + login/logout commands + AppState + account panel UI

**Wave 2** *(blocked on Wave 1)*

- [ ] 03-02-PLAN.md — Premium download-into-staging slice (NEXUS-03/05/06): hybrid REST-v1-download-link / GraphQL-v2-metadata client + governor rate limiter + streaming download w/ progress + V4 provenance migration + store facade + core NexusSource + downloads command (extract→stage→provenance) + downloads-list UI

**Wave 3** *(blocked on Wave 2)*

- [ ] 03-03-PLAN.md — Free-user nxm:// handoff slice (NEXUS-04, NXM-01): headless strict nxm:// parser + single-instance-first + deep-link wiring + on_open_url routing (oauth-callback vs download) + free-user key+expires redemption + nxm:// toast / free-user hint / expired-link Warning UI

### Phase 4: Guided Installers & Collections

**Goal**: A user can install complex mods through a guided FOMOD option wizard and install an entire curated NexusMods Collection end-to-end — download all pinned mods, replay the Collection's FOMOD choices and load order, deploy so the modded game launches, and cleanly and reversibly uninstall the whole Collection.
**Mode:** mvp
**Depends on**: Phase 2, Phase 3
**Requirements**: FOMOD-01, FOMOD-02, COLL-01, COLL-02, COLL-03, COLL-04, COLL-05
**Success Criteria** (what must be TRUE):

  1. User can install a mod with a FOMOD scripted installer, making option choices through a guided UI, and those conditional/option-driven files are installed correctly to staging
  2. User can browse and select a NexusMods Collection for a managed game, and NexTwist downloads all mods in the chosen revision per its manifest (resolving/reporting archived or unavailable mods before touching disk)
  3. NexTwist automatically applies the Collection's FOMOD choices, load order, and rules
  4. User can deploy an installed Collection so the modded game launches, and can later cleanly uninstall the Collection with full reversibility

**Plans**: TBD
**UI hint**: yes

Plans:

- [ ] 04-01: TBD

### Phase 5: AppImage Distribution

**Goal**: A user (or distro) can download a single-file Linux AppImage, run NexTwist with no install friction, and have the nxm:// MIME handler registered automatically — with the distributed build passing a license-compliance audit so it contains no non-free bundled code.
**Mode:** mvp
**Depends on**: Phase 3, Phase 4
**Requirements**: DIST-01, DIST-02
**Success Criteria** (what must be TRUE):

  1. User can run NexTwist as a packaged single-file Linux AppImage, with the nxm:// scheme handler registered to the AppImage on first run (stable Exec path, self-test passes)
  2. The distributed build passes a license-compliance audit (cargo-deny / bundled-binary review) confirming no non-free code (e.g. UnRAR) is shipped

**Plans**: TBD

Plans:

- [ ] 05-01: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Safe Local Round-Trip | 7/7 | Complete   | 2026-06-20 |
| 2. Multi-Mod Management | 5/5 | Complete   | 2026-06-21 |
| 3. NexusMods Login & Download | 1/3 | In Progress|  |
| 4. Guided Installers & Collections | 0/TBD | Not started | - |
| 5. AppImage Distribution | 0/TBD | Not started | - |
