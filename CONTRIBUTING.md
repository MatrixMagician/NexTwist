<!-- generated-by: gsd-doc-writer -->
# Contributing to NexTwist

Thanks for your interest in contributing to NexTwist — a Rust + Tauri v2 mod manager
that brings safe, fully-reversible mod management to Linux gamers running Windows games
via Steam Proton/Wine.

The project lives at <https://github.com/MatrixMagician/NexTwist> and is licensed under
**GPL-3.0-or-later**. By contributing, you agree that your contributions are licensed
under the same terms.

## The core invariant you must protect

Before anything else, understand the one guarantee that overrides every other concern:

> **Deployment is non-destructive (the base game is never modified in place), fully
> reversible (a purge restores a byte-for-byte pristine game), and conflict-aware
> (the user always knows and controls which mods overwrite which files).**

Two structural rules keep this guarantee enforceable:

- **The safety-critical engine lives in `crates/*` as pure, headless Rust with ZERO
  Tauri dependencies.** This is what lets the engine be unit- and property-tested in CI
  without a webview. Do not pull `tauri`, `reqwest`, or any UI concern into the
  `crates/*` engine.
- **The Tauri shell (`src-tauri/`) is a thin adapter.** Command adapters lock state and
  delegate to the engine crates — they add no logic. Do not put real logic in the
  command adapters.

If a change cannot honor the invariant, it does not land. When touching the `deploy`
crate in particular, preserve the intent-before-act operation journal ordering and the
idempotency of file ops — that is the reversibility guarantee in code.

## Development setup

This document does not duplicate setup instructions. For getting up and running:

- See **GETTING-STARTED.md** for prerequisites (Rust toolchain, WebKitGTK dev libs) and
  your first run.
- See **docs/DEVELOPMENT.md** for local development setup, build commands, and code-style
  tooling.
- See **docs/ARCHITECTURE.md** for the crate-layer breakdown and the crash-safety model.

The repository is a virtual Cargo workspace. The headless engine in `crates/*` needs no
system libraries; only the full desktop app (`src-tauri`, a workspace member) needs the
WebKitGTK 4.1 dev libs.

## Coding standards

- **Formatting and linting are enforced by CI.** Run `cargo fmt` (rustfmt is pinned via
  `rust-toolchain.toml`) and `cargo clippy --workspace --all-targets -- -D warnings`
  before you push. CI fails on any clippy warning.
- **Errors:** use `thiserror` enums in the engine crates (`crates/*`); use `anyhow` only
  at the app/Tauri boundary (`src-tauri/`).
- **TLS:** `reqwest` must use `rustls` only — never `native-tls`/OpenSSL. This keeps the
  distributed AppImage self-contained across distros.
- **Licensing constraint (cargo-deny is load-bearing):** the non-free UnRAR source
  (`unrar` / `unrar_sys`) is banned in `deny.toml`, and only the allow-listed permissive
  / GPL-compatible licenses are permitted in the shipped binary. RAR support shells out
  to a system `unrar`/`7z` binary instead of linking the banned source. Adding a
  dependency that trips `deny.toml` (a banned crate or a disallowed license) fails CI.
- **Shared dependency versions are pinned once** in the root `[workspace.dependencies]`
  table in `Cargo.toml`. Member crates reference them via `<dep>.workspace = true` so
  versions stay aligned — add or bump shared deps there, not per-crate.

## Before you open a pull request

All three of the following must pass locally. CI runs the same commands and will block
the PR if any fail:

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check advisories bans licenses sources
```

Note that `cargo test --workspace` compiles `src-tauri`, so it requires the WebKitGTK
dev libs on your build host (see the CI workflow for the exact `apt` package list). If
you are only touching the headless engine, you can iterate faster against a single crate
(for example `cargo test -p nextwist-deploy`) — but run the full workspace suite before
submitting.

## Pull request guidelines

- **Branch from `main`** and keep your branch focused on a single change.
- **Write atomic, well-scoped commits** with clear messages. Split unrelated changes
  into separate commits (and ideally separate PRs).
- **Add or update tests** for any behavior change. Safety-critical changes in `deploy`,
  `store`, or `extract` need test coverage proving the pristine/reversible guarantee
  still holds; the `testkit` crate provides fake game/staging tree builders and blake3
  byte-for-byte pristine-tree assertions for this.
- **Respect the crate boundary:** no Tauri/UI/`reqwest` dependencies in `crates/*`, and
  no business logic in `src-tauri/` command adapters.
- **Ensure the three pre-PR checks pass** (tests, clippy, cargo-deny) before requesting
  review.
- **Describe the change** in the PR: what it does, why, and how you verified the safety
  invariant is preserved.

> Internal planning artifacts live under `.planning/`. You do not need to touch them to
> contribute a fix or feature — focus on the code change, tests, and PR description.

## Reporting issues

Report bugs and request features through GitHub Issues at
<https://github.com/MatrixMagician/NexTwist/issues>.

For bug reports, please include:

- **Steps to reproduce**, as precisely as possible.
- **Expected vs. actual behavior.**
- **Your environment:** Linux distribution, filesystem type(s) of the staging and game
  directories (deploy behavior differs across reflink/hardlink/symlink/copy), Steam
  Proton/Wine version, and the affected game.
- **Relevant logs.** NexTwist uses structured `tracing` logging — include any output
  from the deploy or recovery path when a deployment, purge, or load-order operation
  misbehaves.

Because the safety guarantee is paramount, any report of a deployment that left a game
in a non-pristine state, or a purge that did not fully restore, is treated as a
high-priority bug — please flag it clearly.
