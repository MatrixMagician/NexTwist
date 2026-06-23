<!-- generated-by: gsd-doc-writer -->
# Getting Started

NexTwist is a Rust + Tauri v2 desktop app that brings safe, fully-reversible mod
management to **Linux** gamers running **Windows games via Steam Proton / Wine**.
v1.0 manages Bethesda Creation Engine games — **Skyrim Special Edition** (Steam
AppID `489830`) and **Fallout 4** (Steam AppID `377160`) — installed and run
through Steam Proton on Linux.

This guide covers two paths:

- **[A. Run the AppImage release](#a-run-the-appimage-release)** — for end users who
  just want to use the app.
- **[B. Build from source](#b-build-from-source)** — for developers and contributors.

---

## A. Run the AppImage release

### Prerequisites

- A **Linux desktop** with a graphical session.
- A system **WebKitGTK 4.1** runtime. The AppImage is built against the
  `ubuntu-22.04` glibc / WebKitGTK 4.1 floor for broad compatibility, and links
  against the system WebKitGTK at runtime. Most current desktop Linux
  distributions ship a compatible WebKitGTK 4.1 package.
- **Steam** installed, with Skyrim SE or Fallout 4 installed and launched at least
  once through Proton (so the game's Proton prefix exists under
  `steamapps/compatdata/<appid>/pfx`).

### Steps

The primary v1 distribution channel is an AppImage attached to each GitHub Release.

1. Download the latest `NexTwist_*.AppImage` from the Releases page:
   <https://github.com/MatrixMagician/NexTwist/releases>
2. Make it executable and run it:

   ```bash
   chmod +x NexTwist_*.AppImage
   ./NexTwist_*.AppImage
   ```

That's it — the AppImage is a single, self-contained, portable binary. There is no
installer and no separate dependency download (TLS, SQLite, and the safety engine
are statically linked).

### First run

On launch, NexTwist:

1. Runs **crash-recovery** for every managed game *before the UI is served*,
   replaying any deployment operation that a previous crash left half-finished
   (this is what keeps deployment reversible).
2. Auto-discovers your Steam install and any supported games already installed.

From the UI you then log into NexusMods (OAuth2, or an API-key paste as a
fallback), add a supported game, and install mods. NexTwist requires **no
configuration files or environment variables** to run — see
[CONFIGURATION.md](CONFIGURATION.md) for the small set of standard variables it
*optionally* honours and where it stores state.

---

## B. Build from source

NexTwist is a **virtual cargo workspace** — there is **no root `package.json`**.
The Rust engine builds with `cargo`; the embedded SvelteKit frontend builds with
`npm` scoped to the `frontend/` directory via `npm --prefix frontend`.

### Prerequisites

| Requirement | Version / Notes |
|-------------|-----------------|
| **Rust** | `>= 1.89` (2024 edition). Pinned via `rust-toolchain.toml` (channel `stable`); the floor is set by `libloot`. |
| **Node.js / npm** | Required to build the SvelteKit (Svelte 5) static SPA the Tauri shell embeds. |
| **Tauri Linux build libraries** | Needed to compile/run the desktop app and bundle the AppImage (full list below). |

Install the Tauri Linux build dependencies (Debian/Ubuntu package names, taken
from `.github/workflows/release.yml`):

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf \
  build-essential \
  curl wget file libssl-dev
```

> `patchelf` is only required for AppImage bundling (`cargo tauri build`). The CI
> test job omits it — see `.github/workflows/ci.yml`.

You will also need the Tauri CLI. The build/run commands below use `cargo tauri`,
which requires the `tauri-cli` (e.g. `cargo install tauri-cli` or run via
`cargo tauri` if already available on your toolchain).

### Installation steps

```bash
# 1. Clone the repository
git clone https://github.com/MatrixMagician/NexTwist.git
cd NexTwist

# 2. Build the embedded SvelteKit frontend (static SPA Tauri embeds into frontend/build)
npm --prefix frontend ci
npm --prefix frontend run build
```

### First run (dev)

Run the full desktop app with hot-reload (auto-runs the frontend dev server). This
needs the WebKitGTK 4.1 dev libs installed above:

```bash
cargo tauri dev
```

To produce the distributable AppImage:

```bash
cargo tauri build --bundles appimage
```

### Verify your setup without a webview

The entire safety-critical engine lives in `crates/*` as pure, headless Rust with
**zero Tauri dependencies**, so you can compile and test it without any webview or
system GUI libraries. To run the engine test suite:

```bash
# Full workspace test suite (what CI runs)
cargo test --workspace --locked

# A single crate (e.g. the reversible deployment engine)
cargo test -p nextwist-deploy
```

> Note: `src-tauri` is a workspace member, so `cargo test --workspace` *also*
> compiles the Tauri shell and therefore requires the WebKitGTK 4.1 dev libs on
> the build host. The `crates/*` headless engine alone needs none of them — test
> an individual engine crate with `cargo test -p nextwist-<name>` if you have not
> installed the GUI libraries.

---

## Common setup issues

- **`cargo tauri dev` fails with a WebKitGTK / pkg-config error.** The Tauri Linux
  build libraries are not installed. Install the apt packages listed under
  [Prerequisites](#prerequisites) (notably `libwebkit2gtk-4.1-dev` and
  `libgtk-3-dev`).

- **`cargo test --workspace` fails to compile `src-tauri`** while the `crates/*`
  engine builds fine. `src-tauri` is a workspace member that needs the WebKitGTK
  4.1 dev libs. Either install them, or scope your tests to an engine crate
  (`cargo test -p nextwist-deploy`).

- **`cargo build` / `cargo tauri build` fails on a stale `Cargo.lock`.** CI and the
  release build use `--locked`, which makes a stale lockfile a hard error. Run
  `cargo update` (or rebuild the lockfile) and commit the result, or drop
  `--locked` for a local build.

- **Wrong Rust version.** The workspace floor is Rust `>= 1.89` (2024 edition). If
  you see edition or `io::ErrorKind::CrossesDevices` errors, update your toolchain
  (`rustup update stable`); `rust-toolchain.toml` pins the `stable` channel.

- **The AppImage will not launch on an older distro.** The release is built against
  the `ubuntu-22.04` WebKitGTK 4.1 / glibc floor. A system older than that floor,
  or one missing the WebKitGTK 4.1 runtime package, cannot run it — install your
  distribution's WebKitGTK 4.1 package.

- **No supported game is detected.** Only Skyrim SE (`489830`) and Fallout 4
  (`377160`) are supported in v1, and the game must be installed via Steam and
  launched once under Proton so its prefix exists. If Steam auto-discovery does not
  find your install, you can add a game by its folder from the UI.

---

## Next steps

- **[README.md](../README.md)** — project overview, features, and workspace layout.
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — the headless-engine / thin-Tauri-shell
  design, the crate layers, and the crash-safety operation journal.
- **[CONFIGURATION.md](CONFIGURATION.md)** — the optional environment variables
  NexTwist honours and where it stores its SQLite database and credentials.
