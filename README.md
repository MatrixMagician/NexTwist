<!-- generated-by: gsd-doc-writer -->
# NexTwist

[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)

Safe, fully-reversible, conflict-aware mod management for Linux gamers running Windows games through Steam Proton / Wine.

NexTwist is a Rust + Tauri v2 desktop app that brings Vortex / Mod-Organizer-2-class modding to Linux. It logs into NexusMods, downloads individual mods and curated Collections, runs FOMOD scripted installers, and deploys everything with a core guarantee that overrides everything else: **deployment is non-destructive** (the base game is never modified in place), **fully reversible** (a purge restores a byte-for-byte pristine game), and **conflict-aware** (you always know and control which mods overwrite which files).

## Features

- **Reversible deployment engine** — a per-target `reflink → hardlink → symlink → copy` method ladder with an intent-before-act operation journal for crash-safety, plus byte-for-byte pristine purge and a vanilla-backup ledger.
- **Conflict resolution & load order** — file-level conflict resolution by mod rank and plugin load-order management via `libloot` (LOOT auto-sort, master-first `plugins.txt` written to the Proton-prefix AppData path).
- **NexusMods login & download** — OAuth2 + PKCE (with an API-key-paste fallback), keyring credential storage, Premium in-app downloads with progress, client-side rate limiting, and `nxm://` one-click handoff.
- **Guided installers & Collections** — a FOMOD scripted-installer wizard with live conditional re-evaluation, plus the full NexusMods Collection lifecycle with byte-for-byte reversible uninstall.
- **Per-game profiles** — fully-reversible profile switching (purge-to-pristine between profiles).
- **AppImage distribution** — a single portable, license-clean binary.

## Requirements

- **Linux desktop** (v1 manages Windows games run via Steam Proton / Wine).
- **To run the AppImage:** a system **WebKitGTK 4.1** runtime (the AppImage builds against the `ubuntu-22.04` glibc / WebKitGTK 4.1 floor for broad compatibility).
- **To build from source:**
  - **Rust** `>= 1.89` (2024 edition; pinned in `rust-toolchain.toml`).
  - **Node.js / npm** (for the SvelteKit frontend).
  - Tauri Linux build libraries: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, plus `patchelf` for AppImage bundling. See `.github/workflows/release.yml` for the exact apt list.

## Installation

The primary v1 distribution channel is an AppImage attached to each GitHub Release:

```bash
# Download the latest AppImage from the Releases page, then:
chmod +x NexTwist_*.AppImage
./NexTwist_*.AppImage
```

Releases: <https://github.com/MatrixMagician/NexTwist/releases>

## Quick Start (from source)

NexTwist is a virtual cargo workspace — there is **no root `package.json`**. Build the Rust engine with `cargo` and the embedded frontend with `npm --prefix frontend`.

```bash
# 1. Clone the repository
git clone https://github.com/MatrixMagician/NexTwist.git
cd NexTwist

# 2. Build the embedded SvelteKit frontend (static SPA Tauri embeds)
npm --prefix frontend ci
npm --prefix frontend run build

# 3. Run the desktop app (needs the WebKitGTK 4.1 dev libs above)
cargo tauri dev
```

To produce the distributable AppImage:

```bash
cargo tauri build --bundles appimage
```

## Development

The safety-critical engine lives in `crates/*` as pure, headless Rust with **zero Tauri dependencies**, so it is unit- and property-testable in CI without a webview. The Tauri shell (`src-tauri/`) is a thin adapter that delegates to those crates.

```bash
# Headless safety engine — fast, no webview/system deps needed
cargo test --workspace --locked                          # full test suite (what CI runs)
cargo test -p nextwist-deploy                            # one crate
cargo clippy --workspace --all-targets -- -D warnings    # lint (CI fails on warnings)
cargo deny check advisories bans licenses sources        # supply-chain gate

# Frontend (SvelteKit static SPA)
npm --prefix frontend run check                          # svelte-check type check
npm --prefix frontend run build                          # build to frontend/build
```

> Note: `src-tauri` is a workspace member, so `cargo test --workspace` compiles it and therefore needs the WebKitGTK 4.1 dev libs on the build host. The `crates/*` headless engine needs none of them.

### Workspace layout

```
crates/
  core        shared domain types (Game, ManagedMod, Profile, ...) and error enums
  store       single SQLite DB (rusqlite bundled + refinery migrations): deploy
              manifest, operation journal, vanilla backup ledger, mod/profile tables
  steam       locate Steam, resolve install dir + Proton/Wine prefix + staging dir
  extract     untrusted archive -> validated read-only staging tree (zip/7z/RAR)
  deploy      the reversible deployment engine (the crown jewel)
  loadorder   headless plugin/load-order management via libloot / esplugin
  nexus       NexusMods OAuth2/API-key auth + download client
  fomod       FOMOD scripted-installer engine
  testkit     dev-dependency test helpers (fake trees + pristine assertions)
src-tauri/    Tauri v2 shell — thin command adapters over the engine crates
frontend/     SvelteKit (Svelte 5) static SPA embedded via frontendDist
```

CI (`.github/workflows/ci.yml`) runs `cargo test --workspace`, `cargo clippy`, and the `cargo deny` supply-chain gate on every push. Releases (`.github/workflows/release.yml`) are tag-triggered (`v*`) and build the AppImage.

## License

NexTwist is licensed under the **GNU General Public License v3.0 or later** (GPL-3.0-or-later). See the [LICENSE](LICENSE) file for the full text.
