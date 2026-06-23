<!-- GSD:GENERATED quick-260623-m42 -->
# Changelog

All notable changes to NexTwist are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

_No changes yet._

## [1.0.0] - 2026-06-23

First public release — the v1.0 MVP. NexTwist brings safe, fully-reversible,
conflict-aware mod management to Linux gamers running Windows games via Steam
Proton/Wine. The core guarantee held throughout: deployment is **non-destructive**
(the base game is never modified in place), **fully reversible** (purge restores a
byte-for-byte pristine game), and **conflict-aware**.

### Added

- **Reversible deployment engine** — per-target `reflink → hardlink → symlink → copy`
  method ladder, an intent-before-act operation journal for crash-safety, byte-for-byte
  pristine purge, and a vanilla-backup ledger. Startup recovery replays any interrupted
  operation before the UI is served.
- **Safe archive extraction** — untrusted `.zip` / `.7z` / `.rar` archives are turned into
  validated read-only staging trees that reject zip-slip / absolute / symlink entries
  (CVE-2025-29787); RAR support shells out to a system tool (no non-free code bundled).
- **Multi-mod management** — file-level conflict resolution by mod rank, plugin load-order
  management via `libloot` (correct master-first `plugins.txt` in the Proton-prefix AppData
  path), LOOT auto-sort, and per-game profiles with fully-reversible switching.
- **NexusMods integration** — OAuth2 + PKCE / API-key login with credentials stored in the
  OS keyring (never plaintext), Premium in-app download with progress, client-side rate
  limiting, and `nxm://` one-click "Mod Manager Download" handoff routed to the live instance.
- **Guided installers** — a FOMOD scripted-installer wizard with live conditional
  re-evaluation and a dry-run conflict preview.
- **Collections** — browse, download, apply (FOMOD choices + load order), deploy, and
  byte-for-byte reversible uninstall of NexusMods Collections.
- **Distribution** — a license-clean Linux AppImage built in CI on tagged releases, with a
  reproducible bundled-binary audit proving no non-free UnRAR and no app-path system-OpenSSL.

### Security

- Per-phase STRIDE threat verification for the engine, multi-mod, FOMOD/Collections, and
  distribution areas (`threats_open: 0`); SSRF defense ensures off-Nexus Collection sources
  are never auto-fetched.

### Known limitations

- **Collections cannot be downloaded live from nexusmods.com** — NexusMods restricts
  Collection-archive download to its own Vortex client. The Collection engine is fully
  functional over an already-fetched manifest, but live ingest from the website is not
  available in v1.0.
- Live OAuth2 login activates once a NexusMods OAuth `client_id` is registered; the API-key
  paste path is the works-today login.
- v1.0 targets Bethesda Creation Engine games (Skyrim SE, Fallout 4) under Steam Proton on Linux.

[Unreleased]: https://github.com/MatrixMagician/NexTwist/compare/v1.0...HEAD
[1.0.0]: https://github.com/MatrixMagician/NexTwist/releases/tag/v1.0
