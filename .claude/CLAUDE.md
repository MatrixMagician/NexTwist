<!-- GSD:project-start source:PROJECT.md -->

## Project

**NexTwist**

NexTwist is a Rust + Tauri desktop application that brings Vortex/Mod-Organizer-2-class mod management to Linux gamers. It lets users log into NexusMods, download and install individual mods and curated Collections, and manage them for Windows PC games that run on Linux via Steam Proton / Wine — with safe, fully reversible deployment. It is for Linux gamers (and modding power users migrating from Windows) who want a first-class mod manager that "just works" on their platform.

**Core Value:** Mods must install and uninstall **safely**: deployment is non-destructive (the base game install is never directly corrupted), fully reversible (any mod or the whole load order can be removed leaving the game pristine), and conflict-aware (the user always knows and controls which mods overwrite which files). If everything else fails, this must hold.

### Constraints

- **Tech stack**: Rust backend + Tauri (webview frontend) — chosen by the project owner for a small, fast, native-feeling desktop app
- **Platform**: Linux desktop only for v1; manages Windows games run via Steam Proton / Wine
- **Mod source**: NexusMods only for v1 (API/auth, downloads, Collections)
- **Distribution**: AppImage (single portable binary) as the primary v1 channel
- **Deployment strategy**: To be determined during research — evaluate symlink vs hardlink vs overlay/VFS approaches for correctness and reversibility under Proton

<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->

## Technology Stack

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| **Rust** | 1.85+ (2024 edition) | Backend language | Project owner's constraint; gives precise, safe filesystem control needed for reversible deployment. The 2024 edition is the current default toolchain. |
| **Tauri** | 2.11.x | Desktop app shell (Rust core + system WebView) | Stable since late 2024; ~10-20 MB bundles & ~50% less RAM vs Electron. Uses the system WebKitGTK on Linux, ships AppImage out of the box (a hard project requirement). v2 has the mobile/desktop unified plugin model and a proper capabilities/permissions security layer. |
| **Svelte (SvelteKit in SPA/static mode) + TypeScript** | Svelte 5.x | Frontend framework | Smallest bundle + fastest cold start of the Tauri-supported frameworks (matters because WebKitGTK on Linux is slower than Chromium). Svelte 5 runes give ergonomic local state without a heavy store library. `create-tauri-app` ships a first-class Svelte template. **React is the acceptable fallback** if you want the larger component ecosystem (see Alternatives). |
| **tokio** | 1.52.x | Async runtime | De-facto standard async runtime; required by reqwest, sqlx, and most of the ecosystem. Use the `rt-multi-thread`, `fs`, `macros` features. Tauri v2 commands integrate cleanly with an existing tokio runtime. |
| **reqwest** | 0.13.x | HTTP client for the NexusMods API | The standard high-level async client. Use `rustls-tls` (not native OpenSSL) for self-contained AppImage builds, plus `json`, `stream` (for download progress), and `gzip`/`brotli`. |
| **SQLite via rusqlite** | rusqlite 0.40.x (bundled) | Local mod database / manifests / deploy ledger | A single embedded file DB is exactly right for a desktop app. `rusqlite` with the `bundled` feature statically links SQLite (no system dep → clean AppImage). Use it for the mod catalog, profiles, load order, and the **per-file deployment ledger** that makes uninstall reversible. See Alternatives for sqlx vs sea-orm trade-off. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| **tauri-plugin-deep-link** | 2.4.x | Register & receive `nxm://` (and OAuth `nxm://oauth/callback`) links | Essential. Handles one-click "Mod Manager Download" buttons on the Nexus site and the OAuth2 redirect. On Linux it writes the `.desktop` MIME handler for the `nxm` scheme. |
| **tauri-plugin-single-instance** | 2.x | Forward a second `nxm://` invocation to the already-running app | Required so a browser click while the app is open is routed to the live instance instead of launching a second one. |
| **tauri-plugin-store** | 2.4.x | Lightweight key/value app settings (window state, last game, prefs) | Use for non-relational settings only; keep mod/manifest data in SQLite. |
| **oauth2** | 5.x | OAuth2 Authorization Code + PKCE flow against NexusMods | Implements the S256 PKCE flow cleanly; pair with the deep-link plugin for the `nxm://oauth/callback` redirect and reqwest for the token exchange. |
| **keyring** | 3.x | Store the NexusMods refresh token / API key in the OS secret store | On Linux uses the Secret Service (GNOME Keyring / KWallet). Avoids writing long-lived credentials to plaintext. |
| **governor** | 0.10.x | Client-side rate limiting | Enforce the Nexus quota (300 req, 600 premium; +1/sec recovery) with a token-bucket so you never get throttled/banned. Read the `X-RL-*` response headers and back off. |
| **zip** | 8.x | Extract `.zip` mod archives | Pure-Rust, MIT. The most common mod format alongside 7z. |
| **sevenz-rust2** | 0.21.x | Extract `.7z` mod archives | Pure-Rust, MIT/Apache. Active maintained fork of the abandoned `sevenz-rust`. 7z is the dominant Nexus archive format. |
| **tar** + **xz2** + **flate2** | 0.4 / 0.1 / latest | Extract `.tar.gz` / `.tar.xz` (rare, mostly Linux-native mods) | Only needed for completeness. |
| **reflink-copy** | 0.1.x | Copy-on-write clone of files where the FS supports it (Btrfs/XFS/bcachefs) | Best-of-both deploy primitive: instant like a hardlink, but an independent inode so edits don't corrupt the staged copy. Falls back to copy automatically. See Architecture note below. |
| **steamlocate** | 2.1.x | Locate Steam install, parse `libraryfolders.vdf`, enumerate libraries & installed apps | Purpose-built; handles multi-library setups and returns app install dirs by AppID. Saves you parsing VDF by hand. |
| **keyvalues-serde** (+ **keyvalues-parser**) | 0.2.x | Parse `appmanifest_<id>.acf` and `config.vdf` fields steamlocate doesn't expose | Use for compatdata/Proton-prefix discovery (`STEAM/steamapps/compatdata/<appid>/pfx`) and `config.vdf` Proton mapping. |
| **walkdir** | 2.x | Recursive directory traversal for staging & conflict detection | Standard. |
| **serde** + **serde_json** | 1.x | (De)serialize NexusMods API JSON, Collection manifests, profiles | Universal. |
| **tracing** + **tracing-subscriber** | 0.1 / 0.3 | Structured logging | Essential for diagnosing deploy/Proton issues on user machines. |
| **thiserror** + **anyhow** | 2.x / 1.x | Error types (libraries) / error context (app boundary) | Standard pairing. |
| **refinery** *(if rusqlite)* or sqlx migrations | 0.9.x | Versioned schema migrations for the local DB | The deploy ledger schema will evolve; migrations protect existing user installs. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| `create-tauri-app` | Scaffold the Tauri v2 + Svelte project | `npm create tauri-app@latest` → choose Svelte + TypeScript. |
| Tauri CLI v2 (`tauri-cli`) | Dev server, build, AppImage bundling | `tauri build --bundles appimage`. Linux bundling needs `libwebkit2gtk-4.1-dev` + `libappindicator`/`librsvg` at build time. |
| `cargo-deny` | License & advisory auditing | Critical here — use it to fail the build if anything pulls in the non-free UnRAR source (see What NOT to Use). |
| `sqlx-cli` *(if sqlx)* | Compile-time-checked query prep | Only if you choose sqlx over rusqlite. |

## Installation

# Scaffold (frontend + Tauri shell)

# Rust crates (Cargo.toml — src-tauri/)

# Dev / CI

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| **rusqlite (bundled)** | **sqlx (SQLite)** | Choose sqlx if you want fully async DB access and compile-time-checked SQL. Downside: more ceremony, an async-everywhere DB layer is overkill for a single-user desktop DB, and `sqlx 0.9` query macros need a build-time DB. rusqlite is simpler and the deploy ledger is small/transactional. |
| **rusqlite** | **sea-orm 1.x** | Choose SeaORM if you prefer a full async ORM with entities/relations over hand-written SQL. Adds weight; unnecessary for this schema size. |
| **rusqlite/SQLite** | **sled 0.34** | Avoid — see What NOT to Use. |
| **Svelte 5** | **React 19** | Choose React if team familiarity or a specific component library (e.g. a complex data-grid for the conflict/load-order view) outweighs bundle size. React's larger bundle is a real cost on WebKitGTK. |
| **Svelte 5** | **SolidJS** | Solid matches Svelte on performance with JSX ergonomics; pick it if the team prefers React-style code but wants Svelte-class speed. Smaller ecosystem than React. |
| **reflink-copy + hardlink fallback** | **symlinks** | Symlinks are needed when staging and game dirs are on **different filesystems** (hardlinks/reflinks can't cross FS boundaries) — likely with a separate mods drive. Build the deploy engine to choose per-target: reflink → hardlink → symlink → copy. |
| **oauth2 crate + PKCE** | **Legacy API key + websocket SSO** | The websocket SSO (what MO2/Vortex use) is acceptable as a secondary login path and avoids registering an OAuth client, but OAuth2+PKCE is the forward-looking, recommended path. Plain manual API-key paste should be a last-resort fallback only. |
| **steamlocate** | hand-rolled VDF parsing | Only if steamlocate's data model misses a field you need (then layer keyvalues-serde on top of the raw files). |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| **unrar / unrar_sys crate** | The wrapper is MIT/Apache but it bundles the **UnRAR C++ source**, whose license forbids using it to recreate the RAR algorithm. This makes it **non-free and GPL-incompatible** (Debian/Fedora ship it only in non-free repos). Bundling it in an AppImage you distribute is a licensing liability. | Treat `.rar` mods as a rare edge case: detect them and either (a) shell out to a system `unrar`/`7z` binary if present, or (b) prompt the user. Keep RAR support out of the statically-linked, distributed binary. Enforce with `cargo-deny`. |
| **sled (0.34)** | Effectively unmaintained for years, still pre-1.0/beta, larger on-disk footprint, and you lose SQL/ad-hoc queries you'll want for conflict resolution and reporting. | rusqlite (bundled SQLite). |
| **Electron** | 100 MB+ bundles, high RAM, no native AppImage story comparable to Tauri's — contradicts the "small, fast, native-feeling, AppImage" project goals. | Tauri v2. |
| **sevenz-rust (original)** | Abandoned; bugs unfixed. | sevenz-rust2. |
| **reqwest with default native-tls/OpenSSL** | Dynamically links system OpenSSL → fragile across distros and AppImage portability problems. | reqwest with `rustls-tls` (no system TLS dep). |
| **Copy-only deployment** | Doubles disk usage for huge Bethesda load orders (texture packs are tens of GB) and makes deploy slow. | reflink/hardlink with copy only as the cross-FS fallback. |
| **Overwriting files into the real game dir without a ledger** | Breaks the core safety guarantee (non-destructive, reversible). | Stage every mod, deploy via links, and record every deployed path + original-file backup in the SQLite ledger so uninstall restores pristine state. |
| **NexusMods REST v1 endpoints assumed permanent** | Nexus is migrating to the **GraphQL v2** API; some v1 endpoints are deprecated. | Prefer GraphQL v2 for new data (mods, files, collections); use v1 only where v2 lacks coverage. Verify per-endpoint at api.nexusmods.com / graphql.nexusmods.com. |

## Stack Patterns by Variant

- Per-file deploy strategy must fall back hardlink → symlink (links can't cross FS), and reflink is unavailable across FS.
- Detect target FS at deploy time (`statfs`/device id) and pick the strongest primitive available.
- Use `reflink-copy` for an independent-inode, instant, space-efficient deploy — the safest option (edits to the deployed file can't corrupt the staging copy, unlike hardlinks).
- Use hardlinks for same-FS deploy (instant, space-efficient) but mark the staging copy read-only to preserve the safety invariant; symlink/copy across FS.
- Prefer OAuth2 + PKCE via `nxm://oauth/callback` (deep-link plugin). Store the refresh token in `keyring`. Offer websocket-SSO as secondary and manual API-key paste as last resort.

## Version Compatibility

| Package A | Compatible With | Notes |
|-----------|-----------------|-------|
| tauri 2.11.x | tauri-plugin-* 2.x | Keep all Tauri plugins on the matching 2.x line; mismatched majors break the IPC/permissions contract. |
| reqwest 0.13.x | tokio 1.x | reqwest requires a tokio runtime; ensure `rustls-tls` to avoid an OpenSSL system dep. |
| rusqlite 0.40.x (bundled) | refinery 0.9.x | refinery's rusqlite feature must target the same rusqlite major. |
| sqlx 0.9.x | tokio 1.x | If chosen instead of rusqlite, enable `runtime-tokio-rustls` + `sqlite`. |
| Svelte 5 | Tauri 2 | `create-tauri-app` Svelte template targets Svelte 5; build frontend as a static SPA (adapter-static) for Tauri to embed. |
| AppImage bundling | WebKitGTK 4.1 | Linux build host needs `libwebkit2gtk-4.1-dev`; runtime depends on the user's system WebKitGTK (document minimum). |

## Sources

- crates.io API (api/v1/crates/*) — verified current max-stable versions for tauri 2.11.3, reqwest 0.13.4, tokio 1.52.3, rusqlite 0.40.1, sqlx 0.9.0, sled 0.34.7, steamlocate 2.1.0, keyvalues-serde 0.2.4, reflink-copy 0.1.30, zip 8.6.0, sevenz-rust2 0.21.0, unrar 0.5.8, tauri-plugin-deep-link 2.4.9, sea-orm 1.1.20, refinery 0.9.2 — **HIGH confidence** (authoritative registry).
- Nexus Mods API auth / OAuth2+PKCE / nxm:// / rate limits — modding.wiki OAuth2 guide, DeepWiki (Vortex & MO2 Nexus API), graphql.nexusmods.com, Nexus-Mods/NexusMods.App issue #19 — **MEDIUM** (cross-checked, multiple sources).
- NexusMods.App deployment (hardlink + event-sourced undo + SQLite + Wine/Proton detection) — nexus-mods.github.io/NexusMods.App decision records — **MEDIUM** (official project docs).
- Linux mod-manager deployment patterns (Amethyst, Limo, TMM/Tauri, RadTux VFS) — Nexus Mods listings & forums — **MEDIUM**.
- UnRAR license restriction (non-free, GPL-incompatible) — unrar crate docs.rs/lib.rs + general Debian/Fedora packaging knowledge — **MEDIUM**.
- Tauri v2 frontend framework comparison (Svelte bundle/startup advantage, create-tauri-app templates) — Tauri docs & 2025/2026 comparison articles — **MEDIUM**.

<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->

## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->

## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->

## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, `.github/skills/`, or `.codex/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->

## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:

- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->

<!-- GSD:profile-start -->

## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
