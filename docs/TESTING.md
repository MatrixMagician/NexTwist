<!-- generated-by: gsd-doc-writer -->
# Testing

NexTwist's test strategy follows the project's central architectural rule: the safety-critical engine lives in `crates/*` as pure, headless Rust with zero Tauri dependencies, so the entire engine is unit-, integration-, and property-testable in CI **without a webview or system GUI libraries**. The tests that prove the core safety guarantee — non-destructive, byte-for-byte reversible, conflict-aware deployment — are the most heavily exercised part of the suite.

## Test Framework and Setup

NexTwist uses Rust's built-in test harness (`cargo test`) across a virtual cargo workspace. There is no separate test runner to install. Supporting tooling pulled in as dev-dependencies (pinned once in the root `[workspace.dependencies]`):

| Tool | Version | Role |
|------|---------|------|
| Built-in `cargo test` harness | (toolchain) | Runs unit tests (`#[test]`), async tests (`#[tokio::test]`), and integration tests in `crates/*/tests/`. |
| `proptest` | 1.11 | Property/randomized testing — used for the randomized round-trip pristine deploy/purge test. |
| `tempfile` | 3.27 | Isolated temp directories for filesystem fixtures. |
| `blake3` | 1.8 | Content hashing behind the byte-for-byte pristine-tree assertion. |
| `mockito` | (workspace) | Local mock HTTP server for the NexusMods client tests (no real network). |
| `nextwist-testkit` | local crate (`crates/testkit`) | Shared fake-tree builders + pristine-tree assertion. See below. |

**Setup:**

- The headless engine crates (`crates/*`) need no GUI/system dependencies. A plain Rust toolchain is enough.
- `src-tauri` is a workspace member, so `cargo test --workspace` compiles it and therefore requires the WebKitGTK 4.1 dev libraries on the build host. CI installs `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, and `librsvg2-dev` (see `.github/workflows/ci.yml`).
- The toolchain is pinned to stable Rust (MSRV ≥ 1.89, set by `libloot`) via `rust-toolchain.toml`.

No build step is required before testing the engine crates. To run the full workspace suite (which compiles `src-tauri`), the frontend must be built first because Tauri embeds it — CI runs `npm --prefix frontend ci && npm --prefix frontend run build` before `cargo test --workspace`.

## Running Tests

All `cargo` commands run from the repo root.

```bash
# Full test suite — exactly what CI runs (the --locked flag pins Cargo.lock)
cargo test --workspace --locked

# One crate (crate names are nextwist-<name>, e.g. nextwist-deploy)
cargo test -p nextwist-deploy

# One integration-test file (the file stem under crates/<crate>/tests/)
cargo test -p nextwist-deploy --test crash_recovery

# Tests matching a substring (filters by test-function name)
cargo test -p nextwist-deploy recover
```

To run only the headless safety crates without compiling the Tauri shell, target the
specific crate(s) with `-p` rather than `--workspace` — this avoids needing the
WebKitGTK dev libraries on your machine:

```bash
cargo test -p nextwist-deploy -p nextwist-store -p nextwist-extract
```

There is no watch-mode script in this project; use `cargo watch -x test` if you have `cargo-watch` installed.

## Test Layers

### Unit tests

Inline `#[cfg(test)] mod tests` modules live alongside the code they cover. Two representative examples:

- `crates/testkit/src/lib.rs` — the pristine-assertion primitive is itself unit-tested (mutated/missing/orphan detection, empty-directory tracking, fake-tree builders).
- `src-tauri/src/commands/plugins.rs` — the WR-05 failure-ordering invariant (the `plugins.txt` file write must precede the DB persist so a libloot/IO failure never leaves the on-disk file and the DB disagreeing) is unit-tested in the synchronous core extracted from the Tauri command.

### Integration tests

Each engine crate has a `tests/` directory whose files compile as separate integration-test binaries. The most safety-critical live in `crates/deploy/tests/` (the deployment engine is the crown jewel):

| Test file | What it proves |
|-----------|----------------|
| `round_trip_pristine.rs` | **Property test (proptest).** For randomized game + mod file trees (pure-adds and overwrites of vanilla files): snapshot vanilla, deploy, purge, then assert the game tree is byte-for-byte identical to the vanilla snapshot with no orphans. |
| `crash_recovery.rs` | **The centerpiece (DEPLOY-06).** Simulates a kill mid-deploy via `deploy_with_abort` (journal `pending` rows + files placed, but intents not flipped to `done`), then opens a fresh store handle and calls `recover_on_launch` to assert the half-finished op replays safely. |
| `vanilla_restore.rs` | A mod replacing a vanilla file backs the original to the content-addressed store; purge restores the exact original bytes. Also asserts intent-before-act ordering (pending journal row before the manifest row). |
| `conflict_redeploy.rs` | The conflict slice's safety gate (CONF-03): deploying the user's deterministic conflict-winner set is pristine-reversible. |
| `profile_switch.rs` | Profile switching (PROF-02/03) reconciles through the journaled engine — full purge-to-pristine then a fresh deploy, so one profile's files never leak into another. Includes the **WR-02 failure-injection** case (a switch that fails after the purge step must leave no profile marked active). |
| `verify_drift.rs` | The verify/repair pass (DEPLOY-07) hash-diffs the manifest against the on-disk tree and classifies drift as `missing` / `changed` / `orphan`. |
| `method_ladder.rs` | The per-target method ladder (DEPLOY-05) selects the strongest applicable primitive and downgrades on `CrossesDevices`/EXDEV instead of failing; every method round-trips a deploy + remove. |
| `fs_probe.rs` | The per-target capability probe (ENV-04) reports same_device / reflink / hardlink_ok / casefold for a `(staging, game_data)` pair. |
| `casefold_normalize.rs` | Mixed-case mod paths are rewritten to match the game's canonical `Data/` casing (DEPLOY-08). |
| `collection_round_trip.rs` | A NexusMods Collection deploys pristine-reversibly with no network (COLL-04/05). |

Other crates carry their own integration suites:

- `crates/extract/tests/` — `extract_formats.rs` (zip/7z/tar formats) and `zip_slip_rejected.rs` (the Phase 1 security centerpiece: crafted malicious-archive rejection — zip-slip / symlink write-through defense).
- `crates/fomod/tests/corpus.rs` — the FOMOD fixture corpus under `crates/fomod/tests/fixtures/`, an executable contract for parse → condition → resolve.
- `crates/nexus/tests/` — `auth_mock.rs`, `client_mock.rs`, `collection_mock.rs`, `nxm_parse.rs`. The HTTP client tests are **mockito-backed** (a local mock server), so they exercise the NexusMods REST/auth request shapes with no real network calls.
- `crates/loadorder/tests/` — `libloot_spike.rs` and `plugins.rs` (plugin scan/classify, LOOT sort, asterisk-format `plugins.txt` round-trips). Headless via `crates/testkit`'s `fake_proton_prefix` fixture, which materializes the exact `drive_c/users/steamuser/AppData/Local/<game>/` path libloot targets — so `plugins.txt` round-trips are asserted in CI with no real Proton install.
- `crates/steam/tests/resolve_game.rs` — Steam install / Proton-prefix resolution.
- `src-tauri/tests/` — `download_stage.rs` (the NexusMods download flow's terminus reuses the same `extract::install_archive` pipeline) and `nxm_self_test.rs` (the `nxm://` handler self-test is non-fatal).

### Property tests

`round_trip_pristine.rs` uses `proptest` to generate randomized game and mod file trees, then drives the full deploy → purge cycle and asserts byte-for-byte pristine restoration. This is the strongest correctness signal for the reversibility guarantee because it explores many file-tree shapes rather than a few hand-picked cases.

### Failure-injection tests

Two named invariants (WR-02, WR-05) are proven by deliberately injecting failures and asserting the system lands in a safe, consistent state:

- **WR-02** (`crates/deploy/tests/profile_switch.rs`): a profile switch that fails after the purge step must clear the stale active flag, leaving no profile marked active and the deployment still purgeable to vanilla.
- **WR-05** (`src-tauri/src/commands/plugins.rs` unit tests): if the `plugins.txt` write fails, the DB `plugins` order is left untouched (the file write precedes the DB persist).

## The Pristine-Tree Assertion (`nextwist-testkit`)

The byte-for-byte pristine guarantee is enforced by a single, well-tested primitive in `crates/testkit/src/lib.rs`, used as a dev-dependency by the `deploy`, `steam`, and `extract` suites:

- `snapshot_tree(root)` — walks a directory tree and records every descendant: each regular file by its **blake3 content hash**, and each directory (including **empty** directories) by a reserved `DIR_SENTINEL` (`"<dir>"`) marker. Tracking directory shape is load-bearing — an orphan empty directory a purge fails to clean up is a real difference from vanilla and must be detected. Symlinks are not followed; a placed symlink hashes as the bytes it resolves to, matching how the game would read it.
- `assert_trees_identical(expected, actual)` — panics with an explicit, actionable diff classifying each offending path as **mutated** (different hash), **missing** (in expected, absent from actual), or **orphan** (in actual, not in expected). The verbose diff points straight at the offending file.
- `fake_game_tree(root, files)` / `fake_staged_mod(root, files)` — materialize fake vanilla-game and staged-mod trees from `(relpath, bytes)` pairs.
- `fake_proton_prefix(root, game_name, plugins_txt)` — builds the `drive_c/users/steamuser/AppData/Local/<game>/` tree libloot targets on Linux, optionally seeding `Plugins.txt`, so load-order tests run headlessly.

The canonical safety test shape is: snapshot vanilla → deploy a mod → purge → `assert_trees_identical(vanilla_snapshot, current_tree)`.

## Writing New Tests

- **Integration tests** go in `crates/<crate>/tests/<name>.rs`; each file is its own test binary. Run an individual one with `cargo test -p nextwist-<crate> --test <name>`.
- **Unit tests** go in an inline `#[cfg(test)] mod tests { ... }` block in the source file. Extract IO-ordering logic into a synchronous core (as `plugins.rs` does) so failure invariants are unit-testable without a webview.
- For anything that touches the filesystem, build fixtures with `tempfile::TempDir` plus the `nextwist-testkit` `fake_game_tree` / `fake_staged_mod` builders, and assert reversibility with `assert_trees_identical`.
- For HTTP/NexusMods behavior, use `mockito` to stand up a local mock server — never hit the real API in tests.
- For deployment safety, prefer a full snapshot → deploy → purge → pristine-assert round-trip over checking individual file operations.

## Coverage Requirements

No coverage tooling (`cargo-tarpaulin`, `cargo-llvm-cov`) or coverage threshold is configured in this repository. There is no enforced minimum-coverage gate in CI.

The de-facto coverage bar is qualitative and behavioral: every safety-critical path in `deploy` (round-trip pristine, crash recovery, vanilla restore, conflict redeploy, profile switch, verify/drift, method ladder) has a dedicated integration test, and the most important is randomized via proptest.

## CI Integration

The CI workflow is `.github/workflows/ci.yml` (job `test + deny (ubuntu)`), triggered on every push (all branches) and on pull requests.

The pipeline runs:

1. Install Tauri Linux build deps (WebKitGTK 4.1 and friends) so `src-tauri` compiles.
2. Install the stable Rust toolchain (with `clippy`).
3. Build the static SPA frontend (`npm --prefix frontend ci && npm --prefix frontend run build`).
4. **`cargo test --workspace --locked`** — the full test suite.
5. `cargo clippy --workspace --all-targets -- -D warnings` — lint gate (warnings fail the build).
6. `cargo deny check advisories bans licenses sources` — supply-chain gate (bans the non-free UnRAR source, enforces the license/advisory policy).

The release workflow (`.github/workflows/release.yml`, triggered on `v*` tags) re-runs the same test gate before building and publishing the AppImage.

## The In-Game / UAT Boundary

A subset of behavior cannot be verified headlessly because it requires a real OS desktop session, a built AppImage, and a real Steam Proton install running an actual Bethesda game. These are handled as **manual UAT (User Acceptance Testing)** items, documented per phase under `.planning/phases/<phase>/<n>-UAT.md` and `<n>-VALIDATION.md`.

What lands on the manual side of the boundary:

- True end-to-end `nxm://` deep-link registration and one-click "Mod Manager Download" — needs a real desktop session + a built AppImage. The headless test (`src-tauri/tests/nxm_self_test.rs`) pins only the self-test contract the startup hook depends on, not the actual OS registration.
- Deployment against a real Proton/Wine prefix on a real game (in-game load-order behavior, the game actually starting with mods applied).
- AppImage packaging/runtime validation on real hardware.

Where a property is provable headlessly it is — for example, `round_trip_pristine` runs deterministically on a tempdir in CI, and the project's phase validation re-runs it on the developer's real btrfs filesystem (the hardest filesystem case) as a manual validation step. The rule of thumb: the safety engine is proven in CI; the OS-integration and real-game surfaces are proven by documented manual UAT.

## Next Steps

- See [DEVELOPMENT.md](DEVELOPMENT.md) for build commands, code style, and the contribution workflow.
- See [ARCHITECTURE.md](ARCHITECTURE.md) for the crate layering and the crash-safety journal model the tests exercise.
