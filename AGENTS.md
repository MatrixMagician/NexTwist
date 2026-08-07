# AGENTS.md

Canonical instructions for coding agents (jcode, Claude Code, Codex, etc.) working in
this repository. `CLAUDE.md` is a symlink to this file, so Claude Code loads exactly this
document as project memory and the two can never drift — edit this file, never a copy.
Stack rationale and "what NOT to use" rules live in `.claude/CLAUDE.md`, which is
genuinely distinct content.

## The one rule that outranks everything

**Deployment is non-destructive, fully reversible, and conflict-aware.** The base game
is never modified in place; a purge restores a byte-for-byte pristine game; the user
always knows and controls which mod overwrites which file. If a change you are about to
make weakens that guarantee, stop and surface it instead of shipping it.

## What this is

NexTwist is a Rust + Tauri desktop app bringing Vortex/MO2-class mod management to
Linux gamers running Windows games via Steam Proton/Wine. NexusMods is the only mod
source for v1; AppImage is the primary distribution channel.

## Commands

All Rust commands run from the repo root (a virtual cargo workspace).

```bash
# Headless safety engine — fast, no webview/system deps needed
cargo test --workspace --locked                        # full suite (what CI runs)
cargo test -p nextwist-deploy                          # one crate
cargo test -p nextwist-deploy --test crash_recovery    # one integration-test file
cargo test -p nextwist-deploy recover                  # tests matching a substring
cargo clippy --workspace --all-targets -- -D warnings  # lint (CI fails on warnings)
cargo deny check advisories bans licenses sources      # supply-chain gate

# Frontend (SvelteKit static SPA that Tauri embeds)
npm --prefix frontend ci
npm --prefix frontend run build     # -> frontend/build (Tauri's frontendDist)
npm --prefix frontend run check     # svelte-check
npm --prefix frontend test          # vitest over the pure $lib modules

# Full desktop app (needs WebKitGTK 4.1 dev libs — see .github/workflows/ci.yml)
cargo tauri dev
NO_STRIP=true cargo tauri build --bundles appimage
```

`src-tauri` is a workspace member, so `cargo test --workspace` compiles it and needs the
WebKitGTK dev libs on the host. The `crates/*` engine needs none of them — when you lack
those libs, iterate with `cargo test -p nextwist-<crate>`. Toolchain is pinned to stable

**`NO_STRIP=true` is required for the AppImage on a modern distro**, not optional. Tauri
bundles via `linuxdeploy`, which carries its own ancient `binutils`; that `strip` cannot
parse the `.relr.dyn` relocation section modern glibc emits, so it fails on system
libraries (`libzstd`, `libxml2`, `libxkbcommon`, ...) and the bundle step dies with a bare
`failed to run linuxdeploy`. Setting `NO_STRIP=true` skips that pass and the bundle
succeeds; the binary is already stripped anyway by `strip = true` in `[profile.release]`,
so nothing is lost. Verified on this repo: the plain command fails identically on a
pristine `main` checkout, so it is the environment rather than anything in the tree.
≥ 1.89 (MSRV set by `libloot`); see `rust-toolchain.toml`.

## Architecture

The defining structural decision: **the entire safety-critical engine lives in `crates/*`
as pure, headless Rust with ZERO Tauri dependencies.** The Tauri shell (`src-tauri/`) is a
thin adapter that delegates and adds no logic. This keeps the engine unit- and
property-testable in CI without a webview. Honor the boundary: do not pull
`tauri`/UI/keyring concerns into `crates/*`, and do not put real logic in command adapters.

### Crates (each is `nextwist-<name>`)

- **core** — shared domain types (`Game`, `ManagedMod`, `Profile`) and error enums. Pure
  data. Aliased `nextwist_core`, never `core` (which would shadow the std `::core` that
  Tauri macros expand to). Treat the type *shapes* as a stable contract.
- **store** — the single SQLite DB (rusqlite `bundled` + refinery migrations). Game
  registry, per-file deploy manifest, operation journal, vanilla backup ledger, mod /
  profile / plugin tables. Hard invariant: **no `rusqlite` type in its public API** — all
  SQL stays inside `store`; callers speak `core` types. Migrations live in
  `crates/store/src/migrations/V*.sql` and are additive and versioned; never edit a
  shipped migration, add a new one.
- **steam** — locate Steam, resolve install dir + Proton/Wine prefix + staging dir, handle
  Wine case-folding (`casing.rs`).
- **extract** — untrusted archive → validated read-only staging tree (zip + 7z +
  shell-out RAR), with zip-slip and symlink-write-through defense.
- **fomod** — the full FOMOD 5.x `ModuleConfig.xml` engine as a pure transform: parse →
  condition → resolve, plus `wizard::project` for the ordered step/group/option tree a UI
  renders (the spec's `order` attribute is engine truth, not a UI preference; the projection
  serializes straight to the webview, so the shell mirrors no types). `resolve` is a **pure
  dry-run** producing an ordered file-install plan without touching disk; the plan is
  conflict-previewed before it is applied. A malformed construct returns a specific
  `FomodError`, never a silent mis-install.
- **nexus** — headless NexusMods client: OAuth2+PKCE exchange, API-key validation, REST
  v1 + GraphQL v2 metadata, download-link generation, streaming download, `governor` rate
  limiting with reactive `X-RL-*` backoff. Async `reqwest`, redirects disabled,
  `error_for_status()` enforced. Keyring-free: the shell owns the secret store and passes
  token *values* in.
- **deploy** — **the crown jewel.** Reversible deployment: per-target FS capability probe
  → `reflink → hardlink → symlink → copy` ladder (with EXDEV/`CrossesDevices` fallback),
  conflict resolution by mod rank, byte-for-byte pristine purge, profile switching, and
  verify/repair.
- **loadorder** — headless plugin/load-order management via `libloot`/`esplugin`:
  Proton-prefix AppData resolution, plugin scan/classify, LOOT masterlist sort,
  asterisk-format `plugins.txt` round-trips. Uses blocking `reqwest`.
- **testkit** — dev-dependency helpers: fake game/staging tree builders and blake3
  byte-for-byte pristine-tree assertions used across the engine test suites.

### Crash-safety model (the central idea in `deploy`)

A filesystem syscall (`link`/`reflink`/`copy`) and the DB row recording it cannot be made
atomic together. So safety is **not** SQLite WAL alone — it is an explicit
**intent-before-act operation journal**: intent is written `pending` *before* the syscall
and flipped to `done` *after*, combined with **idempotent file ops** so replaying a
half-finished op after a crash is always safe. On launch the shell calls
`deploy::recover_on_launch` for every managed game *before the UI is served*
(`src-tauri/src/lib.rs`). When touching `deploy`, preserve journal ordering and
idempotency — that is the reversibility guarantee, and it is what the
`crates/deploy/tests/crash_recovery*` suites defend.

### Tauri shell

`src-tauri/src/lib.rs` builds the app, resolves the OS app-data dir, runs startup
recovery, and registers adapters from `src-tauri/src/commands/` (games, mods, deploy,
conflicts, plugins, profiles, plus collections, downloads, fomod, gameconfig, and nexus).
Commands lock `AppState` and call the engine. The frontend
is SvelteKit (Svelte 5 runes) built as a static SPA into `frontend/build` and embedded via
`frontendDist`. New commands need a matching binding in `frontend/src/lib/api.ts`.

The frontend mirrors the engine boundary: `routes/+page.svelte` holds only state wiring
and markup, while pure rules live in unit-tested `$lib` modules (`fomod.ts` for wizard
selection, `plugins.ts` for the masters-first/protected reorder rules, `format.ts` for
display formatting). New UI logic with a testable rule belongs in `$lib`, not the route.

## Conventions and guardrails

- **Errors**: `thiserror` enums in engine crates; `anyhow` only at the app/Tauri boundary.
- **TLS**: `reqwest` uses `rustls` only — never native-tls/OpenSSL (keeps the AppImage
  self-contained).
- **`cargo-deny` is load-bearing**: the non-free UnRAR source (`unrar`/`unrar_sys`) is
  banned and licenses are gated. RAR support shells out to a system `unrar`/`7z`. A dep
  that trips `deny.toml` fails CI.
- **Shared dep versions** are pinned once in the root `[workspace.dependencies]`; members
  use `<dep>.workspace = true`. `rusqlite` is pinned to 0.39 (not 0.40) because
  `refinery 0.9.2` caps its rusqlite feature there.
- **Commits**: Conventional Commits, scoped by phase or crate
  (`feat(08-02): ...`, `fix(deploy): ...`, `docs: ...`). Commit as you go.

## Definition of done

Before claiming a change is complete:

1. `cargo fmt --all --check` is clean (CI gates on this).
2. `cargo test --workspace --locked` passes (or the affected `-p` crates when the host
   lacks WebKitGTK, and say so).
3. `cargo clippy --workspace --all-targets -- -D warnings` is clean.
4. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` is clean (CI gates on this).
   A broken intra-doc link is not cosmetic: it silently misnames the item it points at, and
   module headers are the first thing a reader of a subsystem sees.
5. `cargo deny check advisories bans licenses sources` passes if dependencies changed.
6. `npm --prefix frontend run check` and `npm --prefix frontend test` pass if frontend
   files changed (both are CI-gated).
7. Anything touching `deploy`/`store` has a test proving the reversibility or
   crash-recovery property still holds.
8. If you touched the deploy **method ladder**, re-run `crates/deploy` with `TMPDIR` on a
   CoW filesystem (see the reflink blind spot below).
9. If you touched **startup, `store`, or a migration**, build and actually launch the app
   against your real app-data database — `cargo tauri build --bundles appimage` is not
   enough, it must start. The whole suite runs on empty databases, so an entire class of
   failure is invisible to it (see the migration-checksum blind spot below).
10. If you touched the **frontend**, LOOK at the running app, do not just read its log. A
    blank window, a placeholder heading and a correct render all produce identical startup
    output. `spectacle -b -n -f -o shot.png` captures it on a Wayland session. Note that
    `cargo build --release -p nextwist` skips the bundling that embeds the frontend, so that
    binary shows "Could not connect to localhost" and tells you nothing — use
    `cargo tauri build`. **Look at it with DATA in it**, not just the empty state: the
    managed-game radio was stretched across its row by a text-input CSS rule, and that row
    does not exist until a game is added (see the empty-state blind spot below).

All gates were run green on `main` as of 2026-08-02, so a failure you see is
something you introduced, not pre-existing noise. Some caveats worth knowing:

- **CI never exercises the reflink rung.** GitHub runners are ext4 and `TempDir` defaults
  to `/tmp`, so `caps.reflink` is false there and every reflink assertion in the suite is
  vacuous — you can break copy-on-write deployment and still see a green tick. The
  strongest rung of the ladder is therefore only truly tested on a developer machine
  whose `TMPDIR` is on btrfs/XFS/bcachefs:

  ```bash
  mkdir -p target/reflink-tmp   # inside the repo, which is on the dev btrfs volume
  TMPDIR="$PWD/target/reflink-tmp" cargo test -p nextwist-deploy
  ```

  `tests/reflink_rung.rs` is the one that matters: it asserts a reflink is an independent
  inode and that writing through a deployed file cannot corrupt read-only staging. It
  skips cleanly (printing why) when the filesystem has no CoW support, so a silent skip
  in CI is expected and a silent skip locally means you proved nothing.
- **A green suite does not mean the app starts.** Every test opens a fresh database, so
  nothing in CI ever compares a migration against a *stored* checksum. refinery records one
  per applied migration and refuses to open a database that disagrees, which means editing
  a shipped migration — including **only its comments**, since the checksum covers the whole
  file — bricks every existing install with `applied migration V1__init is different than
  filesystem one V1__init`, and the window never opens. This happened: a docs sweep
  retouched comments in V1/V2/V3/V5, all 48 test binaries and both CI runs stayed green, and
  the built app died on launch. `shipped_migration_checksums_are_frozen` in
  `crates/store/src/db.rs` now pins every shipped checksum, but the durable habit is to
  launch the thing.
- **The whole suite runs on synthetic game trees.** Every fixture builds its `Data/` dir
  from hand-written bytes, so genuine header flags, Creation Club content, Bethesda's
  mixed-case filenames and multi-megabyte plugins are never exercised. Three tests close
  that gap by running over a COPY of a real install — `real_game_roundtrip.rs` (deploy →
  verify → repair → purge, asserting the tree returns byte-for-byte),
  `real_plugin_scan.rs` (discovery + classification), `real_libloot_seam.rs` (the libloot
  Linux seam: protected-master probe, asterisk `plugins.txt` round-trip, user-order splice),
  `real_profile_switch.rs` (A→B→A with no cross-profile leakage), and
  `real_archive_workflow.rs` (archive → extract → stage → deploy → purge, the full user
  workflow). They skip cleanly without the sandbox, so CI never runs them:

  ```bash
  scripts/realtest-setup.sh "$HOME/SteamLibrary/steamapps/common/Skyrim Special Edition"
  cargo test --workspace
  ```

  The real install is only ever read, and each test copies the sandbox into its own temp
  dir before deploying — so a mid-run failure cannot leave a damaged tree that the next run
  would mistake for a pristine baseline. That protection was added after an experiment
  proved the hazard was real.
- **The empty state is not the product.** Launching the app shows you an empty account
  panel and two empty lists; the sections that do the work (install/deploy, conflicts,
  plugins, profiles) only render once a game is selected, so a layout or wiring bug in them
  is invisible to a first-run screenshot. Two ways to get data on screen without touching
  the real database: point `XDG_DATA_HOME` at a throwaway dir and populate it with
  `cargo test -p nextwist-deploy --test ui_fixture -- --ignored`, or serve
  `frontend/build` with a stubbed `window.__TAURI_INTERNALS__.invoke` and render it in a
  headless browser. The latter shows every section at once and is how the radio-width bug
  surfaced.
- `cargo deny` carries two documented `ignore`d advisories (RUSTSEC-2026-0194/0195) for
  the vulnerable `quick-xml <0.41` that Tauri pulls in transitively via `plist` at build
  time. A `[[bans.deny]]` rule with `wrappers = ["plist"]` keeps that exception pinned to
  that one path — **if `quick-xml <0.41` ever reaches our own crates, `bans` fails.**
  `crates/fomod` parses untrusted mod XML, so never relax that floor.
- `licenses` prints one `unmatched license allowance` warning (`Unicode-DFS-2016`). It is
  a warning, not a failure.

## Workflow

Work is driven by **jcode** plus the Matt Pocock engineering skills — there is no separate
planning system to keep in sync, and no planning artifacts to update. The durable record of
a change is the code, its tests, the Conventional Commit, and the GitHub issue it closes.

Reach for the skill that matches the shape of the work:

| Situation | Skill |
| --- | --- |
| A vague idea or plan that needs stress-testing before you build | `/grilling`, `/grill-with-docs` |
| A change big enough to need decomposition | `/to-tickets`, `/wayfinder` |
| Turning a discussion into a written spec on the tracker | `/to-spec` |
| Building a feature or fixing a bug test-first | `/tdd` |
| Something is broken, slow, or throwing | `/diagnosing-bugs` |
| Reviewing a branch or PR against standards and spec | `/code-review` |
| Reshaping a module's interface, or finding deepening opportunities | `/codebase-design`, `/improve-codebase-architecture` |
| Suspected over-engineering | `/ponytail-review`, `/ponytail-audit` |
| Naming a domain concept or recording a decision | `/domain-modeling` |
| Triaging incoming issues and external PRs | `/triage` |
| Handing work to another agent or session | `/handoff` |
| Not sure which applies | `/ask-matt` |

Edit code directly; there is no gate to route through. What is NOT optional is the
[definition of done](#definition-of-done) above — a change is finished when the gates pass
and anything touching `deploy`/`store` carries a test proving the reversibility or
crash-recovery property still holds.

## Agent skills

### Issue tracker

Issues live in GitHub Issues for `MatrixMagician/NexTwist`, driven by the `gh` CLI. See
`docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles, using their default label strings
(`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See
`docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` plus `docs/adr/` at the repo root. See
`docs/agents/domain.md`. Neither exists yet — `/domain-modeling` creates them lazily when a
term or decision actually needs pinning down, so do not scaffold them upfront.
