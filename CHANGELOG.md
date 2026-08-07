# Changelog

All notable changes to NexTwist are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **`CLAUDE.md` is now a symlink to `AGENTS.md`** — the two were hand-maintained
  near-duplicates with a standing instruction to keep them in sync. Drift is now
  impossible by construction, and `.claude/CLAUDE.md` (stack rationale and the "what NOT
  to use" rules) is untouched.
- **The `DeploymentMethod` trait collapsed into the ladder** — four one-line implementors
  in four files, four byte-identical `remove_file` bodies, and a `Box<dyn>` per deployed
  file became a single `deploy_one` match on the `DeployMethod` tag. Ladder semantics are
  unchanged: the reflink → hardlink → symlink → copy weakening order, the EXDEV
  downgrade, remove-if-present-then-create idempotency, and per-file-only deployment all
  hold, with each per-method safety note preserved on its match arm.
- **The Starfield INI editor uses stdlib byte-slice helpers** — `trim_ascii`,
  `strip_suffix`, and `split_inclusive` replace three hand-rolled index-arithmetic loops
  in the module that carries the byte-fidelity guarantee on a user-owned config file.
- **The FOMOD dry-run preview is just the plan** — the three-valued `ConflictClass` had
  one reachable variant, because `fomod::resolve` is itself the gate and returns either a
  deduplicated, conflict-free plan or a typed error. The enum, its TypeScript mirror, its
  dead-code allow, and the two wizard branches that could never render are gone. Cross-mod
  contests remain the conflict-and-priority surface's job.
- **Frontend logic moved behind a testable seam** — the FOMOD wizard selection rules, the
  masters-first / protected-master reorder rules, and the display formatters now live in
  pure `frontend/src/lib/{fomod,plugins,format}.ts` modules instead of inline in
  `+page.svelte`, covered by 25 vitest cases.
- **Session auth centralised** — `AppState::session_auth` / `nexus_client` are now the one
  definition of how a NexusMods session authenticates and how the shared rate limiter is
  wired, replacing copies in the download and Collection adapters.
- **FOMOD wizard projection moved into the engine** — `fomod::wizard::project` owns the
  spec's `order` attribute (steps/groups/plugins) and the authored type-state, and
  serializes straight to the webview. The adapter's six mirrored DTO types are gone
  (`commands/fomod.rs` 691 → 502 lines), with the wire shape pinned by a test.
- **Plugin state merge moved into the engine** — `loadorder::merge_plugin_state` /
  `enabled_names` now own the D-07/D-13 merge (stored enable/order + protected stamping +
  display sort) that the Tauri adapter previously hand-rolled.
- **`Plugins.txt` reads and view downgrades moved into the engine** —
  `loadorder::read_plugins_txt` (with its absent-means-empty rule) and
  `loadorder::view_to_plugin` replace adapter-side copies.
- **Staging subdir naming consolidated** — `extract::staging_dir_name` replaces the
  byte-identical `sanitize()` copies in the download and FOMOD adapters, with a
  property-style test asserting the result is always exactly one normal path component.

### Removed

- **Stale `#[allow(dead_code)]` attributes and unreachable helpers** — four allows whose
  "wired up later" plan references had all since come true, plus `CasingMap::len`, which
  had zero callers anywhere.
- **The unused `oauth2` `reqwest`/`rustls-tls` features** — oauth2 is used for S256 PKCE
  and CSRF state only, so the features dragged a second, entirely unused HTTP stack
  (reqwest 0.12 + webpki-roots) into the build.
- **GSD tooling, its planning documents, and every reference to them.** The repo no longer
  carries a parallel planning system: `.planning/` is gone and `AGENTS.md`/`CLAUDE.md`
  document jcode plus the engineering skills as the way work happens. The research and
  UI-spec documents went with it, and the ~750 source-comment citations that pointed at
  them (`RESEARCH Pitfall 4`, `UI-SPEC §B.2`, `WR-03`, ...) were rewritten so each comment
  states its own reasoning instead of referring to a document that no longer exists.

### Fixed

- **`deploy_collection` / `uninstall_collection` accepted a Collection belonging to a
  different game** — uninstall purges one game's install and then deletes the Collection's
  staged trees, so a mismatched pair could purge one game while destroying another's staged
  mods. Both commands now refuse the mismatch up front.
- Two stray NUL bytes in `frontend/src/routes/+page.svelte` made git treat the file as
  binary, so every diff and code review of the UI showed only `Bin`.
- `uninstall_collection` re-queried every mod for the game once per collection member
  instead of looking the row up by id.

### CI

- `svelte-check` and the frontend unit tests are now gated in CI; the definition of done
  listed them but no workflow step ran them.

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
