# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> Stack rationale, recommended crate versions, and "what NOT to use" rules live in
> `.claude/CLAUDE.md`. This file covers build/test commands and the big-picture
> architecture. Read both.

## What this is

NexTwist is a Rust + Tauri desktop app that brings safe, fully-reversible mod
management to Linux gamers running Windows games via Steam Proton/Wine. The core
guarantee that overrides everything else: **deployment is non-destructive (the base
game is never modified in place), fully reversible (purge restores a byte-for-byte
pristine game), and conflict-aware.**

## Commands

All Rust commands run from the repo root (a virtual cargo workspace).

```bash
# Headless safety engine — fast, no webview/system deps needed
cargo test --workspace --locked        # full test suite (what CI runs)
cargo test -p nextwist-deploy          # one crate (crate names below)
cargo test -p nextwist-deploy --test crash_recovery   # one integration-test file
cargo test -p nextwist-deploy recover  # tests matching a substring
cargo clippy --workspace --all-targets -- -D warnings  # lint (CI fails on warnings)
cargo deny check advisories bans licenses sources      # supply-chain gate (see below)

# Frontend (SvelteKit static SPA that Tauri embeds)
npm --prefix frontend ci               # install
npm --prefix frontend run build        # build to frontend/build (Tauri's frontendDist)
npm --prefix frontend run check        # svelte-check type check
npm --prefix frontend test             # vitest over the pure $lib modules

# Full desktop app (needs WebKitGTK 4.1 dev libs — see CI for the apt list)
cargo tauri dev                        # run app (auto-runs frontend dev server)
cargo tauri build --bundles appimage   # build the AppImage
```

Note: `src-tauri` is a workspace member, so `cargo test --workspace` compiles it and
therefore requires the WebKitGTK dev libs on the build host. The `crates/*` headless
engine needs none of them. Toolchain is pinned to stable ≥ 1.89 (MSRV, set by
`libloot`); see `rust-toolchain.toml`.

## Architecture

The defining structural decision: **the entire safety-critical engine lives in
`crates/*` as pure, headless Rust with ZERO Tauri dependencies.** The Tauri shell
(`src-tauri/`) is a thin adapter that delegates to those crates and adds no logic.
This keeps the engine unit/property-testable in CI without a webview. Honor this
boundary — do not pull `tauri`/`reqwest`/UI concerns into the `crates/*` engine, and
do not put real logic in the command adapters.

### Crate layers (each crate is `nextwist-<name>`, depended on by workspace alias)

- **core** — shared domain types (`Game`, `ManagedMod`, `Profile`, etc.) and error
  enums. Pure data, no I/O-framework deps. The vocabulary every other crate speaks;
  treat the type *shapes* as a stable contract. Aliased `nextwist_core` (never `core`,
  which would shadow std `::core` that Tauri macros expand to).
- **store** — the single SQLite DB (rusqlite bundled + refinery migrations). Holds the
  game registry, **per-file deploy manifest**, **operation journal**, **vanilla backup
  ledger**, plus the Phase-2 mod/profile/plugin tables. Hard invariant: **no `rusqlite`
  type appears in its public API** — all SQL stays inside `store`; callers speak `core`
  types. Migrations are in `crates/store/src/migrations/V*.sql` (additive, versioned).
- **steam** — locate Steam, resolve install dir + Proton/Wine prefix + staging dir,
  handle Wine case-folding (`casing.rs`).
- **extract** — the untrusted-archive → validated read-only staging-tree transform
  (zip + 7z + shell-out RAR), with zip-slip / symlink-write-through defense.
- **fomod** — the full FOMOD 5.x `ModuleConfig.xml` engine as a pure transform (parse →
  condition → resolve), plus `wizard::project` for the ordered step/group/option tree a UI
  renders (the spec's `order` attribute is engine truth; the projection serializes straight
  to the webview, so the shell mirrors no types). `resolve` is a **pure dry-run**: it returns
  an ordered file-install plan without touching disk, so the plan can be conflict-previewed
  before it is applied. Malformed input returns a specific `FomodError`, never a silent
  mis-install.
- **nexus** — headless NexusMods client: OAuth2+PKCE, API-key validation, REST v1 +
  GraphQL v2 metadata, download links, streaming download, `governor` rate limiting with
  reactive `X-RL-*` backoff. Async `reqwest` (rustls, redirects disabled,
  `error_for_status()` enforced) and keyring-free — the shell passes token *values* in.
- **deploy** — **the crown jewel.** The reversible deployment engine. Per-target FS
  capability probe → `reflink → hardlink → symlink → copy` method ladder (EXDEV/
  `CrossesDevices` fallback), conflict resolution by mod rank, byte-for-byte pristine
  purge. Also profile switching and verify/repair.
- **loadorder** — headless plugin/load-order management via `libloot`/`esplugin`:
  Proton-prefix AppData resolution, plugin scan/classify, LOOT masterlist sort,
  asterisk-format `plugins.txt` round-trips. Tauri-free (uses blocking `reqwest`).
- **testkit** — dev-dependency test helpers: fake game/staging tree builders + blake3
  byte-for-byte pristine-tree assertions used across the engine test suites.

### Crash-safety model (the central idea in `deploy`)

A filesystem syscall (`link`/`reflink`/`copy`) and the DB row recording it cannot be
made atomic together. So safety is **not** SQLite-WAL alone — it is an explicit
**intent-before-act operation journal**: intent is written `pending` *before* the
syscall and flipped to `done` *after*, combined with **idempotent file ops** so
replaying a half-finished op after a crash is always safe. On launch, the shell calls
`deploy::recover_on_launch` for every managed game *before the UI is served*
(`src-tauri/src/lib.rs`), replaying any interrupted op. When touching `deploy`,
preserve the journal ordering and idempotency — it is the reversibility guarantee.

### Tauri shell (`src-tauri/`)

`lib.rs` builds the app, resolves the OS app-data dir, runs startup recovery, and
registers command adapters from `src-tauri/src/commands/` (games, mods, deploy,
conflicts, plugins, profiles). Commands are thin: they lock `AppState` and call into
the engine crates. Frontend is SvelteKit (Svelte 5) built as a static SPA into
`frontend/build` and embedded via `frontendDist`.

## Conventions & guardrails

- **Errors**: `thiserror` enums in the engine crates; `anyhow` only at the
  app/Tauri boundary.
- **TLS**: `reqwest` uses `rustls` only — never native-tls/OpenSSL (keeps the AppImage
  self-contained).
- **`cargo-deny` is load-bearing**: the non-free UnRAR source (`unrar`/`unrar_sys`) is
  banned and licenses are gated. RAR support shells out to a system `unrar`/`7z`.
  Adding a dep that trips `deny.toml` fails CI.
- **Shared dep versions** are pinned once in the root `[workspace.dependencies]`;
  member crates reference them via `<dep>.workspace = true`. Note `rusqlite` is pinned
  to 0.39 (not 0.40) because `refinery 0.9.2` caps its rusqlite feature there.

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
definition of done in `AGENTS.md` — a change is finished when `cargo fmt --check`,
`cargo test --workspace --locked`, `cargo clippy --workspace --all-targets -- -D warnings`,
and (for frontend changes) `npm --prefix frontend run check` + `npm --prefix frontend test`
all pass, and anything touching `deploy`/`store` carries a test proving the reversibility
or crash-recovery property still holds.

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

### Reference documents

`docs/reference/` holds the background research and UI contracts the code cites by name
(`RESEARCH Pitfall 1`, `UI-SPEC §B.2`). They are read-only history: when a decision changes,
change the code and record the new decision as an ADR rather than rewriting the research
that justified the old one. See `docs/reference/README.md`.

> Note: `AGENTS.md` at the repo root is the canonical, tool-agnostic version of this
> guidance (and has the up-to-date crate list). Keep the two in sync.
