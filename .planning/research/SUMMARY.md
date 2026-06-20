# Project Research Summary

**Project:** NexTwist
**Domain:** Linux desktop NexusMods mod manager (Rust + Tauri) for Windows games run via Steam Proton / Wine
**Researched:** 2026-06-20
**Confidence:** MEDIUM-HIGH

## Executive Summary

NexTwist is a Vortex/MO2-class mod manager built natively for Linux, targeting Windows games run through Steam Proton/Wine, with Bethesda Creation Engine games (Skyrim SE, Fallout 4) as the beachhead. Experts in this space (Vortex, Mod Organizer 2, the now-archived NexusMods.App) all converge on one fundamental design: **mods never touch the game folder directly; they live in a managed staging store, and a deployment engine projects them into the game via filesystem links, recording every change in a manifest so it can be exactly undone.** The single most important architectural decision flowing from the research is that MO2's USVFS virtual filesystem is Windows-only and not viable on Linux — NexTwist must use the **Vortex model** (real hardlink/symlink deployment + manifest), not the MO2 model. This is uncontested ground: no existing tool fully occupies the Bethesda-first, Linux/Proton-native niche.

The recommended stack is Rust 1.85+ (2024 edition) with Tauri 2.11 (system WebView, native AppImage out of the box), Svelte 5 frontend (smallest bundle for the slower WebKitGTK runtime; React acceptable fallback), tokio + reqwest (rustls-tls) for the NexusMods API, and bundled SQLite via rusqlite for the catalog, profiles, load order, and the all-important per-file deployment ledger. Deployment should use a pluggable method trait choosing reflink → hardlink → symlink → copy per-target based on a runtime filesystem probe. The architecture must keep a **pure, headless, heavily-tested Rust core** (the deployment engine is the crown jewel) with Tauri commands as a thin adapter only — so the safety-critical reversibility logic can be property-tested on temp dirs in CI without a webview.

The dominant risks are all in the deployment/safety core and in Linux/Proton platform realities: hardlinks failing across filesystem/btrfs-subvolume/Proton-drive boundaries (EXDEV); overwriting vanilla files in place with no backup (unrecoverable corruption); non-atomic manifests leaving orphans so "purge" doesn't truly restore pristine state; Wine case-sensitivity mismatches that make mods silently fail to load; and writing load order to the wrong Proton prefix. Mitigation is to front-load the deployment engine with a write-ahead journal, backup-before-overwrite, verify/repair from day one, an empirical fs-capability probe, and casefold handling — and to build the entire differentiating safety story (steps 1–5 below) end-to-end *before* any NexusMods networking exists, since the API surface is replaceable but deployment correctness is the reason the product exists.

## Key Findings

### Recommended Stack

Rust + Tauri is fixed by the project owner and is well-justified: Tauri v2 gives ~10-20 MB AppImage bundles using the system WebView (a hard project requirement) and Rust gives the precise filesystem control the reversibility guarantee needs. Crate versions are HIGH confidence (verified against crates.io); API/auth recommendations are MEDIUM (cross-checked web + NexusMods.App docs).

**Core technologies:**
- **Rust 1.85+ / Tauri 2.11**: backend + desktop shell — native AppImage, small bundles, strong FS control
- **Svelte 5 + TypeScript**: frontend — smallest bundle/fastest cold start for WebKitGTK (React is fallback)
- **tokio 1.52 + reqwest 0.13 (rustls-tls)**: async runtime + NexusMods HTTP client — self-contained, no system OpenSSL dep
- **rusqlite 0.40 (bundled SQLite) + refinery**: local DB for catalog/profiles/load-order/**deploy ledger** — statically linked, clean AppImage
- **reflink-copy + walkdir + steamlocate + keyvalues-serde**: deployment primitives + Steam/Proton discovery
- **tauri-plugin-deep-link + single-instance, oauth2 + keyring, governor**: `nxm://` handling, OAuth2+PKCE login with OS-keyring token storage, client-side rate limiting
- **zip + sevenz-rust2**: archive extraction (avoid the non-free `unrar` crate; enforce with `cargo-deny`)

### Expected Features

**Must have (table stakes) — the v1 core loop login → download → deploy → manage order:**
- NexusMods OAuth2 login + OS-keyring token storage (gates everything)
- Free + premium download handling + `nxm://` one-click handler (the real install path)
- Mod download + archive extract to per-mod staging
- Steam/Proton game + version detection incl. same-mount detection (Skyrim SE, Fallout 4)
- Non-destructive deployment (hardlink primary, symlink fallback) **+ manifest** — Core Value
- Full purge / pristine restore — Core Value, the one thing that must always hold
- Conflict detection + mod priority/load order
- Multiple profiles per game
- Plugin (.esp/.esm/.esl) load order + libloot/LOOT sorting (Bethesda unplayable without it)
- FOMOD scripted installer support
- Collections install (revision/manifest, phases, FOMOD presets, bundles/patches) — composition of nearly every other feature, **must come last**
- AppImage packaging

**Should have (competitive, v1.x):**
- Health Check / pre-launch validation (missing masters, plugin-limit overflow)
- External-change detection (Import/Drop/Restore)
- Mod update detection
- Dry-run conflict preview before deploy
- Crash-safe/atomic transactional deploy and case-sensitivity reconciliation are *core differentiators* baked into the engine, not bolt-ons

**Defer (v2+):**
- Event-sourced granular undo (NexusMods.App Loadout model) — large architecture investment
- overlayfs-based deployment experiment
- Games beyond Bethesda CE
- Anti-features to reject outright: USVFS-on-Linux, built-in launcher replacement, multi-source aggregation, mod authoring, copy-as-primary deployment

### Architecture Approach

All mod managers split into a managed **staging store** plus a **deployment engine** that links staging into the game and records a manifest for exact undo; everything else (UI, API client, profiles, load order) orbits that core. The decisive call: keep the Rust core fully independent of Tauri (workspace of `crates/`), with `#[tauri::command]` functions as thin 3-10 line adapters, so the safety-critical engine is unit/property-testable headless.

**Major components:**
1. **Deployment Engine** (`crates/deploy/`) — the crown jewel: conflict resolution by load order, pluggable hardlink/symlink/copy method trait, three-way synchronizer, manifest-driven deploy/purge/verify, casefold handling
2. **Steam/Proton resolver** (`crates/steam/`) — quarantines all Proton/Steam-layout knowledge; hands the engine already-resolved absolute paths (install dir + prefix AppData)
3. **NexusMods API client** (`crates/nexus/`) — OAuth, GraphQL v2 + REST v1, `nxm://` parsing, CDN URL resolution, Collection revisions, rate limiting
4. **Download manager + Archive/FOMOD extractor** (`crates/download/`, `crates/extract/`) — resumable downloads → cache → safe extraction → staging
5. **Persistence** (`crates/store/`) — SQLite DB + on-disk staging store + deploy manifest

### Critical Pitfalls

1. **Overwrite destroys vanilla files** — back up any pre-existing game file into a per-game original-store *before* overwriting, record it in the manifest; purge restores it. The single most important safety mechanism; corruption here is unrecoverable except via Steam re-verify.
2. **Non-atomic manifest / orphans → purge doesn't restore pristine** — use a write-ahead journal, content hash + provenance per file, idempotent purge, and a verify/repair command run automatically after abnormal exit. Bake in from the start; retrofitting is painful.
3. **Hardlink EXDEV across fs/btrfs-subvolume/Proton boundaries** — empirically probe `link()` capability and `st_dev` at setup; force staging onto the same fs+subvolume as the game; choose method per-target at runtime, never globally.
4. **Wine case-sensitivity mismatch → mods silently don't load** — deploy into an ext4 `casefold` (+F) tree or normalize mod path casing against per-game canonical directory casing.
5. **Wrong Proton prefix for plugins.txt/load order** — derive `compatdata/<APPID>/pfx/.../steamuser/AppData/Local/<Game>` from `libraryfolders.vdf` + `appmanifest_*.acf`; handle Flatpak/Snap roots; re-resolve each session.
6. **Zip-slip / malicious-symlink archive extraction (RCE)** — canonicalize + bounds-check every entry, reject `..`/absolute/symlink entries, pin non-vulnerable crate versions, extract to temp then move; add a crafted-archive test fixture.

## Implications for Roadmap

The architecture research's "Suggested Build Order" is the strongest signal here: **build the differentiating safety core end-to-end before any NexusMods networking.** This de-risks the project because the API is replaceable but deployment correctness is the entire reason to exist. Suggested phases:

### Phase 1: Foundation — Core Model + Store + Steam/Proton Discovery
**Rationale:** No dependencies; everything else needs the data model and resolved game paths.
**Delivers:** App detects Steam libraries, resolves a Bethesda game's install dir + Proton prefix paths, lists detected games with paths.
**Addresses:** Steam/Proton game + version detection (table stakes).
**Avoids:** Pitfall 6 (wrong prefix) — establish robust prefix resolution incl. Flatpak/Snap; Pitfall 1 (capture `st_dev`/fs type per game for later method selection).

### Phase 2: Staging Store + Safe Archive Extraction
**Rationale:** Deployment needs something to deploy; depends on Phase 1.
**Delivers:** Point at a local archive → safely extract into per-mod staging → DB rows; mod appears in list with files enumerated.
**Uses:** zip + sevenz-rust2; rusqlite.
**Avoids:** Pitfall 7 (zip-slip) — canonicalize/bounds-check, reject symlinks/abs/`..`, test fixture, extract-to-temp-then-move.

### Phase 3: Deployment Engine — Deploy + Purge + Manifest (single method)
**Rationale:** The crown jewel and Core Value; depends on Phases 1-2. **Build the most tests here.**
**Delivers:** Deploy one mod (hardlink + copy fallback), see links in `Data/`, purge, folder byte-for-byte pristine. Write-ahead journal, vanilla backup-before-overwrite, verify/repair.
**Implements:** Deployment Engine + manifest persistence.
**Avoids:** Pitfalls 3 (overwrite destroys vanilla), 4 (orphans/non-atomic purge), 1 (EXDEV probe + method abstraction).

### Phase 4: Conflict Resolution + Load Order
**Rationale:** Multiple mods make the manifest meaningful; depends on Phase 3.
**Delivers:** Multiple mods, winner-by-priority, conflict data for UI, reorder → redeploy.
**Addresses:** Conflict detection + mod priority/load order (table stakes).

### Phase 5: Three-Way Sync + Symlink Method + Casefold + Plugin Load Order/LOOT
**Rationale:** Completes the Linux/Proton-native safety story and Bethesda playability; depends on Phases 3-4.
**Delivers:** Delta deploys, external-change detection, cross-volume symlink deployment, case correctness, plugin (.esp/.esm/.esl) ordering via libloot.
**Addresses:** Plugin load order + LOOT sorting (table stakes); case-sensitivity reconciliation (differentiator).
**Avoids:** Pitfalls 2 (symlink-not-followed), 5 (case sensitivity), 12 (archive invalidation), 13 (Steam-update staleness).

### Phase 6: NexusMods API Client + OAuth + Download Manager
**Rationale:** First networking; safe deployment already proven. Depends on Phase 2 (feeds the extractor).
**Delivers:** OAuth2+PKCE login, keyring token storage, `getDownloadURLs`, resumable streamed downloads into cache.
**Uses:** reqwest, oauth2, keyring, governor.
**Avoids:** Pitfalls 9 (Premium-gated free-user flow), 14 (IPC blocking — stream + events), 15 (token storage; no Stronghold).

### Phase 7: `nxm://` One-Click Handler
**Rationale:** Wires website install path to download → extract → deploy; depends on Phase 6.
**Delivers:** One-click "Mod Manager Download" from nexusmods.com routed to the running app.
**Avoids:** Pitfall 10 (fragile Linux handler registration, AppImage path stability).

### Phase 8: FOMOD Installer + Collections Install
**Rationale:** Collections is a composition of nearly everything; must be last. Depends on Phases 4, 6, 7.
**Delivers:** FOMOD scripted installer (ModuleConfig.xml + presets), full Collections flow (revision/manifest, checksum verify, phases, bundles/patches, deploy, plugin sort, Health Check).
**Avoids:** Pitfall 11 (archived mods/version drift/FOMOD automation — dry-run resolve before touching disk).

### Phase 9: Profiles + AppImage Packaging + License Audit
**Rationale:** Profile switch re-runs resolve+deploy across the now-complete pipeline; packaging closes v1.
**Delivers:** Multiple profiles per game; distributable AppImage with registered MIME handler.
**Avoids:** Pitfall 8 (RAR/extractor license audit via cargo-deny).

### Phase Ordering Rationale

- **Safety-first, networking-last:** Phases 1-5 deliver the differentiating reversible-deploy story before any API exists, per the architecture build order — the highest-risk, highest-value work first.
- **Dependency-driven:** Collections depends on download + FOMOD + deployment + ordering + plugin sort, so it lands last; conflict resolution requires load order so they ship together.
- **Pitfall front-loading:** the unrecoverable pitfalls (overwrite, orphans, EXDEV) all sit in Phase 3, which is explicitly the most-tested phase.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 3 (Deployment Engine):** crash-safe journaling/atomicity design, EXDEV probe, vanilla-backup semantics — safety-critical, needs `--research-phase`.
- **Phase 5 (case/Proton/three-way sync):** ext4 casefold mechanics, Proton-specific symlink/archive-invalidation behavior, libloot integration — sparse/converging community sources.
- **Phase 6 (NexusMods API):** GraphQL v2 vs deprecated v1 coverage, OAuth2+PKCE exact flow, free-vs-premium download keying, Acceptable Use Policy app registration — API in flux, verify per-endpoint.
- **Phase 8 (FOMOD/Collections):** FOMOD ModuleConfig.xml conditional logic + headless preset replay, Collections manifest/bundle/patch format — largest single feature, known recurring bugs.

Phases with standard patterns (skip research-phase):
- **Phase 1-2:** well-documented (steamlocate, VDF/ACF parsing, archive extraction) — apply known zip-slip mitigations.
- **Phase 9 (Profiles/AppImage):** established Tauri bundling + profile re-derivation patterns.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Crate versions verified against crates.io; Tauri/Svelte choices cross-checked. API-client specifics MEDIUM. |
| Features | MEDIUM | Cross-checked across DeepWiki, LOOT/Nexus/UESP wikis, MO2/Vortex GitHub; no single curated source covers all claims. |
| Architecture | MEDIUM-HIGH | Deployment + NexusMods.App model verified against primary sources/ADRs; some Proton/case-folding details inferred from converging community sources. |
| Pitfalls | MEDIUM | Most findings cross-checked against official wikis, GitHub issues, kernel docs; a few web-search-only items flagged LOW. |

**Overall confidence:** MEDIUM-HIGH

### Gaps to Address

- **NexusMods API surface in flux (v1 REST deprecating → GraphQL v2):** verify per-endpoint at planning time for Phase 6; prefer v2, use v1 only where v2 lacks coverage.
- **Free-user download mechanics:** confirm the exact website-initiated keyed `nxm://` flow with a real non-Premium account before building the download UX (dev is likely Premium).
- **Proton/Wine deployment correctness per game:** symlink-following, archive invalidation, and casefold behavior need empirical validation on real Skyrim SE/FO4 under Proton — add automated post-deploy "known test mod loads" assertions (Phase 5).
- **NexusMods.App archived 2026-02-20:** excellent *design* reference (Loadout, event-sourcing, FS abstraction) but no longer a moving target and was not Bethesda-first — treat as a frozen pattern source, not a live competitor.
- **App registration under Nexus Acceptable Use Policy:** start early; required to avoid throttling/blocking of an unregistered third-party app.

## Sources

### Primary (HIGH confidence)
- crates.io registry — current max-stable crate versions (tauri 2.11.3, reqwest 0.13.4, tokio 1.52.3, rusqlite 0.40.1, steamlocate 2.1.0, reflink-copy 0.1.30, sevenz-rust2 0.21.0, tauri-plugin-deep-link 2.4.9, etc.)
- NexusMods.App Disk State Storage ADR 0016 + MnemonicDB docs — three-way sync / event-sourcing design
- Tauri v2 State Management + Security docs — IPC, state, Stronghold deprecation

### Secondary (MEDIUM confidence)
- Vortex Mod Deployment / Install Manager / Nexus API (DeepWiki, derived from source) — manifest-based deploy, premium-vs-free, nxm://
- modding.wiki / Nexus Collections + FOMOD docs; LOOT/libloot sorting docs; UESP Bethesda plugin format (ESL, 254 limit)
- nxm:// Linux handler precedent (nxm-handler, modorganizer2-linux-installer)
- Phoronix/kernel — ext4 + tmpfs casefold for Wine; btrfs cross-subvolume EXDEV threads
- Rust `zip` CVE-2025-29787, async-tar TARmageddon, async_zip no-protection stance — extraction safety

### Tertiary (LOW confidence)
- WineHQ forum — Wine does not abstract filesystem / case-mismatch crashes (needs empirical validation under Proton)
- Steam Community — verify-integrity ignores non-Steam files / can overwrite modded base files

---
*Research completed: 2026-06-20*
*Ready for roadmap: yes*
