# Feature Research — Starfield (Creation Engine 2) Support

**Domain:** Adding Starfield mod support to NexTwist (existing Linux/Proton NexusMods mod manager)
**Researched:** 2026-07-07
**Confidence:** MEDIUM-HIGH — load-order facts cross-checked against Ortham's libloot/libloadorder author blog (authoritative, the exact library NexTwist already uses); INI/loose-file facts from Nexus (mod 273), modding.wiki, BethINI consensus; Proton paths from ProtonDB / Steam Deck guides.

## Scope note (read first)

This milestone is a **game-support extension, not a re-architecture.** Skyrim SE / Fallout 4
already ship in v1.0 with reversible deployment, file conflict resolution, plugins.txt via
libloot, per-game profiles, NexusMods download, FOMOD, and Collections. **Do not re-research
or re-list those** — this document covers **only what Creation Engine 2 (Starfield) does
differently** and the new behaviors NexTwist must add.

The one-line thesis: **For a Starfield mod to install AND be visible in-game, NexTwist must (a)
detect Starfield under Proton at the CE2 `Documents/My Games/Starfield` path, (b) reversibly
manage the `StarfieldCustom.ini` archive-invalidation edit so loose files load, and (c) write
an asterisk-format `plugins.txt` at that same CE2 path via `libloot GameType::Starfield`.**
Everything else (deploy ladder, journal, conflicts, profiles) is reused unchanged.

## The core mechanical differences vs SSE/FO4

| Concern | SSE / FO4 (v1.0, already built) | Starfield (CE2, new) | Impact on NexTwist |
|---------|-------------------------------|----------------------|--------------------|
| AppData path | `…/AppData/Local/Skyrim Special Edition` (and `…/Local/Fallout4`) | `…/Documents/My Games/Starfield` | **NEW path resolver.** Different Proton-prefix subtree — `Documents/My Games/…` not `AppData/Local/…`. The v1.0 `appdata_local_path` seam must gain a Starfield branch (or a `local_path` override). |
| Loose-file activation | Archive invalidation exists but SSE/FO4 mods mostly ship BA2 that auto-loads; loose files "just work" for many | **Loose files DO NOT load** until `StarfieldCustom.ini` has `[Archive] bInvalidateOlderFiles=1` + `sResourceDataDirsFinal=` (empty) | **NEW reversible INI-management path.** This is the headline new behavior; without it a "successfully deployed" mod is invisible in-game. |
| plugins.txt format | Asterisk-active `plugins.txt`, base ESMs implicit — handled by libloot | Same asterisk format, but **8** implicit base ESMs and the game **rewrites plugins.txt on launch** (moves `.ccc` entries to top, strips implicit ESMs) | **libloot already supports it** — pass `GameType::Starfield`. Verify/repair must tolerate the game rewriting the file. |
| Plugin tiers | ESM / ESL (light, `FE` FormID space) / ESP | Adds **medium** plugins (`0x400` flag, `FDxxyyyy` FormID space, 256 active / 65535 records) | **Zero manager work** — index math is inside esplugin/libloadorder ≥ v6/v17 (NexTwist's libloot 0.29.5 exposes `GameType::Starfield`). Surface tier in UI is optional. |
| Archives | BA2 v1 | **BA2 v2/v3** | **Awareness only.** Loose files + plugins.txt cover ~95% of third-party mods; no repack needed. |
| CC load-order file | `Skyrim.ccc` / `Fallout4.ccc` (present, read-only) | `Starfield.ccc` (usually absent; game synthesizes/merges it into plugins.txt) | Read-only awareness; do not author it (see anti-features). |

## Feature Landscape

### Table Stakes (a Starfield mod must actually load in-game)

Missing any of these = "I installed a mod and nothing changed in-game." These are the pass/fail
gate for the milestone.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Starfield detection under Proton (AppID 1716740)** | Can't manage what you can't find | LOW | Steam-only. Reuse v1.0 steamlocate + prefix resolver; add AppID 1716740 + install-dir `Starfield`. |
| **CE2 AppData path resolution** | INI + plugins.txt live here, not the SSE/FO4 location | MEDIUM | `compatdata/1716740/pfx/drive_c/users/steamuser/Documents/My Games/Starfield/`. NOTE: `Documents/My Games/` — a **different subtree** than v1.0's `AppData/Local/<Game>`. Wine case-folding still applies (reuse `casing.rs`). |
| **Reversible `StarfieldCustom.ini` archive-invalidation edit** | Loose files are invisible in-game without it | MEDIUM-HIGH | Ensure `[Archive]` section contains `bInvalidateOlderFiles=1` and `sResourceDataDirsFinal=` (empty). Must be **non-destructive + byte-for-byte reversible** (back up any pre-existing user INI; purge restores it exactly, or removes the file if NexTwist created it). This is the new safety-critical surface. See "exact edits" below. |
| **Asterisk `plugins.txt` at the CE2 path** | Plugins won't be active otherwise | LOW-MEDIUM | Reuse v1.0 plugin manager; point libloot at the Starfield `local_path` and pass `GameType::Starfield`. Do NOT list the 8 implicit base ESMs. |
| **LOOT sort for Starfield** | Load-order correctness / conflict avoidance | LOW | libloot 0.29.5 already exposes `GameType::Starfield`; masterlist exists. Same `sort_plugins → set_load_order` flow as v1.0 (plan 02-04). |
| **Reversible deploy of Starfield mods** | The core product guarantee | LOW | Reuse the deploy ladder + journal unchanged; only the deploy target (game `Data/`) and the INI/plugins side-effects are game-specific. |
| **Handle the game rewriting plugins.txt** | Game edits the file on every launch | LOW-MEDIUM | Verify/repair must treat game-side reordering of implicit ESMs / `.ccc` insertion as **expected**, not corruption. Re-derive active set from NexTwist's DB `plugin_state`, don't blindly diff the on-disk file. |

### Differentiators (competitive advantage, align with the safety Core Value)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **INI edit under the same reversibility guarantee as deployment** | No other Linux manager treats the `StarfieldCustom.ini` edit as journaled + byte-for-byte reversible; NexTwist's whole pitch is "purge restores pristine" — extend it to INI | MEDIUM | Record the pre-edit INI (or its absence) in the vanilla-backup ledger; purge/verify restores it. This is the differentiator that fits the existing engine's identity. |
| **BA2-packed mod awareness** | Detect a mod that ships only `.ba2` and either register or transparently unpack to loose files, so it loads without user INI archaeology | MEDIUM | ~95% case is loose files; BA2 handling covers the tail. Detect `ModName - *.ba2` matching a plugin (auto-loads) vs orphan BA2 (needs registration/unpack). Ship AFTER table stakes. |
| **"Will this load?" pre-flight check** | Warn if archive invalidation is off, or a plugin references a missing master, before the user launches and sees nothing | MEDIUM | Reuse the dry-run/verify machinery; surface Starfield-specific "loose files won't load — enable archive invalidation?" |
| **Medium-master tier surfaced in the load-order UI** | Power users want to see the ESM/medium/ESL/ESP tier and index budget | LOW | esplugin already classifies it; just display. Pure read-out, no engine work. |

### Anti-Features (do NOT build — scope-guard for the milestone)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Xbox / Game Pass / Microsoft Store Starfield support** | Some users own it there | Different install layout, encrypted/UWP packaging, no Proton path, no `compatdata`; NexTwist is **Steam/Proton-only** (stated constraint) | Steam-only. Detect non-Steam install and say "unsupported" cleanly. |
| **Creation Kit / Creations (paid mods) integration** | "Manage my Creations too" | In-game Creations menu + Bethesda.net auth is a separate ecosystem; CK is a Windows authoring tool, out of a mod *manager*'s job | Manage NexusMods loose-file/plugin mods only. Read `.ccc` awareness ≠ authoring it. |
| **Authoring / writing `Starfield.ccc`** | Force a load order the game respects | The game synthesizes and rewrites it; hand-authoring fights the engine and breaks reversibility | Manage order through `plugins.txt` via libloot (the supported seam), leave `.ccc` read-only. |
| **Achievement-enabler / SFSE-dependent "core" mods as a requirement** | Popular community asks | SFSE is a separate DLL-injection dependency with its own versioning; making it a hard dependency couples NexTwist to script-extender release cadence and Proton quirks | Deploy SFSE-dependent mods like any other files; do NOT bundle/manage SFSE itself or gate features on it. |
| **Deep BA2 repacking / archive rebuilding** | "Optimize my archives" | Rebuilding BA2 v3 is a modding-tool job (Archive2/BSArch), heavy, and risks correctness | Loose files cover the case; at most unpack, never repack. |
| **Editing `StarfieldPrefs.ini`** | It's the other INI in the folder | `StarfieldPrefs.ini` holds video/audio/gameplay prefs — irrelevant to mod loading; editing it is scope creep and a support liability | Touch **only** `StarfieldCustom.ini` `[Archive]`. Leave Prefs alone. |
| **Blindly "fixing" plugins.txt after the game rewrites it** | It looks "wrong" after launch | The game legitimately reorders implicit ESMs / injects `.ccc` — treating that as corruption causes a fight-the-game loop | Derive intent from DB; only assert NexTwist's managed entries, tolerate game-managed ones. |

## Exact behaviors to implement (testable — for requirements)

### 1. `StarfieldCustom.ini` — the canonical loose-file activation edit

Location (Proton prefix, Steam AppID 1716740):
```
<STEAM>/steamapps/compatdata/1716740/pfx/drive_c/users/steamuser/Documents/My Games/Starfield/StarfieldCustom.ini
```
Required content (community-canonical; Nexus mod 273 "Base StarfieldCustom.ini", modding.wiki, BethINI produce exactly this):
```ini
[Archive]
bInvalidateOlderFiles=1
sResourceDataDirsFinal=
```
- `sResourceDataDirsFinal=` is set to the **empty string** (not deleted, not a path list).
- If `[Archive]` already exists with other keys, **merge** these two keys, don't clobber the section.
- `StarfieldPrefs.ini` is present in the same folder but is **NOT involved** in loose-file loading — do not edit it.
- Reversibility: if the file/section pre-existed, back it up (content-addressed, like the vanilla file ledger) and restore byte-for-byte on purge; if NexTwist created the file, purge removes it.

### 2. `plugins.txt` — asterisk format, Starfield quirks

Location: `…/Documents/My Games/Starfield/Plugins.txt`. Format: `*ModName.esp` per active plugin (asterisk = active), handled by `libloot GameType::Starfield`.
- **Do NOT list the 8 implicit base ESMs:** `Starfield.esm`, `Constellation.esm`, `OldMars.esm`, `BlueprintShips-Starfield.esm`, `SFBGS003.esm`, `SFBGS006.esm`, `SFBGS007.esm`, `SFBGS008.esm`. They are auto-active; libloot knows this.
- The game **rewrites `plugins.txt` on launch** (inserts any `Starfield.ccc` entries at top, strips implicit ESMs). Verify/repair must not flag this as tampering.
- Generate active flags from DB `plugin_state` **before** `load_current_load_order_state` (per the v1.0 plan 02-02 finding: libloot 0.29.5 has no active-plugin setter).

### 3. `.ba2` v3 archives

- No manager action for the common case. A mod's `ModName - Main.ba2` / `- Textures.ba2` auto-loads when its `ModName.esp` is active.
- Loose files + `plugins.txt` + the INI edit cover the ~95% third-party case. BA2 registration/unpack is a differentiator, not table stakes.

## Feature Dependencies

```
Starfield detection (AppID 1716740)
    └──requires──> CE2 AppData path resolver (Documents/My Games/Starfield)
                       ├──requires──> reversible StarfieldCustom.ini management ──(so loose files load)
                       └──requires──> plugins.txt @ CE2 path via libloot GameType::Starfield
                                          └──enhances──> LOOT sort (Starfield masterlist)

Reversible deploy engine (v1.0, reused) ──deploys──> mod loose files into game Data/
    └──must pair with──> StarfieldCustom.ini edit  (deploy without the INI = invisible mod)

BA2 awareness ──enhances──> loose-file activation (covers the non-loose tail)
Medium-master UI ──enhances──> load-order view  (display only)
```

### Dependency Notes

- **Path resolver blocks everything:** INI and plugins.txt both live at the CE2 `Documents/My Games/Starfield` path; resolving it correctly under Proton (with Wine case-folding) gates both table-stakes behaviors.
- **INI edit and deploy are co-dependent for the success criterion:** the milestone's pass/fail test ("a real mod deploys AND is visible in-game") fails if either the deploy or the INI edit is missing. They must ship together.
- **plugins.txt reuses v1.0's libloot seam** — the only new input is `GameType::Starfield` + the new `local_path`; the plan 02-02/02-04 machinery is otherwise unchanged.

## MVP Definition

### Launch With (v1.1)

- [ ] Starfield detection under Proton (AppID 1716740) — can't manage otherwise
- [ ] CE2 `Documents/My Games/Starfield` path resolver (INI + plugins.txt target) — gates both
- [ ] Reversible `StarfieldCustom.ini` archive-invalidation edit — **the headline new behavior**; loose files invisible without it
- [ ] Asterisk `plugins.txt` at CE2 path via `libloot GameType::Starfield`, base ESMs excluded — plugins inactive otherwise
- [ ] LOOT sort for Starfield — near-free on the existing seam
- [ ] Reversible deploy of Starfield mods (reused engine) + verify/repair tolerant of game-side plugins.txt rewrite
- [ ] On-hardware proof: a real Nexus Starfield mod deploys AND is visible in-game (owner has Starfield on Proton)

### Add After Validation (v1.x)

- [ ] BA2-packed-mod detection (register or unpack orphan BA2s) — trigger: users hit BA2-only mods
- [ ] "Will this load?" pre-flight (archive-invalidation off / missing master) warning — trigger: support noise about invisible mods
- [ ] Medium-master tier surfaced in the load-order UI — trigger: power-user request

### Future Consideration (v2+)

- [ ] Blueprint-plugin nuances (Ortham part-2 — build/outpost blueprint plugins) — defer until users report ordering issues
- [ ] Broader CE2 games if Bethesda ships more — defer until they exist

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Starfield detection (AppID 1716740) | HIGH | LOW | P1 |
| CE2 AppData path resolver | HIGH | MEDIUM | P1 |
| Reversible StarfieldCustom.ini edit | HIGH | MEDIUM-HIGH | P1 |
| plugins.txt via libloot GameType::Starfield | HIGH | LOW | P1 |
| LOOT sort (Starfield masterlist) | MEDIUM | LOW | P1 |
| Verify/repair tolerant of game rewrite | MEDIUM | LOW-MEDIUM | P1 |
| BA2-packed-mod awareness | MEDIUM | MEDIUM | P2 |
| "Will this load?" pre-flight | MEDIUM | MEDIUM | P2 |
| Medium-master tier in UI | LOW | LOW | P3 |

## Competitor Feature Analysis

| Behavior | Vortex | Mod Organizer 2 | NexusMods.App | NexTwist plan |
|----------|--------|-----------------|---------------|---------------|
| Loose-file activation | Auto-writes `StarfieldCustom.ini` archive-invalidation on Starfield deploy | Writes the INI edit / relies on profile-specific INI; historically also used the SFSE Plugins.txt Enabler pre-CK | Manages the INI as part of its managed-game handling | **Reversible, journaled** INI edit (backup + byte-for-byte restore) — the safety differentiator |
| plugins.txt | Manages asterisk plugins.txt | Manages per-profile plugins.txt | libloot-driven | libloot `GameType::Starfield`, base ESMs excluded, reuse v1.0 seam |
| Load-order sort | LOOT integration | LOOT integration | libloot native | libloot sort (already wired for SSE/FO4) |
| Base ESMs | Not listed (implicit) | Not listed (implicit) | Not listed | Not listed |
| Platform | Windows-first; Linux via community | Windows (USVFS) — Linux via wine/community, no native | Cross-platform target | **Native Linux/Proton, real link-based deploy** (USVFS is Windows-only — already an out-of-scope decision) |

**Minimum a manager MUST automate (consensus across all three):** (1) write the `StarfieldCustom.ini`
`[Archive] bInvalidateOlderFiles=1` + `sResourceDataDirsFinal=` edit, and (2) write the asterisk
`plugins.txt` with the base ESMs excluded, both at the `Documents/My Games/Starfield` path. If a
manager does only these two plus deploy, third-party mods install and show in-game.

## Sources

- Ortham (libloot/libloadorder author), "Load order in Starfield" — https://blog.ortham.net/posts/2024-06-28-load-order-in-starfield/ (**authoritative** for plugins.txt, implicit ESMs, medium plugins, .ccc rewrite, esplugin v6/libloadorder v17 Starfield support)
- Ortham, "Load order in Starfield, part 2: blueprint plugins" — https://blog.ortham.net/posts/2024-07-30-blueprint-plugins/
- Nexus mod 273 "Base StarfieldCustom.ini to Enable Loose File Mods" — https://www.nexusmods.com/starfield/mods/273
- modding.wiki, "Loose File Modding" (Starfield) — https://modding.wiki/en/starfield/users/loose-file-modding
- Nexus article 116 "Howto: Archive Invalidation" — https://www.nexusmods.com/starfield/articles/116
- Nexus article 578 "Best Practices with Loose Files" — https://www.nexusmods.com/starfield/articles/578
- ProtonDB / RetroResolve / Steam Deck HQ — Proton `compatdata/1716740/.../Documents/My Games/Starfield` path — https://www.protondb.com/app/1716740 , https://retroresolve.com/guides/how-to-install-starfield-mods-on-steam-deck/
- Internal: `.planning/milestones/v1.0-phases/02-multi-mod-management/02-02-SUMMARY.md` (verified libloot 0.29.5 API incl. `GameType::Starfield`, `with_local_path` local-path semantics, no active-plugin setter)

---
*Feature research for: Starfield (Creation Engine 2) support in NexTwist*
*Researched: 2026-07-07*
