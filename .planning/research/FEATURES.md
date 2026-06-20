# Feature Research

**Domain:** Linux desktop NexusMods mod manager (Vortex/MO2/NexusMods.App-class) for Proton/Wine games, Bethesda Creation Engine first
**Researched:** 2026-06-20
**Confidence:** MEDIUM (cross-checked across DeepWiki, official LOOT/Nexus/UESP wikis, modding.wiki, MO2/Vortex GitHub; no single curated source covers all claims)

## Context note (important for the roadmap)

**NexusMods.App was archived 2026-02-20 and is read-only.** It is still the best *design* reference (Loadout model, event-sourcing for undo, SQLite, FS abstraction, Wine/Proton detection design) but it is no longer a moving target and its game roster was *not* Bethesda-first (Stardew Valley, Cyberpunk 2077, BG3, Bannerlord). NexTwist's Bethesda-first + Linux/Proton focus is genuinely uncontested ground — neither Vortex (Windows-centric) nor NexusMods.App (now dead, non-Bethesda-first) nor MO2 (Windows USVFS, no native Linux) fully occupies it.

**Key technical constraint that shapes every feature below:** MO2's USVFS is a *Windows-only, process-local API-hooking* virtual filesystem. It is **not viable on Linux/Proton**. NexTwist must achieve the "vanilla game folder stays pristine" property through real-filesystem deployment (hardlink/symlink) plus a manifest — i.e. the **Vortex model**, not the MO2 model. This is the single biggest architecture-driving feature decision.

## Feature Landscape

### Table Stakes (Users Expect These)

Missing any of these and a Vortex-migrating user will conclude NexTwist "isn't a real mod manager."

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| NexusMods account login (OAuth2 via system browser; API key fallback) | Can't download without it; users expect "Sign in with Nexus" | MEDIUM | Vortex uses OAuth2 (system browser) + legacy API-key. OAuth needs a loopback/redirect catch; Tauri can open the browser and run a tiny local listener. Store tokens in OS keyring. |
| Free vs Premium download handling | Free users are the majority; must work for them without misleading them | MEDIUM | Premium: direct CDN links via API (no friction, server choice). Free: download key is generated **only on the website** and embedded in the `nxm://` link, with an expiry. No fully-automated free download — free flow is inherently website-initiated + a confirm step. Must not pretend otherwise. |
| One-click install via `nxm://` links | The "Mod Manager Download" button is THE install path on nexusmods.com | MEDIUM | Register an `nxm://` URL scheme handler (Linux: `.desktop` MimeType `x-scheme-handler/nxm`). Parse `gameId/modId/fileId` (+ `key`/`expires` for free users). Route to a running app instance, resolve CDN link, queue download. Linux precedent: `nxm-handler`, MO2-linux-installer. |
| Individual mod download + install from archive | Core loop; without it nothing works | MEDIUM | Download to a downloads cache, extract to a per-mod staging folder. Handle 7z/zip/rar. |
| Non-destructive deployment (staging + link into game) | PROJECT Core Value: base game never directly corrupted | HIGH | Vortex model: mods live in staging; deployment hardlinks/symlinks them into the game data dir. Game folder is never the source of truth. |
| Deployment manifest + full purge / reversibility | PROJECT Core Value: fully reversible, pristine restore | HIGH | Vortex's `__vortex_deployment.json` records every deployed file + source mod + hash + method. Purge inverts it, deleting ONLY files it deployed (never game/user files). This is the safety contract — must be bulletproof and crash-safe. |
| Hardlink deployment (same-volume) | Native perf, zero extra disk, no game-side awareness | HIGH | Requires staging and game dir on same filesystem/partition. On Linux this also crosses into the Steam library / Proton prefix layout — must detect and warn when staging and game are on different mounts. |
| Symlink deployment (cross-volume fallback) | Steam libraries often on a different drive than home | MEDIUM | Works across drives. On Linux no admin needed (unlike Windows). Risk: some games/anti-tamper resolve symlinks oddly; Proton generally fine for Bethesda. |
| Conflict detection + overwrite winner resolution | Two mods touching the same file is the norm; user must know who wins | HIGH | Detect files provided by >1 mod; resolve via load order / priority / explicit rules before deployment. Surface conflicts in UI. Vortex resolves "which mod wins" in its linking layer. |
| Load order / mod priority management | Determines overwrite order; non-negotiable for modding | MEDIUM | Per-profile ordered list; drag-to-reorder or rule-based. Distinct from *plugin* load order (below). |
| Multiple profiles per game | Different playthroughs / mod sets without re-downloading | MEDIUM | MO2-class isolation: independent enabled-mod set, load order, plugin order, INIs; optionally per-profile saves. Switching a profile re-derives deployment. |
| Plugin (.esp/.esm/.esl) load order management | Bethesda games are unplayable with wrong plugin order | HIGH | Separate from mod priority. ESM pinned top → ESL → ESP. 254 (0xFE) full-plugin limit + up to 4096 ESL light plugins. Must parse plugin headers for masters and ESL flag. |
| Automated plugin sorting (LOOT / libloot) | Manual plugin sorting is error-prone; everyone uses LOOT | HIGH | Integrate **libloot** + community masterlist + user rules. Topological sort respecting masters→non-masters and group ordering. This is a hard dependency users assume exists. |
| FOMOD scripted installer support | A large fraction of major mods ship as FOMOD; install fails without it | HIGH | Parse `fomod/ModuleConfig.xml`: install steps, option groups, flags, conditional steps, conditional file installs (deps + logic operators). Render the chooser UI; persist chosen options. |
| Game/version detection (Steam/Proton on Linux) | Must find the install + know which game/version to mod | MEDIUM | Parse Steam `libraryfolders.vdf` + `appmanifest_*.acf`; map appid → game; locate Proton prefix; read game version/build. Detect data dir + plugins.txt/loadorder location inside the prefix. |
| Collections install (revision + manifest model) | The modern Vortex-defining feature; "one-click curated list" is in PROJECT scope | HIGH | See concrete flow below. Spans download, FOMOD presets, install phases/order, bundled mods, binary patches, then deploy + plugin sort. Largest single feature. |
| Backup/restore of load order & profile state | Users fear bricking a 200-mod setup | MEDIUM | Snapshot enabled mods + mod order + plugin order + INIs. Distinct from "purge" (which restores the *game folder*). |
| Mod enable/disable toggle (without re-extract) | Iterating on a setup must be fast | LOW | Because staging is decoupled from deployment, toggling just re-derives the deployed set. |
| Update detection for installed mods | Mods update constantly; stale mods break collections | MEDIUM | Compare installed file/version vs Nexus API latest. Surface "updates available." |
| External-change detection | Game/other tools write into the data dir; must not silently clobber | MEDIUM | Vortex: Import / Drop / Restore choices when files change outside the manager. Protects the manifest's integrity. |

### Differentiators (Competitive Advantage)

Where NexTwist competes. Aligned with PROJECT Core Value (safe, reversible, Linux-native).

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| First-class Linux/Proton-native deployment | Vortex on Linux is a Wine-in-Wine hack; MO2 needs USVFS workarounds. A native Rust app that *understands* Proton prefixes, case-sensitivity, and Steam layout is the whole reason to exist | HIGH | Detect prefix, handle case-folding mismatches (Windows-case-insensitive vs ext4-case-sensitive), pick hardlink vs symlink per mount automatically. THE differentiator. |
| Crash-safe / atomic, transactional deployment | "It must hold if everything else fails" — go beyond Vortex by making deploy/purge resumable after a kill -9 | HIGH | Write-ahead manifest, two-phase apply, fsync discipline. Rust gives strong FS control. Event-sourcing (NexusMods.App's undo model) is a proven pattern to borrow. |
| Case-sensitivity reconciliation for Proton | Bethesda assets are Windows-case-insensitive; ext4 isn't — a frequent silent breakage on Linux | MEDIUM | Normalize/alias paths; optionally use a casefold dir or a per-prefix case map. Big quiet win Windows tools never had to solve. |
| Health Check / pre-launch validation | Borrow NexusMods.App's Health Check: catch missing masters, broken FOMOD output, plugin-limit overflow before launch | MEDIUM | Validate masters present, ESL/plugin counts under limits, no orphaned overwrites. |
| Loadout/event-sourced undo | "Undo my last 3 changes" is more reassuring than backup files | HIGH | NexusMods.App proved the pattern. Lets reversibility be granular, not just all-or-nothing purge. |
| Dry-run conflict preview before deploy | Show exactly which files will be overwritten and by whom *before* committing | MEDIUM | Cheap given the manifest model; builds the trust the Core Value promises. |
| Single portable AppImage, no Wine needed for the manager itself | Native binary vs running a Windows mod manager under Wine | MEDIUM | Rust+Tauri delivers this; the manager runs natively, only the *game* uses Proton. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Reimplementing MO2-style USVFS on Linux | "MO2 keeps the game folder 100% untouched" | USVFS is Windows API-hooking, process-local, Windows-only. An equivalent on Linux (FUSE/overlayfs/LD_PRELOAD into Proton) is fragile, breaks under Proton's own Wine layering, and fights anti-cheat/launch flows | Vortex-style real hardlink/symlink deployment + manifest. Achieves "pristine restore" via purge, which is the property users actually need. (overlayfs is a *possible* future research item but not v1.) |
| Built-in game launcher replacement | "Launch modded game from the manager" | Steam already launches via Proton; intercepting it on Linux is brittle and out of PROJECT scope | Deploy, then let Steam launch normally. Optionally offer a "deploy then launch via Steam URI" convenience. |
| Multi-source mod aggregation (ModDB, GameBanana, etc.) in v1 | "Manage all my mods in one place" | Each source has different auth, archive conventions, no checksum manifest — explodes the safety model PROJECT is built on | NexusMods-only for v1 (explicit PROJECT scope). Generic "install from local archive" covers edge cases. |
| Authoring/hosting mods or Collections | "Let me publish my collection from here" | NexTwist consumes the catalog; authoring is a separate product surface and out of scope | Consume Collections only; link out to the website for authoring. |
| Move/copy deployment as a primary method | "Just copy files in, simplest" | Move makes the game folder the source of truth → destroys reversibility (you can't tell mod files from game files), and copy wastes disk. Both undermine Core Value | Hardlink primary, symlink fallback. Reserve copy only as a last-resort for exotic filesystems, clearly flagged as less safe. |
| Auto-installing every Collection mod with zero prompts | "True one-click" | Free-user downloads legally/technically require website-initiated keyed links + confirmation; FOMOD choices sometimes need user input; silent install hides conflicts | Automate everything that *can* be automated (premium, preset FOMOD options, phases) and prompt only where required. Be honest about free-user friction. |
| Editing/cleaning plugins (xEdit-style) inside the app | "One tool to do everything" | Plugin record editing is a deep, separate domain (xEdit); scope explosion and high risk to save integrity | Integrate *sorting* (LOOT) and *validation* only; defer record editing to external tools. |
| Native Windows/macOS build | "Cross-platform reach" | The product's entire reason to exist is Linux/Proton; cross-platform dilutes focus (explicit PROJECT non-goal) | Linux-only v1. |

## Collections install flow (concrete)

A Collection is **a list of mods plus metadata, interpreted by the mod manager** — the heavy lifting is client-side. Each edit on Nexus creates a new incrementally-numbered **revision**; users install a specific revision.

1. **Acquire the revision + manifest.** Download the collection archive for the chosen revision. The manifest enumerates each constituent mod (Nexus game/mod/file IDs), per-file **checksums**, install **phases/order**, required vs optional/recommended designation, and any saved **FOMOD option presets**.
2. **Resolve & download each mod.** For each manifest entry, resolve the Nexus file and download it (premium → direct CDN; free → keyed website-initiated link, confirm-per-mod). Verify downloaded archive files against the manifest **checksums**; only matching files are accepted/installed to their recorded paths.
3. **Handle bundled mods and patches.** **Bundled** mods (those distributed inside the collection rather than via Nexus) are imported directly with their metadata (name/version/author). **Binary patches** are applied on top of a base mod (collections use a ~20% threshold: if a change touches >20% of a mod it is bundled rather than patched).
4. **Apply FOMOD presets.** Where the manifest stored installer option choices, run the FOMOD installer **headlessly using the preset flags** instead of prompting the user.
5. **Order via install phases.** Install/stage mods respecting the collection's defined phases/order so dependencies land before dependents; map collection order onto NexTwist's mod priority.
6. **Deploy + sort plugins.** Run normal non-destructive deployment (hardlink/symlink + manifest), then run LOOT/libloot to sort the plugin (.esp/.esm/.esl) load order. Run Health Check before declaring success.

**Dependency implication:** Collections is a *composition* of nearly every other feature (download, checksum verify, FOMOD presets, bundle/patch apply, mod ordering, deployment, plugin sort). It must be the **last** major feature, after all of those exist.

## Feature Dependencies

```
NexusMods OAuth login
    └──requires──> OS keyring / token storage
    └──enables──> Mod download (free + premium paths)
                     └──requires──> Download cache + archive extraction
                     └──requires──> nxm:// URL scheme handler (one-click)

Game/version detection (Steam/Proton/prefix)
    └──requires──> staging-vs-game same-mount detection
                     └──decides──> hardlink (same vol) vs symlink (cross vol)

Non-destructive deployment
    └──requires──> staging folder model
    └──requires──> deployment manifest  ──enables──> purge / reversibility
    └──requires──> conflict detection ──requires──> mod priority / load order

Profiles ──enhances──> mod load order + plugin load order + INIs

Plugin load order management
    └──requires──> plugin header parsing (masters, ESL flag, format)
    └──requires──> libloot + masterlist  (automated sorting)

FOMOD installer support
    └──requires──> archive extraction
    └──enables──> Collections (FOMOD presets)

Collections install
    └──requires──> mod download (free+premium) + checksum verify
    └──requires──> FOMOD preset application
    └──requires──> bundled-mod import + binary-patch apply
    └──requires──> mod ordering (phases) + deployment + plugin sort
    └──requires──> Health Check (validation)

USVFS-on-Linux ──conflicts──> Linux/Proton reality (Windows-only) → DO NOT BUILD
Move/copy-as-primary ──conflicts──> reversibility (Core Value) → fallback only
```

### Dependency Notes

- **Deployment manifest enables purge:** reversibility is impossible without a precise record of what was deployed; the manifest IS the safety contract.
- **Conflict detection requires load order:** "who wins" is only definable once mods are ordered; ordering and conflict resolution ship together.
- **Plugin load order is separate from mod priority:** mod priority decides *file* overwrites at deploy time; plugin order decides *record* precedence at game runtime. Both are needed for Bethesda; do not conflate them.
- **Collections depends on almost everything:** it is a composition feature and must come last.
- **Hardlink vs symlink is decided by mount detection:** game/version detection must report whether staging and game dir share a filesystem before deployment can choose a method.

## MVP Definition

### Launch With (v1) — the PROJECT core loop: login → download → deploy → manage order

- [ ] NexusMods OAuth2 login + token storage — gates everything
- [ ] Free + premium download handling + `nxm://` one-click handler — the real install path
- [ ] Individual mod download + archive extract to staging — core loop
- [ ] Steam/Proton game + version detection (Skyrim SE, Fallout 4) incl. same-mount detection — must find the game safely
- [ ] Non-destructive deployment: hardlink primary, symlink fallback, **+ manifest** — Core Value
- [ ] Full purge / pristine restore — Core Value (the one thing that must always hold)
- [ ] Conflict detection + mod priority/load order — users must control overwrites
- [ ] Multiple profiles per game — explicit PROJECT requirement
- [ ] Plugin (.esp/.esm/.esl) load order + libloot/LOOT sorting — Bethesda games unplayable without it
- [ ] FOMOD scripted installer support — many major mods require it
- [ ] Collections install (revision/manifest, phases, FOMOD presets, bundles/patches) — explicit PROJECT scope, the modern defining feature
- [ ] AppImage packaging — explicit PROJECT distribution requirement

### Add After Validation (v1.x)

- [ ] Health Check / pre-launch validation — add once deployment is trusted
- [ ] External-change detection (Import/Drop/Restore) — add when users start hand-editing
- [ ] Mod update detection — add once libraries grow large
- [ ] Dry-run conflict preview before deploy — polish the trust story
- [ ] Backup/restore of profile state (separate from purge) — add when users have big setups to protect

### Future Consideration (v2+)

- [ ] Event-sourced granular undo (NexusMods.App Loadout model) — large architecture investment; revisit once core is stable
- [ ] overlayfs-based deployment experiment — only if hardlink/symlink prove insufficient on Linux
- [ ] Additional games beyond Bethesda CE — after Skyrim SE/FO4 validated
- [ ] Case-sensitivity auto-reconciliation tooling — promote from "handle inline" to a first-class feature if it proves common

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| OAuth login + token storage | HIGH | MEDIUM | P1 |
| Free/premium download + nxm:// handler | HIGH | MEDIUM | P1 |
| Mod download + extract to staging | HIGH | MEDIUM | P1 |
| Steam/Proton game+version detection | HIGH | MEDIUM | P1 |
| Non-destructive deployment + manifest | HIGH | HIGH | P1 |
| Purge / pristine restore | HIGH | HIGH | P1 |
| Conflict detection + mod priority | HIGH | HIGH | P1 |
| Profiles per game | HIGH | MEDIUM | P1 |
| Plugin load order + LOOT sorting | HIGH | HIGH | P1 |
| FOMOD installer support | HIGH | HIGH | P1 |
| Collections install | HIGH | HIGH | P1 (last) |
| AppImage packaging | MEDIUM | LOW | P1 |
| Health Check / validation | MEDIUM | MEDIUM | P2 |
| External-change detection | MEDIUM | MEDIUM | P2 |
| Mod update detection | MEDIUM | MEDIUM | P2 |
| Dry-run conflict preview | MEDIUM | MEDIUM | P2 |
| Profile backup/restore | MEDIUM | MEDIUM | P2 |
| Event-sourced undo | MEDIUM | HIGH | P3 |
| overlayfs deployment experiment | LOW | HIGH | P3 |

**Priority key:** P1 = must have for launch · P2 = should have, add when possible · P3 = future.

Note: the v1 P1 list is large because the PROJECT explicitly scopes Collections + profiles + load-order as *core*. Collections, plugin sorting, FOMOD, and deployment-with-purge are each genuinely HIGH cost; expect the roadmap to need several phases for the deployment/safety core alone before Collections can land.

## Competitor Feature Analysis

| Feature | Vortex | Mod Organizer 2 | NexusMods.App (archived) | NexTwist approach |
|---------|--------|------------------|--------------------------|-------------------|
| Deployment | Hardlink/symlink/move into game dir + manifest | USVFS virtual merge (no real files) | FS-abstraction + loadout sync | Hardlink/symlink + manifest (Vortex model); USVFS rejected as Windows-only |
| Reversibility | Purge via manifest | Inherent (nothing written) | Event-sourced undo | Purge via crash-safe manifest; event-sourced undo as v2 |
| Profiles | Yes | Yes (strong: INIs, saves) | Loadouts | Per-game profiles (MO2-strength isolation) |
| Plugin sort | Built-in LOOT | External LOOT integration | LOOT | libloot integrated |
| Collections | Native (defining feature) | Plugin (collection-dl) | Native | Native, v1 scope |
| FOMOD | Yes | Yes | Yes | Yes |
| nxm:// one-click | Yes | Yes (Win); Linux via handler | Yes | Native Linux URL-scheme handler |
| Linux support | Runs under Wine (hacky) | No native; community USVFS hacks | Yes (Windows+Linux) | **Native Linux/Proton — the differentiator** |
| Bethesda-first | General | Bethesda-centric heritage | Non-Bethesda roster | **Bethesda CE first (Skyrim SE/FO4)** |

## Sources

- NexusMods.App repo + docs (archived 2026-02-20): https://github.com/Nexus-Mods/NexusMods.App , https://nexus-mods.github.io/NexusMods.App/
- Vortex mod deployment (DeepWiki): https://deepwiki.com/Nexus-Mods/Vortex/3.2-mod-deployment
- Vortex Nexus API / auth / premium-vs-free / nxm (DeepWiki): https://deepwiki.com/Nexus-Mods/Vortex/6.1-nexus-api
- Vortex deployment methods (modding.wiki / Vortex wiki): https://modding.wiki/en/vortex/users/deployment-methods , https://github.com/Nexus-Mods/Vortex/wiki/MODDINGWIKI-Users-General-Deployment-Methods
- NexusMods Collections (modding.wiki + Nexus help + feature-list issue): https://modding.wiki/en/nexusmods/collections/FAQ , https://help.nexusmods.com/article/115-guidelines-for-collections , https://github.com/Nexus-Mods/NexusMods.App/issues/3013
- MO2 USVFS: https://github.com/ModOrganizer2/usvfs , https://stepmodifications.org/wiki/Guide:Mod_Organizer/Advanced
- nxm:// link handling on Linux: https://github.com/luluco250/nxm-handler , https://deepwiki.com/rockerbacon/modorganizer2-linux-installer/4.1-nxm-link-handling
- FOMOD scripted installer: https://fomod-docs.readthedocs.io/en/latest/tutorial.html , https://github.com/wrye-bash/wrye-bash/wiki/%5Bdev%5D-Fomod-for-Devs
- LOOT / libloot sorting + masterlist: https://loot.github.io/ , https://loot-api.readthedocs.io/en/latest/api/sorting.html , https://github.com/loot/loot
- Bethesda plugin format / ESL / 254 limit: https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format , https://modding.wiki/en/skyrim/users/plugin-load-order
- node-nexus-api (download keys/expiry, premium): https://github.com/Nexus-Mods/node-nexus-api

---
*Feature research for: Linux NexusMods mod manager (Bethesda-first, Proton/Wine)*
*Researched: 2026-06-20*
