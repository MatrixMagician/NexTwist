# Pitfalls Research

**Domain:** Linux mod manager (Rust + Tauri) deploying mods into Windows games run via Steam Proton/Wine
**Researched:** 2026-06-20
**Confidence:** MEDIUM (most findings cross-checked across official Nexus wiki/docs, GitHub issues, and Linux kernel docs; web-search-only items flagged LOW)

> Phase names below are *suggested topics* for the roadmap, not yet-existing phases. They map cleanly to the Active requirements in PROJECT.md: Game Detection, Deployment Engine, Auth/Download, Collections, Conflicts/Load Order, Profiles, Packaging.

---

## Critical Pitfalls

These cause data loss, "game won't launch", or force an architecture rewrite. They directly threaten the core value (non-destructive, reversible, conflict-aware).

### Pitfall 1: Hardlink deployment fails across filesystem / btrfs subvolume / Proton "drive" boundaries

**What goes wrong:**
The mod staging area and the game folder end up on different filesystems (separate partition, separate btrfs subvolume, separate physical drive, or a Steam library on a different mount). `link()` returns `EXDEV` ("Invalid cross-device link") and deployment fails or silently falls back to a worse method. Crucially, **btrfs treats every subvolume as a separate device for `link()`** — even on the *same* physical disk — so a user with a `@home` subvolume staging folder and a game on `@` cannot hardlink. Proton compounds this: Wine registers the prefix as a different "drive", and tools like Vortex refuse to deploy across drive letters.

**Why it happens:**
Developers test on a single ext4 partition where hardlinks "just work", then ship. The btrfs-subvolume case is invisible in dev and extremely common on Fedora/openSUSE/SteamOS-adjacent setups. The Steam-library-on-second-drive case is the modal Linux gamer setup.

**How to avoid:**
- Detect the filesystem and `st_dev` of both the game dir and the chosen staging dir at setup time; **force the staging folder onto the same filesystem AND same subvolume as the game** (e.g. create staging *inside* the Steam library dir for that game, not in `~/.local/share`).
- Probe deployment capability empirically: attempt a real `link()` of a temp file from staging into the game tree, catch `EXDEV`, and surface the result before the user ever installs a mod.
- Implement a deployment-method abstraction (hardlink / reflink / symlink / copy) and pick at runtime per game based on the probe, not a global setting.

**Warning signs:**
`EXDEV` in logs; "deployment failed, only symlink available"; users on btrfs/Fedora/SteamOS reporting failures dev cannot reproduce on ext4.

**Phase to address:** Deployment Engine (probe + method selection); Game Detection (capture `st_dev`/fs type per game).

---

### Pitfall 2: Symlinks that Wine/Proton or the game engine won't follow

**What goes wrong:**
Symlink deployment is the natural Linux fallback when hardlinks fail, but many Windows games/loaders resolve paths in ways that break on symlinks: BSA/archive loaders, some anti-tamper, and Wine's path translation can mis-handle symlinked directories. The game launches but mods "don't load", or Wine reports the file missing. Worse, symlinking a *directory* into the game tree means a Steam update writing into that path can write through the link into your staging area.

**Why it happens:**
Symlink looks like a clean, instantly-reversible solution and works in a quick smoke test. Engine-specific resolution failures only show up with real mods on real games.

**How to avoid:**
- Prefer **hardlinks (files, not dirs)** or **reflinks (CoW, btrfs/xfs)** over symlinks for Creation Engine games; reserve symlinks for last resort.
- Never symlink whole directories into the game tree — deploy per-file so the game and Steam see real files.
- Validate, per supported game, that the chosen method actually loads a known test mod under Proton (an automated post-deploy assertion).

**Warning signs:**
Mods install "successfully" but have no in-game effect; works on native Linux games but not under Proton; directory symlinks appearing in the game folder.

**Phase to address:** Deployment Engine (per-game method validation).

---

### Pitfall 3: Overwrite collisions silently destroy original (vanilla) game files

**What goes wrong:**
A mod (or hardlink/copy deploy) writes a file whose path matches a real game asset. If you overwrite a vanilla file *in place* without first backing it up, purge has nothing to restore to — the game is permanently corrupted and only a Steam re-verify/redownload fixes it. With hardlinks this is especially dangerous: deleting/replacing the original can also affect the staged copy depending on order.

**Why it happens:**
Most game files are mod-added (new paths), so the in-place-overwrite bug doesn't surface until a mod *replaces* a base asset (very common for textures, meshes, `.ini`, base ESMs). Devs assume "mods only add files."

**How to avoid:**
- Before deploying any file that **already exists in the game tree and was not deployed by NexTwist**, move the original into a per-game backup/original-store and record it in the manifest. This is the heart of the non-destructive guarantee.
- Maintain a manifest that distinguishes: vanilla files, NexTwist-deployed files, and user-added files. Purge restores backed-up originals and only deletes files NexTwist created.
- Treat the manifest as the source of truth and write it transactionally (see Pitfall 4).

**Warning signs:**
No "originals" backup store exists; purge logic only deletes and never restores; deployment opens game files for write without a prior backup step.

**Phase to address:** Deployment Engine (backup-before-overwrite + manifest). This is the single most important safety mechanism in the whole product.

---

### Pitfall 4: Incomplete / non-atomic manifest leaves orphan files; purge does not return vanilla state

**What goes wrong:**
The manifest of "what we deployed" gets out of sync with the actual filesystem — because the app crashed mid-deploy, the user closed it, or a write was partial. On purge, files NexTwist created but didn't record are left behind (orphans), or recorded files were already removed and purge errors out. Net result: "purge" doesn't actually return the game to pristine — exactly the failure the product exists to prevent. Vortex itself ships a "Repair" function precisely because this happens in practice.

**Why it happens:**
Deploy/purge are treated as best-effort loops instead of transactions. State is written after the fact rather than as a journal. There's no reconciliation between recorded state and on-disk reality.

**How to avoid:**
- Use a **write-ahead journal**: record intended operations before performing them; on next launch, detect an incomplete journal and roll forward/back to a consistent state.
- Store a content hash + provenance for every deployed file so reconciliation can verify "is this file still the one we deployed, or did Steam/another tool change it?"
- Provide a `verify`/`repair` command that diffs manifest vs. disk and reports orphans and missing files; run it automatically after any abnormal exit.
- Make purge idempotent: missing files are a no-op, not an error.

**Warning signs:**
Files left in the game folder after a full purge; manifest entries pointing at non-existent files; no journal/transaction log; deploy and manifest-write are separate non-atomic steps.

**Phase to address:** Deployment Engine (journaling + reconcile + verify/repair). Bake verify/repair in from the start; retrofitting is painful.

---

### Pitfall 5: Case-sensitivity mismatch — mod ships `Textures/`, game opens `textures/`, file "not found"

**What goes wrong:**
Wine/Proton does **not** abstract the filesystem: a Windows `fopen("Data\\Textures\\x.dds")` maps straight to a Linux `open()`. On case-sensitive ext4/btrfs, if the mod author packaged `TEXTURES/` or `Mesh.NIF` but the game requests `textures/` / `mesh.nif`, the lookup fails — mods don't load, or the game crashes. Creation Engine mods are notoriously inconsistent about casing because they were authored on case-insensitive Windows/NTFS.

**Why it happens:**
On Windows (NTFS, case-insensitive) this never matters, so mod archives are full of mixed-case paths. Devs on a single test mod that happens to be correctly-cased never see it.

**How to avoid:**
- Deploy into a directory tree marked **case-insensitive at the filesystem level**: ext4 `casefold` (`mkfs.ext4 -O casefold` + `chattr +F dir`, dir must be empty when set) or tmpfs case-folding (Linux 6.13+). This is what Wine/Proton tooling recommends and is faster than runtime fixups.
- Detect at setup whether the game's deploy target supports/has casefold; if not, either (a) require/offer to create a casefolded staging dir, or (b) normalize casing on install by mapping mod paths to the game's canonical casing.
- Per-game, know the canonical casing of base directories (`Data`, `Textures`, `Meshes`, `Scripts`, ...) and rewrite incoming mod paths to match.

**Warning signs:**
Mods with no in-game effect despite "successful" install; works for some mods (correctly cased) not others; `chattr +F` failing because the dir is non-empty.

**Phase to address:** Game Detection / environment setup (detect casefold capability) + Deployment Engine (path-casing normalization).

---

### Pitfall 6: Wrong Proton prefix — writing `plugins.txt`/load order to the wrong place

**What goes wrong:**
Bethesda games read the active plugin list from `%LOCALAPPDATA%\<Game>\plugins.txt` (and historically `loadorder.txt`). Under Proton this maps to `~/.local/share/Steam/steamapps/compatdata/<APPID>/pfx/drive_c/users/steamuser/AppData/Local/<Game>/plugins.txt`. If NexTwist writes to the system `~/.config`/native HOME, a wrong `steamuser` path, or guesses the wrong appid/prefix, load order silently never applies. Each game has its own `compatdata/<APPID>` prefix, and Steam can recreate prefixes on update.

**Why it happens:**
Devs reuse Windows path logic, or assume one prefix. The `compatdata/<APPID>/pfx/.../steamuser/...` path is non-obvious and varies (`steamuser` vs username, Flatpak Steam relocates everything under `~/.var/app/com.valvesoftware.Steam`).

**How to avoid:**
- Resolve the prefix from Steam's own data: parse `libraryfolders.vdf` + the game's `appmanifest_<APPID>.acf` to get install dir and appid, then derive `compatdata/<APPID>/pfx`.
- Handle Flatpak and Snap Steam path roots, and `STEAM_COMPAT_DATA_PATH` if present.
- Locate `plugins.txt` by globbing the prefix's `AppData/Local/<Game>` rather than hardcoding the user folder name; verify the file exists/has the right header before writing.
- Re-resolve the prefix on each session (Steam may rebuild it).

**Warning signs:**
Load order edits have no effect in-game; `plugins.txt` written but game ignores it; Flatpak Steam users can't get any mods active; hardcoded `steamuser` or `~/.local/share/Steam` assumptions in code.

**Phase to address:** Game Detection (Steam/Proton prefix resolution) + Load Order management.

---

### Pitfall 7: Archive extraction path traversal (zip-slip) and malicious symlink entries

**What goes wrong:**
A downloaded mod archive contains entries like `../../../../home/user/.bashrc` or absolute paths, or (per Rust `zip` **CVE-2025-29787**) a symlink entry that later entries write *through*, escaping the target dir. Result: arbitrary file write outside the mod folder — a real RCE/data-loss vector since mods are third-party. Several Rust crates do **not** protect against this by default: `async_zip` explicitly refuses to, and `async-tar` had the "TARmageddon" traversal bug.

**Why it happens:**
Devs assume the extraction crate sanitizes paths. It often doesn't. Symlink-in-archive bypasses naive `..` checks.

**How to avoid:**
- For every entry: reject absolute paths and any path containing `..`; **canonicalize the resolved destination and assert it is still under the extraction root** after joining.
- Refuse to create symlink entries during extraction (or resolve+validate their targets); do not follow an extracted symlink when writing subsequent entries.
- Pin and audit the extraction crate version (avoid vulnerable `zip` 1.3.0–2.2.x ranges); add a unit test with a crafted zip-slip archive.
- Extract to a temp dir, validate, then move into staging — never extract directly into the game tree.

**Warning signs:**
Extraction code that joins entry paths without re-canonicalizing; no zip-slip test fixture; following symlinks during unpack.

**Phase to address:** Auth/Download or Deployment Engine — whichever owns extraction. Add the malicious-archive test before shipping any download feature.

---

## Moderate Pitfalls

### Pitfall 8: RAR licensing makes bundling `unrar` in an AppImage legally unsafe

**What goes wrong:** Many older Nexus mods ship as `.rar`. The reference `unrar`/`libunrar` uses a **non-free license** that prohibits using it to create a competing RAR-compatible archiver and restricts redistribution — risky to bundle in a distributed AppImage. Shipping it can violate the license and block distro packaging.

**How to avoid:** Use a permissively-licensed extractor — `libarchive` (handles zip/7z/tar and RAR read), `7z`/`p7zip`, `unar`/`The Unarchiver`, or a Rust crate (`sevenz-rust`, `compress-tools` which wraps libarchive). Verify the chosen path can read RAR5. Audit all bundled binaries' licenses for AppImage redistribution.

**Phase to address:** Auth/Download (extraction stack choice) + Packaging (license audit).

---

### Pitfall 9: NexusMods API limits & "downloads require the website/Premium" break the free-user flow

**What goes wrong:** API download *links* are gated to Premium accounts; **free users must initiate downloads from nexusmods.com** (the site hands off via `nxm://`). Building the UX assuming the app can fetch any download URL directly will work for the dev (likely Premium) and fail for most users. Rate limits (historically ~2500/day + 100/hr per personal key; reset 00:00 GMT) and the **API Acceptable Use Policy / app-approval requirement** can throttle or block an unregistered third-party app.

**How to avoid:** Design the download flow around `nxm://` handoff from the website for free users (register the handler, see Pitfall 10), and direct API downloads only for Premium. Honor `X-RL-*` rate-limit headers with backoff; cache mod metadata to minimize calls. Register the app with Nexus under the API Acceptable Use Policy early; adopt OAuth2 login (the path NexusMods.App uses). Show users their remaining quota.

**Phase to address:** Auth/Download.

---

### Pitfall 10: `nxm://` handler registration on Linux is fragile (AppImage, Flatpak, multiple handlers)

**What goes wrong:** Without a registered `nxm://` handler, free-user downloads from the website have nowhere to go. AppImages have no fixed install path, so the `.desktop` `Exec=` can point at a moved/deleted binary; multiple managers fight over the default handler; Flatpak sandboxing complicates registration.

**How to avoid:** Generate the `.desktop` with `MimeType=x-scheme-handler/nxm` on first run; register via `xdg-mime`/`xdg-settings`; for AppImages, write an absolute path that's stable (or use a launcher in `~/.local/bin`). Detect and warn if another app currently owns `x-scheme-handler/nxm`. Provide a one-click "set as default handler" and a self-test (`xdg-open "nxm://test"`).

**Phase to address:** Auth/Download + Packaging (AppImage path stability).

---

### Pitfall 11: Collections — missing/archived mods, version drift, and FOMOD automation

**What goes wrong:** Collections pin exact mod *versions*; mods get archived/deleted/updated, so a collection that worked yesterday fails today. FOMOD scripted installers require replaying the curator's chosen options; if NexTwist can't automate FOMOD choices, collection install stalls or installs wrong files. NexusMods.App's own issue tracker shows FOMOD-with-predefined-choices bugs were a recurring source of broken collection installs.

**How to avoid:** Pull the curator-pinned version; if archived/deleted, fall back to newest with an explicit warning, never silently. Implement the FOMOD XML installer (`ModuleConfig.xml`) including conditional flags and predefined-choice replay; persist chosen options per mod (the "magic wand" preset model). Validate the full collection in a dry run (resolve all downloads + options) before touching the filesystem; report unresolvable mods up front instead of failing midway.

**Phase to address:** Collections (depends on a working FOMOD installer + download + deployment).

---

### Pitfall 12: Archive invalidation / loose-file precedence not handled for Bethesda games

**What goes wrong:** Creation Engine games load packed BSA/BA2 archives and loose files with specific precedence rules. Without correct archive-invalidation handling, loose mod files (textures/meshes) get ignored in favor of base BSAs, so mods appear installed but have no effect. The relevant `.ini` settings live in the (Proton-prefix) `Documents/My Games/<Game>/*.ini`.

**How to avoid:** Per supported Bethesda game, apply the correct archive-invalidation method (modern SE/FO4: set `[Archive] bInvalidateOlderFiles=1`, `sResourceDataDirsFinal=` in the prefix's `Skyrim.ini`/`Fallout4.ini`; older titles use empty-BSA tricks). Locate those `.ini`s inside the Proton prefix `My Games` folder (same prefix-resolution problem as Pitfall 6). Document casing for the `Data` tree (ties into Pitfall 5).

**Phase to address:** Load Order / game-specific support (Bethesda first per PROJECT.md).

---

### Pitfall 13: Steam game update / re-verify silently breaks deployment

**What goes wrong:** A Steam update or "Verify integrity of game files" deletes/replaces files Steam owns — but **ignores files it never installed** (most mods) and can overwrite a vanilla file you'd hardlinked/backed-up, breaking deployment and leaving the manifest stale. Worse, an update can re-create the Proton prefix, invalidating prefix paths.

**How to avoid:** Detect game-version changes (hash `appmanifest_<APPID>.acf` build id / key files) on app launch; if changed, mark deployment stale and prompt re-deploy + re-resolve prefix. Never assume the on-disk game matches the last-deployed state — reconcile (Pitfall 4). Document for users that they should purge before verifying integrity.

**Phase to address:** Game Detection (version/build tracking) + Deployment Engine (staleness reconcile).

---

## Minor Pitfalls

### Pitfall 14: Tauri IPC blocking on long downloads / large-file handling

**What goes wrong:** Running a multi-GB collection download as a blocking `#[tauri::command]` freezes the IPC bridge and UI; passing large file bodies through the JS↔Rust IPC boundary is slow and memory-heavy.

**How to avoid:** Stream downloads in async Rust tasks; report progress via Tauri **events** (`emit`), not IPC return values. Never read whole archives into memory across IPC — keep file bytes in Rust, send only progress/paths to the webview. Use the OS-native HTTP stack/streaming and write to disk directly.

**Phase to address:** Auth/Download (download manager architecture).

---

### Pitfall 15: Insecure OAuth token / credential storage

**What goes wrong:** Storing the Nexus OAuth token in plaintext (config file, localStorage) exposes the user's account. Stronghold (the obvious Tauri choice) is **deprecated and removed in Tauri v3**.

**How to avoid:** Use the OS keyring (Secret Service / KWallet via a keyring crate or Tauri keyring plugin) for the refresh/access token; fall back to an encrypted vault with the key in the keyring only where Secret Service is unavailable (headless). Do not adopt Stronghold for new work. Scope tokens minimally; handle refresh.

**Phase to address:** Auth/Download (credential storage), reviewed in any security gate.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Copy-deploy instead of hardlink/reflink | Always works, no EXDEV/fs concerns | 2x disk for every game; slow deploy; CoW benefits lost | MVP fallback only, behind a probe; never the sole strategy |
| Overwrite vanilla files in place (no backup store) | Simpler deploy code | Breaks the non-destructive core value; unrecoverable corruption | **Never** |
| Manifest written after deploy, no journal | Faster to build | Orphans + un-restorable purge after any crash | **Never** for the safety path |
| Hardcode `steamuser` / single Steam root | Works on dev machine | Fails for Flatpak/Snap Steam, custom usernames, 2nd-drive libraries | Spike only; replace before any release |
| Use Rust `zip` crate default extract, no path checks | Fast to wire up | Zip-slip RCE on third-party archives | **Never** |
| Bundle `unrar` for RAR support | One-line RAR support | Non-free license; blocks distro packaging | Never in distributed builds |
| Assume API can fetch any download URL | Simple download UX | Broken for all free users (Premium-gated links) | Never — design for nxm:// handoff from day one |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| NexusMods API | Direct download links for everyone; ignoring rate-limit headers; unregistered app | Premium → API links, free → nxm:// website handoff; honor `X-RL-*`; register under Acceptable Use Policy; OAuth2 |
| Steam library | Single root, single drive, NTFS-style assumptions | Parse `libraryfolders.vdf` + `appmanifest_*.acf`; support multi-drive, Flatpak/Snap roots |
| Proton prefix | Writing load order to native HOME / wrong prefix | Derive `compatdata/<APPID>/pfx/.../steamuser/AppData/Local/<Game>`; re-resolve each session |
| Wine filesystem | Assume case-insensitive like Windows | Casefold the deploy tree or normalize mod path casing |
| FOMOD installers | Treat as plain archives | Implement `ModuleConfig.xml` installer with flags + predefined-choice replay |
| Archive libs | Trust crate to prevent traversal | Canonicalize + bounds-check every entry; reject symlink/absolute/`..`; pin safe versions |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Re-hashing/relinking entire load order on every deploy | Long stalls deploying large lists | Incremental deploy: diff manifest, only touch changed files | Large Skyrim setups (hundreds of mods, 10k+ files) |
| Wine's runtime case-insensitive lookups instead of fs casefold | Slow file access in-game | Use ext4/tmpfs casefold at deploy target | Texture-heavy mods, many files |
| Large file bytes over Tauri IPC | High RAM, UI jank | Stream in Rust; events for progress only | Multi-GB collections |
| Synchronous metadata calls per mod during collection resolve | Slow installs; hits rate limit | Batch + cache metadata; respect rate limits | Collections with 100s of mods |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Zip-slip / symlink-in-archive extraction | Arbitrary file write / RCE from untrusted mods | Canonicalize+bounds-check entries; reject symlinks/abs/`..`; pin non-vulnerable crates; test fixture |
| Plaintext OAuth token | Account takeover | OS keyring; encrypted vault fallback; no Stronghold (deprecated) |
| Extracting directly into game tree | Malicious archive corrupts game | Extract to temp, validate, then move |
| Trusting collection/mod file hashes only from API | Tampered download installed | Verify downloaded file hash against API-provided hash before install |
| nxm:// handler hijack / pointing Exec at arbitrary path | Malicious handler interception | Validate handler ownership; stable absolute Exec path |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| "Deploy succeeded" with no in-game effect (case/symlink/archive-invalidation) | User can't tell mods are broken | Post-deploy validation + load a known test asset; surface red/green status per game |
| Failing a collection install midway | User left with a half-modded game | Dry-run resolve all mods/options first; report blockers before touching disk |
| Silent fallback to copy/symlink | Confusing disk usage or broken mods | Tell the user which method is active and why; explain EXDEV/casefold remediation |
| No "remaining API quota" feedback | Sudden rate-limit walls mid-session | Show quota; queue + backoff |
| Purge that doesn't truly restore vanilla | Erodes the trust that is the product's reason to exist | Verify/repair + show "game is pristine" confirmation with diff vs. manifest |

## "Looks Done But Isn't" Checklist

- [ ] **Deployment:** Tested on ext4 *and* btrfs-subvolume *and* second-drive Steam library — verify EXDEV handling, not just the dev's single ext4 partition.
- [ ] **Purge:** After install→purge, the game folder byte-for-byte matches vanilla (no orphans, originals restored) — verify with a hash diff, not "looks empty".
- [ ] **Overwrite safety:** A mod that *replaces* a base game file is backed up and restored on purge — verify the original returns.
- [ ] **Crash recovery:** Kill the app mid-deploy, relaunch — verify journal reconcile leaves a consistent state.
- [ ] **Case sensitivity:** A mod packaged with mismatched casing actually loads in-game under Proton — verify, don't assume.
- [ ] **Prefix:** Load order applies for default Steam, Flatpak Steam, and a custom username — verify in-game plugin list.
- [ ] **Free-user download:** Tested with a non-Premium account via nxm:// handoff — not just the dev's Premium account.
- [ ] **Archive safety:** A crafted zip-slip / symlink archive is rejected — verify with a test fixture.
- [ ] **Collection:** A collection containing an archived/deleted mod and a FOMOD-with-choices installs or fails gracefully with a clear report.
- [ ] **Archive invalidation:** Loose texture/mesh mods visibly override base BSAs in-game.
- [ ] **Steam update:** After a simulated game update/verify, app detects staleness and offers re-deploy.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Orphan files after purge | LOW–MEDIUM | verify/repair command diffs manifest vs disk, deletes recorded orphans, restores backed-up originals |
| Vanilla file overwritten with no backup | HIGH | Only fixable by Steam "Verify integrity" re-download; prevention (backup store) is the real fix |
| Wrong prefix / load order never applied | LOW | Re-resolve prefix from acf/vdf, re-write plugins.txt in correct path |
| EXDEV deployment failure | LOW | Probe + relocate staging onto same fs/subvolume, or fall back to reflink/copy |
| Case-mismatch broken mod | LOW–MEDIUM | Casefold the deploy dir (must be empty) or normalize casing and redeploy |
| Half-installed collection | MEDIUM | Dry-run resolver state lets you resume; otherwise purge collection and reinstall |
| Leaked plaintext token | MEDIUM | Revoke token on Nexus, re-auth via OAuth, migrate to keyring |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| 1 Hardlink/EXDEV/btrfs | Deployment Engine + Game Detection | Deploy succeeds on ext4, btrfs subvol, 2nd-drive library |
| 2 Symlink not followed | Deployment Engine | Known test mod loads in-game under Proton |
| 3 Overwrite destroys vanilla | Deployment Engine (backup store) | Replaced base file restored on purge |
| 4 Manifest/orphans/purge | Deployment Engine (journal + verify/repair) | Hash-diff vanilla after install→purge; crash-mid-deploy recovers |
| 5 Case sensitivity | Game Detection + Deployment Engine | Mixed-case mod loads in-game |
| 6 Wrong Proton prefix | Game Detection + Load Order | Load order applies on default/Flatpak/custom-user Steam |
| 7 Zip-slip | Auth/Download (extraction) | Crafted malicious archive rejected (test fixture) |
| 8 RAR license | Auth/Download + Packaging | License audit of bundled extractors; RAR5 reads |
| 9 API limits/Premium gating | Auth/Download | Free account downloads via nxm:// handoff |
| 10 nxm:// handler | Auth/Download + Packaging | `xdg-open nxm://test` reaches the app from AppImage |
| 11 Collections/FOMOD | Collections | Collection with archived mod + FOMOD choices resolves or fails cleanly |
| 12 Archive invalidation | Load Order / Bethesda support | Loose files override base BSAs in-game |
| 13 Steam update breakage | Game Detection + Deployment Engine | App flags staleness after build-id change |
| 14 Tauri IPC/large files | Auth/Download | Multi-GB download streams with progress, no UI freeze |
| 15 Token storage | Auth/Download + security gate | Token in keyring, not plaintext; no Stronghold |

## Sources

- Nexus Mods Wiki — Deployment Methods (hardlink/symlink mechanics, purge): https://wiki.nexusmods.com/index.php/Deployment_Methods (MEDIUM)
- Vortex Wiki / DeepWiki — Mod Deployment & manifest-based safety, Repair for orphans: https://deepwiki.com/Nexus-Mods/Vortex/3.2-mod-deployment ; https://github.com/Nexus-Mods/Vortex/wiki/MODDINGWIKI-Users-FAQ (MEDIUM)
- Vortex GitHub issues — symlink deploy failures, purge not purging: https://github.com/Nexus-Mods/Vortex/issues/9266 ; https://github.com/Nexus-Mods/Vortex/issues/6234 ; https://github.com/Nexus-Mods/Vortex/issues/5497 (MEDIUM)
- Phoronix / kernel — Wine + ext4 casefold, tmpfs case-folding (Linux 6.13): https://www.phoronix.com/news/Linux-6.13-Tmpfs-Case-Folding (MEDIUM)
- WineHQ forum — Wine does not abstract filesystem, case mismatch crashes: https://forum.winehq.org/viewtopic.php?t=2959 (LOW)
- btrfs cross-subvolume hardlink EXDEV: https://itsfoss.gitlab.io/blog/getting-invalid-cross-device-link-doing-a-cp--l---same-volume/ ; kernel patch threads (MEDIUM)
- Skyrim plugins.txt / loadorder.txt location, MO2 profile paths: Step Mods, LOOT docs, MO2 issue #644 (MEDIUM)
- NexusMods API rate limits / Premium-gated downloads / Acceptable Use Policy: https://help.nexusmods.com/article/105 ; https://help.nexusmods.com/article/114 (MEDIUM)
- NexusMods.App downloads FAQ + nxm:// xdg-mime handler on Linux; OAuth2 issue #19: https://nexus-mods.github.io/NexusMods.App/users/faq/NexusModsDownloads/ ; https://github.com/Nexus-Mods/NexusMods.App/issues/19 (MEDIUM)
- NexusMods.App FOMOD docs + collection FOMOD-choices bug; Collections version pinning/archived fallback: https://nexus-mods.github.io/NexusMods.App/developers/misc/AboutFomod/ ; https://modding.wiki/en/nexusmods/collections/create/mod-options (MEDIUM)
- Zip-slip: Rust `zip` CVE-2025-29787, async-tar TARmageddon, async_zip no-protection stance, Snyk zip-slip: https://www.sentinelone.com/vulnerability-database/cve-2025-29787/ ; https://github.com/snyk/zip-slip-vulnerability ; https://linuxsecurity.com/news/security-vulnerabilities/linux-tar-async-tar-vulnerability-tarmageddon (MEDIUM)
- Tauri 2 — Stronghold deprecation, keyring/OS-native secure storage, IPC: https://v2.tauri.app/plugin/stronghold/ ; https://github.com/orgs/tauri-apps/discussions/7846 ; https://v2.tauri.app/security/ (MEDIUM)
- Steam verify-integrity ignores non-Steam files, can overwrite modded base files: https://steamcommunity.com/sharedfiles/filedetails/?id=2834863313 (LOW)

---
*Pitfalls research for: Linux/Proton NexusMods mod manager (Rust + Tauri)*
*Researched: 2026-06-20*
