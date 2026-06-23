<!-- generated-by: gsd-doc-writer -->

# Development

This guide covers local development setup, build and quality commands, code style,
and the contribution flow for NexTwist. NexTwist is a Rust + Tauri v2 virtual cargo
workspace: a headless safety engine in `crates/*` plus a thin Tauri shell in
`src-tauri/` and a SvelteKit static SPA in `frontend/`.

## Local Setup

For prerequisites and a first-run walkthrough, see
[GETTING-STARTED.md](./GETTING-STARTED.md). The steps below are the development-oriented
setup (running the full app and the headless engine locally).

1. Clone and enter the repository:

   ```bash
   git clone https://github.com/MatrixMagician/NexTwist.git
   cd NexTwist
   ```

2. Install the Rust toolchain. The workspace is pinned to **stable `>= 1.89`** (MSRV)
   via `rust-toolchain.toml`, with the `rustfmt` and `clippy` components. `rustup` reads
   this file automatically, so no manual `rustup` command is required.

3. Choose your build target:

   - **Headless engine only** (`crates/*`) — no webview or system GUI libraries needed.
     You can run the full engine test suite immediately:

     ```bash
     cargo test -p nextwist-deploy
     ```

   - **Full desktop app** (`src-tauri/` + `frontend/`) — requires the WebKitGTK 4.1 dev
     libraries. On Debian/Ubuntu, install the same set CI uses:

     ```bash
     sudo apt-get install -y \
       libwebkit2gtk-4.1-dev \
       libgtk-3-dev \
       libayatana-appindicator3-dev \
       librsvg2-dev \
       build-essential \
       curl wget file libssl-dev
     ```

4. Install frontend dependencies and run the desktop app in dev mode:

   ```bash
   npm --prefix frontend ci
   cargo tauri dev
   ```

   `cargo tauri dev` auto-runs the frontend dev server (`vite dev` on port 5173) and
   embeds it in the Tauri window.

> Note: `src-tauri` is a workspace member, so `cargo test --workspace` compiles it and
> therefore requires the WebKitGTK dev libraries on the build host. The `crates/*`
> headless engine needs none of them.

## Build Commands

All Rust commands run from the repo root. Frontend commands run via `npm --prefix frontend`.

### Rust (engine + shell)

| Command | Description |
|---------|-------------|
| `cargo test --workspace --locked` | Full test suite — what CI runs. Compiles `src-tauri`, so needs WebKitGTK 4.1 dev libs. `--locked` fails on a stale `Cargo.lock`. |
| `cargo test -p nextwist-<crate>` | Test a single crate (e.g. `nextwist-deploy`). The headless engine crates need no webview. |
| `cargo test -p nextwist-deploy --test crash_recovery` | Run one integration-test file. |
| `cargo test -p nextwist-deploy recover` | Run tests matching a substring. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint. CI fails on any warning. |
| `cargo deny check advisories bans licenses sources` | Supply-chain gate (see Code Style). |
| `cargo fmt` | Format Rust code with the pinned `rustfmt` component. |

### Tauri app

| Command | Description |
|---------|-------------|
| `cargo tauri dev` | Run the full desktop app (auto-starts the frontend dev server). |
| `cargo tauri build --bundles appimage` | Build the AppImage (the v1 distribution channel). |

### Frontend (SvelteKit static SPA)

| Command | Description |
|---------|-------------|
| `npm --prefix frontend ci` | Install frontend dependencies from the lockfile. |
| `npm --prefix frontend run build` | Build the static SPA to `frontend/build` (Tauri's `frontendDist`). |
| `npm --prefix frontend run check` | Type-check with `svelte-check`. |
| `npm --prefix frontend run dev` | Run the Vite dev server standalone (port 5173). |

## Code Style

NexTwist enforces style and supply-chain rules through tooling. There is no
`.editorconfig`, Prettier, or Biome configuration; formatting is handled by the
Rust and SvelteKit toolchains directly.

- **Rust formatting** — `rustfmt` (default profile, no `rustfmt.toml`), available via the
  `rust-toolchain.toml` `rustfmt` component. Run `cargo fmt`. Not currently enforced in CI.
- **Rust linting** — `clippy`, **enforced in CI** with `cargo clippy --workspace
  --all-targets -- -D warnings`. Warnings fail the build, so run this before pushing.
- **Frontend type-checking** — `svelte-check` via `npm --prefix frontend run check`.
- **Supply-chain policy** — `cargo deny check advisories bans licenses sources`, enforced
  in CI. `deny.toml` is load-bearing: the non-free UnRAR source (`unrar` / `unrar_sys`) is
  **banned** outright (RAR support shells out to a system `unrar`/`7z` binary instead), and
  only a permissive / GPL-compatible license set is allowed. Adding a dependency that trips
  `deny.toml` fails CI.

### Project conventions and guardrails

These rules are architectural invariants, not preferences. Honor them in every change:

- **The headless-engine boundary is sacred.** The entire safety-critical engine lives in
  `crates/*` as pure, headless Rust with **zero Tauri dependencies**, so it stays
  unit/property-testable in CI without a webview. Do **not** pull `tauri`, `reqwest`, or
  UI concerns into the `crates/*` engine. Command adapters in `src-tauri/src/commands/`
  must stay thin — they lock `AppState` and call into the engine crates, and contain no
  real logic.
- **Errors** — use `thiserror` enums in the engine crates; use `anyhow` only at the
  app / Tauri boundary.
- **TLS** — `reqwest` uses `rustls` only, never native-tls/OpenSSL. This keeps the
  AppImage self-contained.
- **Shared dependency versions** are pinned once in the root `[workspace.dependencies]`
  in `Cargo.toml`; member crates reference them via `<dep>.workspace = true`. Add or
  bump shared versions there, not in individual member manifests. Note `rusqlite` is
  pinned to `0.39` (not `0.40`) because `refinery 0.9.2` caps its rusqlite feature there.

## Branch Conventions

- The default / main branch is **`main`**.
- This project plans work as phases; the active development branches follow the
  pattern `gsd/phase-NN-<slug>` (e.g. `gsd/phase-05-appimage-distribution`). Planning
  artifacts live in `.planning/` and are kept out of the published product.
- **Commit messages follow Conventional Commits** — `feat:`, `fix:`, `docs:`, `test:`,
  `chore:`, `refactor:`, etc., with an optional scope (e.g. `test(05): ...`,
  `docs(phase-02): ...`).

## PR Process

There is no `PULL_REQUEST_TEMPLATE.md` or `CONTRIBUTING.md` in the repository yet, so
the process below reflects the conventions visible in the codebase and CI. Before
opening a pull request:

- Branch from `main` (use a descriptive branch name).
- Ensure the full safety suite passes locally: `cargo test --workspace --locked`.
- Ensure the lint gate passes: `cargo clippy --workspace --all-targets -- -D warnings`.
- Ensure the supply-chain gate passes: `cargo deny check advisories bans licenses sources`.
- Run `cargo fmt` so formatting is clean.
- Write commit messages in Conventional Commits style.
- Confirm CI is green. The `CI` workflow (`.github/workflows/ci.yml`) runs on every push
  and pull request: it installs the Tauri Linux deps, builds the frontend, then runs the
  test, clippy, and `cargo deny` gates. Releases are tag-triggered (`v*`) via
  `.github/workflows/release.yml`, which re-runs the same gates and builds the AppImage.

## Next Steps

- [TESTING.md](./TESTING.md) — how to run, write, and structure tests.
- [ARCHITECTURE.md](./ARCHITECTURE.md) — the crate layers, the crash-safety journal, and
  the Tauri shell boundary.
- [CONFIGURATION.md](./CONFIGURATION.md) — runtime configuration and environment.
