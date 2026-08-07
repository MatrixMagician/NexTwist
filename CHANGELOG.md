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
- **The shell keyring's single-implementor `KeyringBackend` trait is gone** — the trait
  plus two test fakes existed only to inject one failure mode. All the branching lives in
  three pure error mappers, which are now tested directly: a raw `keyring::Error` is
  constructible in a test, so simulating a machine with no Secret Service needs no DBus
  session. The no-plaintext-fallback invariant is unchanged and still structural (the
  module reaches for no file API at all), and coverage went from four cases to six.
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

- **Three public helpers kept alive only by their own tests.** `loadorder::scan_plugins`
  and `scan_plugins_for` were thin wrappers over `scan_plugin_views_for`, which the command
  layer already calls directly. Propping them up was `game_id_for_data` — a stub that
  ignored both parameters and returned a hardcoded `GameId::SkyrimSE`, i.e. the plugin layer
  quietly deciding which game's header semantics apply. Harmless while the flags `scan`
  reads match across SkyrimSE and Fallout 4, wrong the moment a fourth game arrives; the
  tests it supported now go through the production path with an explicit `GameId`.
  `store::backup_key_exists` was dead *and* misleading: its doc claimed to gate
  content-addressed dedupe, which actually checks the filesystem, pointing anyone auditing
  the backup path at the wrong mechanism. Net 55 lines, no behaviour change.

- **155 citations pointing at documents that no longer exist.** Deleting `.planning/` was
  supposed to take its ~750 source-comment citations with it, but the sweep was
  incomplete: 93 references to `Plan 0N` / `RESEARCH ...` / `WR-0N` / `D-NN` /
  `threat T-05-NN` / `UI-SPEC` survived across 28 files, plus a second family of 62
  requirement labels (`ENV-03`, `NEXUS-02`, `SFDET-02`, `FOMOD-01`, `PROF-02`, ...) that
  nothing in the repo defines either. This is the rustdoc problem one level up: a dead
  citation asserts a justification exists somewhere and then fails to deliver it, which
  is worse than no reference because it looks authoritative. Where the rationale was
  worth keeping it is now stated inline (`V3__profile_fks.sql`'s referential-integrity
  rebuild, `deny.toml`'s UnRAR ban, the `release.yml` action pin); where the citation
  was decoration on prose that already stood alone it is simply gone; and comments frozen
  in the future tense of a completed plan now describe what is (`crates/nexus`'s module
  layering, the `nxm://` deep-link plugin that *is* registered, `crates/loadorder`'s
  sibling modules). Comment-only: the store migrations changed not one SQL statement.

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

- **Every unmodded game reported "Drift detected".** `pristine` folded in the orphan walk,
  which by definition contains the untouched vanilla game tree, so a freshly added game
  with nothing deployed came back non-pristine with a five-figure orphan count — the first
  thing the app said about a user's game, and pure noise. `pristine` now answers only "is
  our deployment consistent with the manifest" (missing / changed / INI drift). Both orphan
  sets are excluded, including `orphan_dirs`: a vanilla game shipping an empty directory
  (Bethesda titles do, e.g. `Data/Video`) was flagged too. Orphans stay fully reported, and
  purge still refuses to delete anything it cannot explain. The byte-for-byte pristine
  guarantee is a separate blake3 whole-tree comparison and is untouched.

- **The built app refused to start.** A docs sweep rewrote comments inside the shipped
  `V1`/`V2`/`V3`/`V5` migrations. refinery checksums the whole file, comments included, so
  every existing install panicked on launch with `applied migration V1__init is different
  than filesystem one V1__init` — the window never opened, leaving no route to a purge or
  an uninstall. Every gate stayed green because every test opens an *empty* database and so
  never compares against a stored checksum. The migrations are restored byte-for-byte,
  `shipped_migration_checksums_are_frozen` now pins each shipped checksum (verified to fail
  on a comment-only edit), and the definition of done requires actually launching the app
  for any change to startup, `store`, or a migration.

- **Crash recovery could destroy a vanilla game file and then report the game pristine.**
  The engine writes a durable `pending` intent *before* taking the vanilla backup, so a
  crash — or an ordinary I/O failure such as a full or read-only disk — in that window
  left a journal row declaring intent to overwrite a file no copy existed of. Two defects
  compounded there. Recovery reconstructed the staged source as `staging_dir/<target_rel>`,
  a path production never produces (both install paths stage each mod in a per-mod
  subdirectory), so forward recovery could never fire and every interrupted deploy took
  the roll-back branch instead. That branch deleted the target unconditionally and then
  called `restore_vanilla`, which returns `Ok(false)` when the ledger has no row —
  discarded. Net effect: an untouched vanilla file deleted outright, `verify()` reporting
  `pristine = true`, and a purge with nothing to restore. Now the staging root is recorded
  with the intent (migration `V6`, nullable so pre-existing rows fall back to the old
  reconstruction), forward replay backs up before it overwrites, and recovery refuses
  rather than touching an original it cannot first preserve — a transient disk failure
  keeps the intent pending for a later retry instead of costing the user their file. Four
  tests in `crates/deploy/tests/recovery_vanilla_safety.rs` pin the behaviour, and a
  `V5 → V6` upgrade test proves a pre-existing pending row survives the migration.
- **`repair` silently restored nothing for any mod staged the way production stages
  them.** The same root cause as above, in the sibling code path: it reconstructed the
  staged source as `staging_dir/<target_rel>`, so `redeploy_from_staging` returned `false`
  for every file and repair reported success having changed nothing. Nothing was destroyed
  (it refuses to fabricate content rather than guess), but `verify` + `repair` is the tool
  a user reaches for when a deployment drifts, and a full purge and redeploy was the only
  way out. No new schema was needed: the manifest already records each file's
  `source_mod`, and `ManagedMod` carries that mod's `staging_root`.
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
- **Rustdoc warnings are now a CI failure, and the 22 existing ones are gone.**
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` runs alongside clippy. A dead
  intra-doc link is not cosmetic: it silently misnames the item it points at, and the worst
  offenders sat in module headers, which are the first thing a reader of a subsystem sees
  (the `deploy` journal header documented the intent-before-act protocol as
  `[begin]`/`[finish]`, neither of which exists). The fixes disambiguate `fomod::resolve`
  the module from `resolve()` the function, point unresolved links at real names
  (`TOKEN_BASE`, `NexusClient::mod_file_metadata`, `RateLimiter::note_headers`), de-link
  private items rather than leak them into public docs, and backtick `<GameName>` so
  rustdoc stops parsing it as an HTML tag.
- **The reflink rung of the deploy ladder is now genuinely tested, and its CI blind spot
  is documented.** CI runners are ext4 and `TempDir` defaults to `/tmp`, so `caps.reflink`
  is false there and every reflink assertion in the suite was vacuous — copy-on-write
  deployment, the strongest and safest rung, could break with a green tick.
  `crates/deploy/tests/reflink_rung.rs` now asserts (only on a CoW filesystem, skipping
  cleanly elsewhere) that reflink is chosen and succeeds without falling back, that the
  result is an independent inode rather than a hardlink's shared inode, that the recorded
  method is the one that truly ran, and that writing through a deployed file cannot
  corrupt read-only staging. `AGENTS.md` documents the `TMPDIR`-on-btrfs run and adds it
  to the definition of done for changes touching the method ladder.

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
