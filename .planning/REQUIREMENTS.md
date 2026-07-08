# Requirements: NexTwist — Milestone v1.1 Starfield Support

**Defined:** 2026-07-07
**Core Value:** Mods install and uninstall safely — non-destructive, fully reversible, conflict-aware deployment into Proton/Wine games on Linux. This guarantee must extend unchanged to Starfield and to the new INI-management write target.

**Milestone goal:** Bring full, safe mod management to Starfield (Creation Engine 2, Steam AppID 1716740) by reusing the v1.0 engine and adding the CE2-specific handling required for mods to actually load in-game.

**Research basis:** libloot 0.29.5 + esplugin 6.1.4 already expose `GameType::Starfield` / `GameId::Starfield` (zero MSRV/dependency change); store is appid-generic (no DB migration); the only new dependency is `rust-ini`; `.ba2` v3 stays opaque to deploy. See `.planning/research/SUMMARY.md`.

---

## v1.1 Requirements

Each requirement maps to exactly one roadmap phase (see Traceability). Phase numbering continues from v1.0 (starts at Phase 6).

### Starfield Detection & Environment

- [x] **SFDET-01**: User can add Starfield (Steam AppID 1716740) as a managed Bethesda game, auto-detected under Steam/Proton with its install dir and Proton prefix resolved
- [x] **SFDET-02**: NexTwist resolves Starfield's CE2 config location (`Documents/My Games/Starfield` inside the Proton prefix), handling Wine case-folding and the not-yet-created (pre-first-launch) folder case
- [x] **SFDET-03**: NexTwist detects the installed Starfield game version and warns the user when a game update may have changed loose-file/load-order behavior (version drift)

### Starfield Load Order

- [x] **SFLO-01**: User can enable/disable and order Starfield plugins, written as asterisk-format `plugins.txt` at the CE2 prefix location
- [x] **SFLO-02**: User can auto-sort the Starfield load order via LOOT using libloot's Starfield masterlist
- [x] **SFLO-03**: NexTwist never reorders, disables, or writes Starfield's protected base masters, and correctly classifies the CE2 "medium master" tier — both determined via libloot, not hard-coded
- [x] **SFLO-04**: Verify/repair treats the game's on-launch rewrite of `plugins.txt` (e.g. `.ccc` entries, stripped implicit ESMs) as expected, deriving intent from recorded plugin state rather than a raw on-disk diff

### Reversible Loose-File Activation (StarfieldCustom.ini)

- [x] **SFINI-01**: User can enable loose-file loading for Starfield; NexTwist writes the required `StarfieldCustom.ini` edits (`[Archive] bInvalidateOlderFiles=1`, `sResourceDataDirsFinal=`)
- [x] **SFINI-02**: NexTwist records the INI's provenance (pre-existing vs created-by-NexTwist) and, on purge, restores it exactly — restoring original bytes if it pre-existed, or deleting the file and any emptied parent directories if NexTwist created it (restore-ABSENCE)
- [x] **SFINI-03**: NexTwist merges its edits surgically into an existing `StarfieldCustom.ini` without clobbering the user's other keys/sections, preserving byte fidelity (line endings, BOM)
- [x] **SFINI-04**: INI activation is journaled and idempotent — an interrupted or repeated deploy/purge leaves the INI in a correct, recoverable state
- [x] **SFINI-05**: The INI edit participates in the same reversibility guarantee as deployment — covered by crash recovery (`recover_on_launch`) and verify/repair

### In-Game Verification

- [ ] **SFVER-01**: User can complete an on-hardware verification that a real Starfield mod deploys AND is visible/loaded in-game on the owner's live Proton install
- [ ] **SFVER-02**: NexTwist surfaces a "deployed vs actually loaded in-game" distinction so the user understands deployment success does not by itself guarantee in-game loading

---

## Future Requirements (deferred to v1.x / v2)

- [ ] **SFBA2-01** *(future)*: `.ba2` v3 archive inspection / conflict-awareness of archived assets (v1.1 treats `.ba2` as opaque files; loose files + plugins cover ~95% of third-party mods)
- [ ] **SFSE-01** *(future)*: Manage SFSE (Starfield Script Extender) as a first-class dependency/launch path
- [ ] **SFCOLL-01** *(future)*: Starfield-specific Collection curation nuances beyond the game-agnostic Collection lifecycle already shipped in v1.0

## Out of Scope (explicit exclusions)

- **Game Pass / Xbox-app Starfield** — NexTwist is Steam/Proton only; the Xbox install layout and its separate mod path are a deliberate non-goal
- **Creation Kit integration** — authoring tooling is out of scope; NexTwist consumes mods, it does not create them
- **Authoring / editing `Starfield.ccc`** — the game owns its content catalog; NexTwist delegates load-order concerns to libloot and never writes `.ccc`
- **`StarfieldPrefs.ini` edits** — only `StarfieldCustom.ini` is touched; graphics/preferences INIs are the user's domain
- **`.ba2` v3 repacking / extraction into loose files** — no archive repacking in v1.1 (see Future)

---

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| SFDET-01 | Phase 6 — Starfield Detection & CE2 Path Resolution | Complete |
| SFDET-02 | Phase 6 — Starfield Detection & CE2 Path Resolution | Complete |
| SFDET-03 | Phase 6 — Starfield Detection & CE2 Path Resolution | Complete |
| SFLO-01 | Phase 7 — Starfield Load Order | Complete |
| SFLO-02 | Phase 7 — Starfield Load Order | Complete |
| SFLO-03 | Phase 7 — Starfield Load Order | Complete |
| SFLO-04 | Phase 7 — Starfield Load Order | Complete |
| SFINI-01 | Phase 8 — Reversible StarfieldCustom.ini Activation | Complete |
| SFINI-02 | Phase 8 — Reversible StarfieldCustom.ini Activation | Complete |
| SFINI-03 | Phase 8 — Reversible StarfieldCustom.ini Activation | Complete |
| SFINI-04 | Phase 8 — Reversible StarfieldCustom.ini Activation | Complete |
| SFINI-05 | Phase 8 — Reversible StarfieldCustom.ini Activation | Complete |
| SFVER-01 | Phase 9 — On-Hardware In-Game Verification | Pending |
| SFVER-02 | Phase 9 — On-Hardware In-Game Verification | Pending |

**Coverage:** 14/14 v1.1 requirements mapped to exactly one phase. No orphans, no duplicates.
