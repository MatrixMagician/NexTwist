# Architecture Research

**Domain:** Desktop mod manager (Rust + Tauri) deploying Windows-game mods into Steam Proton / Wine on Linux
**Researched:** 2026-06-20
**Confidence:** MEDIUM-HIGH (deployment + NexusMods.App model verified against primary sources; some Proton/case-folding details inferred from converging community sources)

## Standard Architecture

Mod managers (Vortex, Mod Organizer 2, NexusMods.App) all converge on the same fundamental split: **mods are never installed directly into the game; they live in a managed staging store, and a deployment engine projects them into the game folder in a way that can be exactly undone.** Everything else (UI, API client, profiles, load order) orbits that core.

The single most important architectural decision for this project is to make the **pure-Rust core fully independent of Tauri** so the deployment engine, sync logic, and API client are unit-testable headless. Tauri commands are a thin adapter layer only.

### System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    Tauri Frontend (Webview)                       │
│   Mod list · Conflicts UI · Load order · Profiles · Downloads     │
│   Login flow · Collection installer · Deploy/Purge buttons        │
└───────────────▲───────────────────────────────────┬──────────────┘
        events / │ emit (progress, state)            │ invoke()
        channels │                                    ▼
┌───────────────┴───────────────────────────────────────────────────┐
│            Tauri Command Layer (thin adapter)                      │
│   #[tauri::command] fns · State<Mutex<AppState>> · event emitters  │
├────────────────────────────────────────────────────────────────────┤
│                     Rust Core (pure, testable)                     │
│  ┌────────────┐ ┌──────────────┐ ┌───────────┐ ┌────────────────┐ │
│  │ NexusMods  │ │  Download    │ │  Archive   │ │ Game / Profile │ │
│  │ API Client │ │  Manager     │ │ Extractor  │ │   Registry     │ │
│  │ (OAuth/    │ │ (CDN, queue, │ │ (-> staging│ │ (Proton/Steam  │ │
│  │  GraphQL,  │ │  resume)     │ │  store)    │ │  discovery)    │ │
│  │  nxm://)   │ └──────┬───────┘ └─────┬──────┘ └───────┬────────┘ │
│  └─────┬──────┘        │               │                │          │
│        │               ▼               ▼                │          │
│        │        ┌──────────────────────────────┐        │          │
│        │        │      Mod Staging Store        │        │          │
│        │        │  (per-mod extracted trees)    │        │          │
│        │        └──────────────┬───────────────┘        │          │
│        │                       ▼                         │          │
│        │        ┌──────────────────────────────┐        │          │
│        │        │      DEPLOYMENT ENGINE        │◄───────┘          │
│        │        │  conflict resolver · linker   │                   │
│        │        │  three-way synchronizer       │                   │
│        │        │  deploy / purge / verify      │                   │
│        │        └──────────────┬───────────────┘                   │
│        │                       │ writes links + records             │
├────────┴───────────────────────┼───────────────────────────────────┤
│                  Persistence (SQLite + on-disk store)              │
│  ┌─────────────┐  ┌─────────────────┐  ┌────────────────────────┐ │
│  │  Database   │  │  Deploy Manifest │  │  Staging files on disk │ │
│  │ (games,     │  │ (every file      │  │  + downloads cache     │ │
│  │  profiles,  │  │  written, hash,  │  │                        │ │
│  │  mods, LO)  │  │  source, method) │  │                        │ │
│  └─────────────┘  └─────────────────┘  └────────────────────────┘ │
└────────────────────────────────────────────────────────────────────┘
        │                                              ▲
        ▼ resolve install dir / prefix                 │ launch via Steam
┌────────────────────────────────────────────────────────────────────┐
│              Target: Steam Proton / Wine game                      │
│  steamapps/common/<Game>/Data/ (plugins, meshes, textures)         │
│  steamapps/compatdata/<appid>/pfx/.../AppData/.../plugins.txt       │
└────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|------------------------|
| Tauri Frontend | All UI: mod list, conflict resolution, load-order drag/drop, profiles, download queue, login, collection install wizard | Web UI (React/Svelte/Solid) talking to backend only via `invoke` + event listeners |
| Tauri Command Layer | Thin sync/async boundary: marshals JSON ↔ Rust, holds `State<Mutex<AppState>>`, emits progress events | `#[tauri::command]` async fns; no business logic |
| NexusMods API Client | OAuth login, GraphQL (v2) + REST (v1) queries, `nxm://` URL parsing, resolve CDN download URLs, fetch Collection revisions, rate-limit handling | `reqwest` + `serde`; OAuth via system browser + loopback/`oauth2` crate |
| Download Manager | Queue, resumable HTTP downloads from CDN mirrors, hash verification, write to downloads cache | `reqwest` streaming + `tokio`; progress via channels/events |
| Archive Extractor | Unpack `.zip`/`.7z`/`.rar` mod archives into a clean per-mod staging tree; apply FOMOD/installer scripts | `sevenz-rust`/`zip`/`unrar`; FOMOD XML parser |
| Mod Staging Store | Canonical, immutable-per-mod extracted file trees; source of truth for deployment; survives enable/disable | Content-addressed or per-mod directories under app data |
| Game / Profile Registry | Discover Steam libraries + Proton prefixes, identify supported games (Bethesda first), resolve install dir vs prefix paths, manage per-game profiles | Parse `libraryfolders.vdf` + `appmanifest_*.acf`; path resolver |
| **Deployment Engine** | Resolve conflicts by load order, link staging → game dir, record a manifest of every file written, deploy/purge/verify, three-way sync | Hardlink/symlink/copy strategies behind a trait; manifest in DB |
| Database | Persist games, profiles, mods, files, conflicts, load order, collection revisions | SQLite (`sqlx`/`rusqlite`) — relational fits this normalized model |
| Deploy Manifest | Exact record of deployed state for reversible purge | Table(s) in SQLite, or per-deploy JSON keyed by profile |

## Recommended Project Structure

```
nextwist/
├── src-tauri/                      # Tauri shell + command adapter ONLY
│   ├── src/
│   │   ├── main.rs                 # builder, manage(AppState), nxm:// handler reg
│   │   ├── commands/               # #[tauri::command] thin wrappers
│   │   │   ├── auth.rs             # login/logout
│   │   │   ├── mods.rs             # list/install/enable/disable
│   │   │   ├── deploy.rs           # deploy/purge/verify
│   │   │   ├── profiles.rs
│   │   │   └── downloads.rs
│   │   └── state.rs                # AppState (holds core services)
│   └── tauri.conf.json             # deep-link plugin for nxm://, AppImage cfg
├── crates/
│   ├── core/                       # pure domain types, no I/O frameworks
│   │   ├── model.rs                # Game, Profile, Mod, FileEntry, Conflict, LoadOrder
│   │   └── error.rs
│   ├── deploy/                     # DEPLOYMENT ENGINE (the crown jewel)
│   │   ├── method/                 # trait DeploymentMethod
│   │   │   ├── hardlink.rs
│   │   │   ├── symlink.rs
│   │   │   └── copy.rs
│   │   ├── manifest.rs             # record/load deployed-file manifest
│   │   ├── conflict.rs             # winner-by-load-order resolution
│   │   ├── sync.rs                 # three-way sync (orig/applied/current)
│   │   └── casefold.rs             # Proton case-mapping
│   ├── nexus/                      # API client: oauth, graphql, rest, nxm parse
│   ├── download/                   # resumable download manager
│   ├── extract/                    # archive + FOMOD extraction -> staging
│   ├── steam/                      # Proton/Steam discovery + path resolution
│   └── store/                      # SQLite persistence + staging store
└── frontend/                       # web UI (framework of choice)
```

### Structure Rationale

- **`src-tauri/` holds zero business logic.** Every command is a 3–10 line wrapper that calls into a `crates/` service and emits events. This keeps the safety-critical deployment logic testable without spinning up a webview, and lets you fuzz/property-test the engine on temp dirs in CI.
- **`crates/deploy/` is isolated and dependency-light.** It only knows about staging trees, target dirs, and a manifest — not about NexusMods or Tauri. The reversibility guarantee (the product's Core Value) lives here and must be the most heavily tested crate.
- **`crates/steam/` quarantines all Proton/Steam-layout knowledge** so the deployment engine receives already-resolved absolute paths (install dir + prefix AppData) and doesn't itself reason about `compatdata`.

## Architectural Patterns

### Pattern 1: Staging Store + Manifest-Driven Deploy/Purge (NON-NEGOTIABLE)

**What:** Mods are extracted once into a per-mod staging tree. Deployment *links* (never moves the only copy of) staging files into the game folder, and records **every** path it creates in a manifest. Purge reads the manifest and deletes exactly those paths — nothing discovered by scanning. This is the Vortex model and directly satisfies "non-destructive + fully reversible."

**When to use:** Always. This is the foundation of the entire product.

**Trade-offs:** Costs extra bookkeeping and disk (staging is a second copy), but hardlinks make the *deployed* copy free, and the manifest is what makes "return game to pristine" provable rather than hopeful.

**Example:**
```rust
// Conceptual: manifest is the source of truth for what to undo
struct DeployedFile {
    target: RelPath,        // path inside game/prefix
    source_mod: ModId,      // which staged mod won
    method: DeployMethod,   // Hardlink | Symlink | Copy
    hash: u64,              // xxhash64 of source at deploy time
    pre_existing: bool,     // was there a vanilla file here? (back it up!)
}
// purge(): for each DeployedFile -> remove link; if pre_existing, restore backup
```

### Pattern 2: Three-Way Synchronizer (from NexusMods.App)

**What:** NexusMods.App tracks three disk states: **original** (vanilla, before any mods), **last-applied** (what we last deployed), and **current** (what's actually on disk now). The synchronizer diffs these to decide what to add/remove and — critically — to detect files the *user or game patcher* changed outside the manager, so it never blindly clobbers or orphans them. NexusMods.App implements this on its immutable temporal DB (MnemonicDB: `[Entity,Attribute,Value,Tx,Assert/Retract]` tuples; `conn.AsOf(txId)` recovers any past state). You don't need MnemonicDB — SQLite + recorded transaction snapshots reproduce the same three-state diff.

**When to use:** For deploy and for "verify"/"detect external changes" before re-deploying.

**Trade-offs:** More complex than naive purge-all-then-redeploy, but it's what prevents data loss when Proton/the game writes into `Data/` or when a user hand-edits a file. Phase this in: ship simple deploy/purge first, add full three-way sync second.

### Pattern 3: Pluggable Deployment Method Trait

**What:** A `DeploymentMethod` trait with `deploy_file`, `remove_file`, `is_applicable(staging_fs, game_fs)`. Implementations: Hardlink (preferred), Symlink (cross-filesystem), Copy (fallback). The engine picks the best applicable method per (staging, target) pair — exactly Vortex's `IDeploymentMethod` design.

**When to use:** Always — Linux/Proton makes the choice situational (see Integration Points).

**Trade-offs:** A trait indirection, but it isolates the messy per-filesystem correctness logic and lets you add overlayfs later without touching callers.

## Data Flow

### Core Loop (login → purge)

```
[Login]  UI invoke(login) → nexus::oauth (system browser, loopback) → JWT (premium claim) → store

[nxm:// or Collection]
  OS hands nxm://gameId/modId/fileId?key&expires  → deep-link plugin → command
       → nexus::resolve_download_urls() → CDN mirror list
  (Collection: nexus graphql collectionRevision → list of {mod,file} → enqueue each)

[Download]  download::enqueue(url) → resumable fetch → downloads cache → verify hash
       → emit progress events to UI

[Extract]   extract::unpack(archive) (+FOMOD choices) → staging store per-mod tree
       → store::insert Mod + FileEntry rows

[Resolve conflicts]  deploy::conflict::resolve(profile.load_order)
       → for each target path, highest-priority mod wins → Conflict rows for UI

[Deploy]   deploy::sync(original, last_applied, current)
       → choose method per file → write links → back up pre-existing vanilla files
       → record DeployedFile manifest rows (atomic w.r.t. DB tx)

[Manage order]  UI reorders → load_order rows updated → re-run resolve + deploy
       (only the delta of changed winners is re-linked)

[Launch]   Steam launches game (manager does NOT replace launcher)

[Purge]    deploy::purge(manifest) → remove every recorded link
       → restore backed-up vanilla files → game folder pristine
```

### State Management (Tauri)

```
AppState { nexus: NexusClient, store: Db, jobs: JobRegistry, ... }
   managed as State<Mutex<AppState>>  (tokio::Mutex — guards held across await)

UI ──invoke(cmd)──► command fn ──► core service (returns Result)
UI ◄──emit("download://progress" | "deploy://progress" | "state://changed")── spawned tokio task
```
Long operations (download, extract, deploy) run as spawned `tokio` tasks that push progress via `app.emit` / Channels, so the UI stays responsive and commands return immediately with a job id.

### Key Data Flows

1. **Path resolution:** `steam` crate parses `libraryfolders.vdf` → finds each library's `steamapps/` → `appmanifest_<appid>.acf` gives `installdir` (→ `steamapps/common/<Game>` for `Data/`), and `steamapps/compatdata/<appid>/pfx/drive_c/users/steamuser/AppData/Local/<Game>/` for `plugins.txt`/load order. Engine receives both resolved absolute roots.
2. **Reversibility flow:** every write goes through the manifest; every read for purge comes *only* from the manifest — disk scanning is used to *detect drift*, never to decide what to delete.

## Scaling Considerations

Scale here is "size of a single user's load order," not number of users (desktop app).

| Scale | Architecture Adjustments |
|-------|--------------------------|
| Small load order (<50 mods) | Naive full purge+redeploy is fine; SQLite trivial |
| Large (500–2000 mods, 100k+ files — typical Skyrim) | Deploy only the *delta* of changed conflict winners; index DiskState by (location, relpath) for range queries; batch hardlink syscalls; hash with xxhash64 not SHA |
| Huge collections / frequent reorders | Cache conflict resolution; incremental sync via three-way diff so a load-order tweak relinks only affected files |

### Scaling Priorities

1. **First bottleneck:** redeploying everything on each change. Fix with delta deployment driven by the three-way synchronizer.
2. **Second bottleneck:** hashing/IO on huge mod sets. Fix with xxhash64, parallel extraction, and storing file hashes so re-verify is incremental.

## Anti-Patterns

### Anti-Pattern 1: Copying mods directly into the game folder
**What people do:** Extract mods straight into `steamapps/common/<Game>/Data`.
**Why it's wrong:** Destroys the vanilla state, makes uninstall guesswork, and corrupts the base install (violates the Core Value). Steam "verify integrity" will fight you.
**Do this instead:** Staging store + manifest-driven linking; the game folder only ever contains links + backed-up originals.

### Anti-Pattern 2: Purging by directory scan instead of by manifest
**What people do:** Delete everything in `Data/` that "looks like a mod."
**Why it's wrong:** Deletes user/game-created files and vanilla content; can't distinguish managed from unmanaged.
**Do this instead:** Purge only files recorded in the manifest; restore pre-existing backups; use scanning solely to *warn* about external drift.

### Anti-Pattern 3: Ignoring case-sensitivity until it breaks
**What people do:** Deploy mod files with their archive casing onto ext4.
**Why it's wrong:** Bethesda games/mods reference paths in mixed case; Linux ext4/btrfs are case-sensitive, so Wine lookups fail and assets silently don't load.
**Do this instead:** Detect/handle case at deploy time — prefer placing the game tree on an ext4 dir with the `casefold` (+F) attribute, or normalize casing and maintain a case map. Make this a first-class concern in `deploy/casefold.rs`.

### Anti-Pattern 4: Business logic inside Tauri commands
**What people do:** Put deployment/sync logic in `#[tauri::command]` functions.
**Why it's wrong:** Untestable without a webview; couples safety-critical code to the UI.
**Do this instead:** Commands are thin; logic lives in `crates/` and is unit/property-tested headless.

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| NexusMods API | OAuth 2.0 (system browser → JWT w/ premium claim) for v2 GraphQL; legacy v1 REST + personal API key. `getDownloadURLs` returns CDN mirrors. | Non-premium users can't auto-download — must click through manual confirm. Respect rate limits (handle RateLimitError). Register OS handler for `nxm://`. |
| `nxm://` protocol | Tauri deep-link plugin registers the scheme; URL carries `gameId/modId/fileId` (+ `key`/`expires` for premium). | One-click installs from the website depend on this. AppImage must install a `.desktop` MIME handler. |
| Collections (NexusMods) | GraphQL `collectionRevision` → ordered list of mod+file refs + metadata; install loop downloads/extracts/deploys each. | Revisions are versioned; store the revision id so a collection can be updated/reverted. |
| Steam / Proton | Filesystem discovery only (no Steam API needed for v1): `libraryfolders.vdf`, `appmanifest_*.acf`, `compatdata/<appid>/pfx`. | Launch is delegated to Steam; manager does not launch the game itself. |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| Frontend ↔ Command layer | `invoke` (req/resp) + `emit`/Channels (progress) | Only boundary the UI knows |
| Command layer ↔ Core crates | Direct Rust calls returning `Result` | Commands own no logic |
| Deploy engine ↔ Steam resolver | Engine receives resolved absolute roots (install dir + prefix AppData) | Engine stays Proton-agnostic |
| Deploy engine ↔ Store | Manifest read/write inside DB transactions | Atomicity = no half-deployed state recorded |

## Suggested Build Order (component dependencies)

Driven by dependency edges and by front-loading the Core Value (safe reversible deploy). Each step is independently demonstrable.

1. **`core` model + `store` (SQLite) + `steam` discovery.** Foundation: define Game/Profile/Mod/FileEntry/LoadOrder; detect Steam libraries and resolve a Bethesda game's install dir + prefix paths. *Demo: app lists detected games and their resolved paths.* (No deps.)
2. **Mod Staging Store + `extract`.** Manually point at an archive → extract into staging → rows in DB. *Demo: a mod appears in the list, its files enumerated.* (Deps: 1.)
3. **Deployment Engine — deploy + purge + manifest (single method first).** The crown jewel. Hardlink with copy fallback, full manifest, vanilla backup, exact purge. *Demo: deploy one mod, see links in `Data/`, purge, folder pristine.* (Deps: 1, 2.) **Build the most tests here.**
4. **Conflict resolution + load order.** Multiple mods, winner-by-priority, conflict UI data, reorder→redeploy. (Deps: 3.)
5. **Three-way synchronizer + symlink method + casefold handling.** Delta deploys, external-change detection, Proton case correctness. (Deps: 3, 4.)
6. **NexusMods API client + OAuth + download manager.** Login, `getDownloadURLs`, resumable downloads into cache → feed step 2's extractor. (Deps: 2.)
7. **`nxm://` handler.** One-click installs wired to steps 6→2→3. (Deps: 6.)
8. **Collections installer.** GraphQL revision → batch the 6→2→3→4 loop. (Deps: 6, 7, 4.)
9. **Profiles (multi-profile switching) + AppImage packaging.** Per-game profile switch re-runs resolve+deploy; package + register MIME handler. (Deps: 3–8.)

Steps 1–5 deliver the differentiating safety story end-to-end before any NexusMods networking exists — which de-risks the project, since the API surface is replaceable but the deployment correctness is the reason to exist.

## Sources

- [Vortex Mod Deployment — DeepWiki](https://deepwiki.com/Nexus-Mods/Vortex/3.2-mod-deployment) (MEDIUM-HIGH: derived from Vortex source)
- [Vortex Install Manager — DeepWiki](https://deepwiki.com/Nexus-Mods/Vortex/3.1-install-manager)
- [Vortex Nexus API — DeepWiki](https://deepwiki.com/Nexus-Mods/Vortex/6.1-nexus-api)
- [NexusMods.App — Disk State Storage ADR (0016)](https://nexus-mods.github.io/NexusMods.App/developers/decisions/backend/0016-disk-state-storage/) (HIGH: official ADR)
- [MnemonicDB docs](https://nexus-mods.github.io/NexusMods.MnemonicDB/) and [repo](https://github.com/Nexus-Mods/NexusMods.MnemonicDB) (HIGH: official)
- [Nexus Mods Deployment Methods wiki](https://wiki.nexusmods.com/index.php/Deployment_Methods) (MEDIUM)
- [Tauri v2 State Management](https://v2.tauri.app/develop/state-management/) (HIGH: official)
- [Locate Steam Play game files on Linux](https://linuxhint.com/locate_linux_steam_game_file/) and [Single Proton prefix guide](https://steamcommunity.com/sharedfiles/filedetails/?id=3378517770) (MEDIUM: community)
- [ext4 casefold / Wine case-insensitivity (kernel + archinstall discussion)](https://github.com/archlinux/archinstall/issues/380) (MEDIUM: converging community/kernel sources)
- [Nexus Mods GraphQL API](https://graphql.nexusmods.com/) and [node-nexus-api](https://github.com/Nexus-Mods/node-nexus-api) (MEDIUM-HIGH)

---
*Architecture research for: Rust + Tauri Proton/Wine mod manager (NexTwist)*
*Researched: 2026-06-20*
