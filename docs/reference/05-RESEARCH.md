# Phase 5: AppImage Distribution - Research

**Researched:** 2026-06-22
**Domain:** Linux desktop packaging (Tauri v2 AppImage), URL-scheme MIME registration, license-compliance auditing
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**nxm:// Handler Registration from AppImage**
- Rely on `tauri-plugin-deep-link`'s built-in `$APPIMAGE` handling for a stable `Exec=` path (appimagetool exports the `APPIMAGE` env var; the plugin writes the stable AppImage path into the `.desktop` file). No custom `.desktop` writer.
- Register the handler on first run automatically — keep the current `register_all()` call in the Tauri `setup` hook; registration is idempotent on relaunch.
- Add a lightweight handler self-test: a startup/diagnostic check that runs `xdg-mime query default x-scheme-handler/nxm` and surfaces a warning if the default is not NexTwist (satisfies the success-criterion "self-test passes").
- If `xdg-mime` / `update-desktop-database` are absent (minimal distros): keep the existing non-fatal warn-and-continue behavior (the app still opens) plus a one-line UI hint. Do not hard-fail startup.

**Build & Release Pipeline**
- Build the AppImage with `cargo tauri build --bundles appimage` (Tauri's built-in linuxdeploy path). The `appimage` bundle target is already configured in `src-tauri/tauri.conf.json`.
- Automate release in CI: add a tag-triggered GitHub Actions job that builds the AppImage and uploads it to a GitHub Release. (Existing `ci.yml` test+deny job is untouched; release is a separate workflow/job.)
- First release version: keep `0.1.0`, tag `v0.1.0`. `tauri.conf.json` `version` is the single source of truth.
- Target architecture: x86_64 only for v1. aarch64 deferred to v2.

**License-Compliance Audit (DIST-02)**
- Audit evidence = the existing `cargo-deny` gate (sources/licenses/bans/advisories) **plus** a one-time bundled-binary review of the actual built AppImage contents (enumerate bundled libraries/binaries; confirm no `unrar`/non-free code is shipped).
- Record the audit as a checked-in artifact: a short `DIST-AUDIT.md` capturing the cargo-deny result and the bundled-binary review findings.
- RAR support in the shipped binary: confirm and document zero bundled RAR code — `.rar` is handled by shelling out to a system `unrar`/`7z` (already enforced by the `unrar`/`unrar_sys` bans in `deny.toml`).
- CI gating shape: keep `cargo-deny` as the enforced per-push gate (already in `ci.yml`); run the bundled-binary review at release time (not per push).

### Claude's Discretion
- Exact placement/format of the self-test (startup log vs. settings panel surface), the UI-hint copy, the `DIST-AUDIT.md` layout, and the release-workflow file structure are at Claude's discretion, consistent with codebase conventions.

### Deferred Ideas (OUT OF SCOPE)
- Auto-updater / self-update mechanism — v2.
- Code-signing / notarization infrastructure — v2.
- Multi-arch (aarch64) AppImage builds — v2.
- Flatpak or other package formats — v2 (AppImage is the v1 channel per PROJECT.md).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DIST-01 | NexTwist is packaged and runnable as a Linux AppImage | Tauri v2 AppImage bundler via `tauri-action` (Build & Release Pipeline section); icon-size gap (32x32 → 128x128) flagged as a blocker; durable `nxm://` registration via `Env.appimage`/`APPIMAGE` (verified from plugin source); built-in `is_registered()` self-test. |
| DIST-02 | The distributed build passes a license-compliance audit (no non-free bundled code, e.g. UnRAR) | `cargo-deny` already gates sources/licenses/bans/advisories (incl. the GPL libloot family). Bundled-binary review technique via `--appimage-extract` + `ldd`/`find usr/lib`; UnRAR-absence evidence; `DIST-AUDIT.md` artifact contents. |
</phase_requirements>

## Summary

This is the final v1 packaging phase. All three work areas are well-supported by the existing codebase and current tooling; the locked decisions are sound and implementable as written. The single most important discovery is that **`tauri-plugin-deep-link` already does everything the decisions require — the durable-Exec mechanism and the self-test are both built into the installed 2.4.9 source.** When `APPIMAGE` is set, `register()` writes the AppImage's stable path as `Exec=`, and the plugin ships an `is_registered()` method that runs the exact `xdg-mime query default x-scheme-handler/nxm` check the decisions describe. No custom `.desktop` writer or hand-rolled self-test is needed — the phase should *call the plugin's existing API*, not reimplement it.

The build/release pipeline is standard `tauri-apps/tauri-action` territory: a tag-triggered workflow on `ubuntu-22.04` with `args: --bundles appimage`, `permissions: contents: write`, and the WebKitGTK/appindicator/rsvg/patchelf apt set (a near-superset of the existing `ci.yml` deps). The license audit is mostly pre-positioned: `cargo-deny` already enforces the source/license/ban policy per-push, and the bundled-binary review is a short, reproducible `--appimage-extract` + `ldd`/`find usr/lib` enumeration recorded in `DIST-AUDIT.md`.

**One concrete blocker surfaced:** `src-tauri/icons/icon.png` is **32×32**, but linuxdeploy/Tauri AppImage bundling expects a **128×128** (or larger) icon. This must be regenerated (via `cargo tauri icon`) before a clean AppImage build, or bundling may fail / ship a degraded desktop icon. This is the only must-fix gap.

**Primary recommendation:** Call the plugin's existing `register_all()` + `is_registered("nxm")` API (do not hand-roll). Regenerate icons to ≥128×128. Add a separate tag-triggered `release.yml` using `tauri-action@v0` with `--bundles appimage` on `ubuntu-22.04`. Run a one-time `--appimage-extract` + `ldd` review at release time and record it in `DIST-AUDIT.md`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| nxm:// MIME registration | Tauri shell (`src-tauri/`) | OS (`xdg-mime`/`update-desktop-database`) | OS-integration is the shell's job; the headless engine has zero Tauri/OS-handler deps. Already wired in `lib.rs`. |
| Handler self-test (`is_registered`) | Tauri shell (`src-tauri/`) | OS (`xdg-mime`) | Shells out to the system `xdg-mime`; surfaces result to the UI. Pure shell concern. |
| AppImage build/bundle | Build tooling (Tauri CLI / linuxdeploy) | CI (GitHub Actions) | Packaging, not application logic. Driven by `tauri.conf.json` bundle config. |
| Tag-triggered release | CI (GitHub Actions) | — | A new `.github/workflows/release.yml`; engine untouched. |
| License gate (per-push) | CI (`cargo-deny`) | `deny.toml` policy | Already enforced in `ci.yml`. |
| Bundled-binary audit (release-time) | Build/release tooling + docs | `DIST-AUDIT.md` | Manual/scripted enumeration of the built artifact; recorded as a checked-in doc. |

## Standard Stack

### Core
| Library / Tool | Version | Purpose | Why Standard |
|----------------|---------|---------|--------------|
| `tauri` | 2.11.3 (installed) | App shell + bundler config | Already the project's shell. [VERIFIED: Cargo.lock] |
| `tauri-plugin-deep-link` | 2.4.9 (installed) | nxm:// MIME registration + self-test API | Already a dependency; provides `register_all()` AND `is_registered()`. [VERIFIED: Cargo.lock + plugin source] |
| `tauri-plugin-single-instance` | 2.4 (`deep-link` feature) | Forward 2nd nxm:// invocation to live instance | Already wired BEFORE deep-link (load-bearing order). [VERIFIED: src-tauri/Cargo.toml] |
| `tauri-apps/tauri-action` | `@v0` (latest stable, action v0.6.2) | CI build + GitHub Release upload | The official Tauri CI action; handles bundle + release in one step. [CITED: github.com/tauri-apps/tauri-action] |
| `cargo-deny` | 0.19.x (CI-installed via taiki-e/install-action) | License/ban/advisory/source gate | Already the load-bearing supply-chain gate in `ci.yml`. [VERIFIED: ci.yml + deny.toml] |

### Supporting (build-time, auto-provisioned)
| Tool | Version | Purpose | When to Use |
|------|---------|---------|-------------|
| `linuxdeploy` + AppImage plugins | bundled by Tauri CLI | Assembles the AppDir → AppImage | Auto-downloaded by `cargo tauri build --bundles appimage`; not a pre-install. [CITED: v2.tauri.app/distribute/appimage] |
| `patchelf` | apt (CI) | RPATH fixups during bundling | Required on the CI runner for linuxdeploy. [CITED: tauri-action README] |
| `cargo tauri icon` | Tauri CLI | Regenerate the full icon set from one source PNG | Needed to fix the 32×32 icon gap (see Common Pitfalls). [CITED: v2.tauri.app/develop/icons] |

**No new Rust crate dependencies are required for this phase.** All registration/self-test capability already exists in the installed `tauri-plugin-deep-link`.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tauri-action@v0` | Hand-written `cargo tauri build` + `softprops/action-gh-release` upload | More control, more YAML to maintain; tauri-action already wraps bundle+upload and is the documented path. Use the manual path only if tauri-action proves too opaque. |
| Plugin's `is_registered()` | Hand-rolled `Command::new("xdg-mime")...query default` | The plugin method does exactly this and matches the desktop-file naming internally — reimplementing risks a filename mismatch. Prefer the API. |
| `cargo-deny` + manual `--appimage-extract` review | `cargo sbom` / `cargo auditable` SBOM | SBOM adds ceremony with little marginal value for a single-binary GPL Linux app whose deps are already gated by cargo-deny. **Not recommended** — see State of the Art. |

**Action version pinning:** Use `tauri-apps/tauri-action@v0` (or pin to the current release tag `action-v0.6.2` for reproducibility). Both `@v0` and `@v1` refs are referenced in Tauri docs; `@v0` is the widely-used current line. [CITED: github.com/tauri-apps/tauri-action] — confirm the exact current ref at plan time.

**Version verification:**
```bash
# already installed — confirmed from Cargo.lock this session:
#   tauri 2.11.3, tauri-plugin-deep-link 2.4.9, tauri-plugin-single-instance 2.4 (deep-link)
# CI tools (installed in workflow, not pinned as crates):
#   cargo-deny via taiki-e/install-action@v2
```

## Package Legitimacy Audit

> No new external crate dependencies are introduced in this phase. The only new "dependency" is a GitHub Action (`tauri-apps/tauri-action`), which is the first-party Tauri org action — not a crates.io package.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `tauri-plugin-deep-link` | crates.io | mature (2.4.9) | high | github.com/tauri-apps/plugins-workspace | OK | Already a dependency — no change |
| `tauri-apps/tauri-action` | GitHub Actions (not crates) | mature | first-party Tauri org | github.com/tauri-apps/tauri-action | OK | Approved (pin to `@v0` or a release tag) |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

No `cargo add` is expected in this phase. The legitimacy gate over crates is satisfied by the existing `cargo-deny` policy.

## Architecture Patterns

### System Architecture Diagram

```
                          ┌─────────────────────────── BUILD / RELEASE (CI) ───────────────────────────┐
  git tag v0.1.0  ───────►│  on: push tags v*                                                          │
                          │    ubuntu-22.04 runner                                                     │
                          │      apt: webkit2gtk-4.1 + appindicator3 + rsvg2 + patchelf                │
                          │      npm build frontend (static SPA → frontend/build)                      │
                          │      tauri-action@v0  args: --bundles appimage                             │
                          │        └─► cargo tauri build → linuxdeploy → NexTwist_0.1.0_amd64.AppImage │
                          │      upload asset ──────────────────────────────────► GitHub Release       │
                          └────────────────────────────────────────────────────────────────────────────┘
                                                          │ (developer downloads .AppImage)
                                                          ▼
  USER RUNTIME:   ./NexTwist_0.1.0_amd64.AppImage
                          │ AppImage runtime mounts squashfs at /tmp/.mount_XXXX  (ephemeral)
                          │ runtime exports  APPIMAGE = /abs/path/to/NexTwist_0.1.0_amd64.AppImage (STABLE)
                          ▼
              ┌──────────────── src-tauri/lib.rs run() ────────────────┐
              │ single-instance plugin (registered FIRST)              │
              │ deep-link plugin                                       │
              │ setup hook:                                            │
              │   recover_all_on_launch(...)        (DEPLOY-06)        │
              │   deep_link().register_all()  ──► reads Env.appimage   │
              │        │                            (= $APPIMAGE)      │
              │        ▼                                               │
              │   writes  ~/.local/share/applications/                 │
              │           nextwist-handler.desktop                     │
              │           Exec="<$APPIMAGE path>" %u   (DURABLE)        │
              │        ▼                                               │
              │   update-desktop-database ; xdg-mime default ...       │
              │   [NEW] is_registered("nxm")  ──► self-test warn       │
              │        │                                               │
              │   on_open_url ──► commands::nexus::handle_nxm_url      │
              └────────────────────────────────────────────────────────┘
                          ▲
   browser nxm:// click ──┘  (2nd instance → forwarded to live instance via single-instance)
```

### Recommended Project Structure
```
src-tauri/
├── tauri.conf.json      # bundle.targets ["appimage"] (exists); add bundle.linux.appimage if needed
├── icons/
│   ├── icon.png         # REGENERATE: currently 32x32 → need ≥128x128
│   ├── 128x128.png      # (cargo tauri icon output)
│   ├── 128x128@2x.png
│   └── 32x32.png
└── src/lib.rs           # add is_registered("nxm") self-test in setup hook
.github/workflows/
├── ci.yml               # UNCHANGED (test + clippy + cargo-deny per push)
└── release.yml          # NEW: tag-triggered AppImage build + GitHub Release
DIST-AUDIT.md            # NEW: checked-in license + bundled-binary audit record
```

### Pattern 1: Durable Exec via the plugin's built-in APPIMAGE handling
**What:** `register()` reads `self.app.env().appimage`. Tauri sets `Env.appimage = std::env::var_os("APPIMAGE")`. When non-empty, `Exec=` is the AppImage's own absolute path; otherwise it falls back to `current_exe()` (which under AppImage is the ephemeral `/tmp/.mount_*` path — the bug we avoid).
**When to use:** Always, from the AppImage. The AppImage type-2 runtime exports `APPIMAGE` automatically — no AppRun edit needed.
**Example (verified from installed plugin source, tauri-plugin-deep-link 2.4.9):**
```rust
// Source: ~/.cargo/.../tauri-plugin-deep-link-2.4.9/src/lib.rs  (Linux register())
let appimage = self.app.env().appimage;                       // = $APPIMAGE (OsString) or None
let exec = appimage.clone()
    .unwrap_or_else(|| bin.into_os_string())                  // fallback to current_exe()
    .to_string_lossy().to_string();
let qualified_exec = format!("\"{}\" %u", exec);              // Exec="<stable path>" %u
// writes  $XDG_DATA_HOME/applications/<binary>-handler.desktop  →  "nextwist-handler.desktop"
// then:   update-desktop-database <dir> ; xdg-mime default <file> x-scheme-handler/nxm
```
```rust
// Tauri populates the field — Source: tauri-utils-2.9.3/src/lib.rs Env::default()
appimage: std::env::var_os("APPIMAGE"),
```

### Pattern 2: Self-test via the plugin's own `is_registered`
**What:** The plugin ships `is_registered("nxm")` which runs `xdg-mime query default x-scheme-handler/nxm` and returns `true` iff the output contains `nextwist-handler.desktop`. This is exactly the decided self-test — call it instead of hand-rolling.
**Example (verified from plugin source):**
```rust
// Source: tauri-plugin-deep-link-2.4.9/src/lib.rs  is_registered()
use tauri_plugin_deep_link::DeepLinkExt;
match app.deep_link().is_registered("nxm") {
    Ok(true)  => tracing::info!("nxm:// self-test passed (NexTwist is the default handler)"),
    Ok(false) => tracing::warn!("nxm:// self-test: NexTwist is NOT the default x-scheme-handler/nxm"),
    Err(e)    => tracing::warn!(error = %e, "nxm:// self-test could not run (xdg-mime missing?)"),
}
```
**Note:** The desktop-file name is derived from the **binary file name** (`nextwist`) → `nextwist-handler.desktop`, NOT from the `com.nextwist.app` identifier and NOT from `productName`. Any UI copy or audit doc referencing the file should say `nextwist-handler.desktop`. The MIME type string is `x-scheme-handler/nxm`. [VERIFIED: plugin source]

### Pattern 3: Tag-triggered release workflow (separate from ci.yml)
**Example (skeleton — confirm action ref at plan time):**
```yaml
# Source: github.com/tauri-apps/tauri-action + v2.tauri.app/distribute/pipelines/github
name: release
on:
  push:
    tags: ["v*"]
permissions:
  contents: write            # required for the action to create/upload the Release
jobs:
  appimage:
    runs-on: ubuntu-22.04    # 22.04 for glibc/WebKitGTK 4.1 compatibility floor
    steps:
      - uses: actions/checkout@v4
      - name: Install Tauri Linux build deps
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
            libayatana-appindicator3-dev librsvg2-dev patchelf \
            build-essential curl wget file libssl-dev
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Build frontend
        run: npm --prefix frontend ci && npm --prefix frontend run build
      - uses: tauri-apps/tauri-action@v0
        env: { GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }} }
        with:
          tagName: ${{ github.ref_name }}     # the pushed v0.1.0 tag
          releaseName: "NexTwist ${{ github.ref_name }}"
          projectPath: src-tauri
          args: --bundles appimage             # AppImage only, x86_64 (native runner arch)
```

### Anti-Patterns to Avoid
- **Hand-writing the `.desktop` file or a custom xdg-mime call.** The plugin already does this correctly (including the APPIMAGE path). Locked decision: "No custom `.desktop` writer." Reimplementing risks the ephemeral `/tmp/.mount_*` Exec bug.
- **Registering deep-link before single-instance.** Already correct in `lib.rs`; do not reorder — a forwarded `nxm://` URL would be lost.
- **Hard-failing startup when `xdg-mime` is absent.** Locked decision: warn-and-continue. The `register_all()` and `is_registered()` calls already return `Result`; keep them non-fatal.
- **Building the AppImage on `ubuntu-latest` (24.04) for distribution.** Newer glibc raises the runtime floor and can break on older user systems. Use `ubuntu-22.04`.
- **Putting any packaging logic into `crates/*`.** Packaging lives in `src-tauri/`, config, CI, and docs only — honor the headless-engine boundary.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Durable Exec path under AppImage | Custom AppRun/.desktop writer reading `$APPIMAGE` yourself | `register_all()` (reads `Env.appimage`) | Plugin already implements the exact mechanism; locked decision forbids a custom writer. |
| nxm:// registration self-test | Manual `Command::new("xdg-mime").args(["query","default",...])` parsing | `app.deep_link().is_registered("nxm")` | Plugin method matches its own desktop-file naming internally; hand-rolling risks a filename mismatch. |
| AppImage assembly | Manual AppDir + appimagetool invocation | `cargo tauri build --bundles appimage` (linuxdeploy) | Tauri's bundler handles AppDir layout, library bundling, RPATH, and `.desktop`/icon placement. |
| Release upload | Custom `gh release upload` scripting | `tauri-apps/tauri-action` | One step does bundle + create-release + upload. |
| License enforcement | A bespoke license scanner | `cargo-deny` (already configured) | `deny.toml` already gates licenses/bans/sources/advisories incl. the UnRAR ban. |

**Key insight:** This phase is almost entirely *configuration + a few API calls into existing infrastructure*. The largest risk is reimplementing what the plugin already provides.

## Common Pitfalls

### Pitfall 1: Icon is 32×32 — linuxdeploy expects ≥128×128 (BLOCKER)
**What goes wrong:** `src-tauri/icons/icon.png` is currently **32×32 RGBA** (verified via `file` this session). linuxdeploy/AppImage tooling expects a 128×128 (or 256×256) icon; a too-small or missing standard icon can fail bundling or ship a degraded/blank desktop icon.
**Why it happens:** The placeholder icon was never regenerated for distribution.
**How to avoid:** Run `cargo tauri icon path/to/source-1024.png` (Tauri CLI) to generate the full standard icon set (32, 128, 128@2x, …) into `src-tauri/icons/`, and ensure `bundle.icon` lists a ≥128×128 PNG. Verify with `file src-tauri/icons/128x128.png`.
**Warning signs:** `failed to run linuxdeploy` during bundling; blank/pixelated icon in the desktop menu. [VERIFIED: `file` output + CITED: tauri-apps/tauri#14796, #15106]

### Pitfall 2: APPIMAGE not exported → ephemeral Exec path
**What goes wrong:** If `register()` runs in a context where `$APPIMAGE` is unset, `Exec=` falls back to `current_exe()` = the `/tmp/.mount_XXXX/...` mount path, which is deleted on exit. The handler then points at a dead path after the next launch.
**Why it happens:** Running the *unpacked* binary, or a wrapper that strips env, or `--appimage-extract-and-run` in some setups.
**How to avoid:** Nothing extra is needed for the normal AppImage launch — the type-2 runtime exports `APPIMAGE` to the absolute AppImage path automatically. Just confirm during UAT that, when launched from the `.AppImage`, the written `nextwist-handler.desktop` `Exec=` line contains the `.AppImage` path (not `/tmp/.mount_`). [VERIFIED: plugin source + CITED: docs.appimage.org/packaging-guide/environment-variables]
**Warning signs:** `Exec=` in `~/.local/share/applications/nextwist-handler.desktop` contains `/tmp/.mount_`.

### Pitfall 3: glibc / WebKitGTK floor from building on too-new Ubuntu
**What goes wrong:** Building on `ubuntu-latest` (24.04) links against a newer glibc and WebKitGTK, raising the minimum the user's host must provide; older distros fail at runtime.
**How to avoid:** Build the release on `ubuntu-22.04` (provides WebKitGTK 4.1 from standard repos). Document the WebKitGTK 4.1 runtime requirement (the AppImage does NOT bundle WebKitGTK — it uses the host's). [CITED: v2.tauri.app/distribute/appimage]
**Warning signs:** Users on older distros report `symbol ... GLIBC_2.3x not found` or missing-webkit errors.

### Pitfall 4: AppImage FUSE in CI
**What goes wrong:** GitHub-hosted runners may lack FUSE; some AppImage tooling needs `--appimage-extract-and-run`.
**How to avoid:** `ubuntu-22.04` GitHub runners support the standard tauri-action AppImage build out of the box. If a FUSE error appears, set `APPIMAGE_EXTRACT_AND_RUN=1` in the job env. [CITED: tauri-apps/tauri#10388]
**Warning signs:** `dlopen(): error loading libfuse.so.2` in the bundling step.

### Pitfall 5: Release token permissions
**What goes wrong:** `tauri-action` fails to create/upload the Release with a 403.
**How to avoid:** Add `permissions: contents: write` to the workflow (or job). The default `GITHUB_TOKEN` is read-only. [CITED: v2.tauri.app/distribute/pipelines/github]

## Runtime State Inventory

> This is a packaging phase, not a rename/refactor. There is no stored-data migration. The one runtime-state concern is the OS-registered MIME handler, listed below for completeness.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None — no DB keys/IDs change in this phase. | None |
| Live service config | None — no external service config. | None |
| OS-registered state | `~/.local/share/applications/nextwist-handler.desktop` (written by `register_all()`) + the `x-scheme-handler/nxm` default in `mimeapps.list`. Re-registered idempotently on each launch; `Exec=` updates if the AppImage path changes. | None beyond the existing idempotent re-register; verify Exec points at the `.AppImage`. |
| Secrets/env vars | NexusMods token in OS keyring (Secret Service) — unchanged by packaging. `$APPIMAGE` is read at runtime by Tauri, not stored. | None |
| Build artifacts | New `NexTwist_0.1.0_amd64.AppImage` produced by the bundler; uploaded to a GitHub Release. Not checked into git. | None (artifact lives in the Release) |

## Code Examples

### Add the self-test in the existing setup hook
```rust
// src-tauri/src/lib.rs — inside the existing #[cfg(any(windows, target_os = "linux"))] block,
// AFTER the register_all() call. Source pattern: tauri-plugin-deep-link 2.4.9 is_registered().
use tauri_plugin_deep_link::DeepLinkExt;
if let Err(e) = app.deep_link().register_all() {
    tracing::warn!(error = %e, "nxm:// deep-link registration failed (xdg-mime/update-desktop-database missing?)");
}
// Phase-5 self-test (DIST-01 "self-test passes"):
match app.deep_link().is_registered("nxm") {
    Ok(true)  => tracing::info!("nxm:// handler self-test: PASS"),
    Ok(false) => tracing::warn!("nxm:// handler self-test: NexTwist is not the default handler"),
    Err(e)    => tracing::warn!(error = %e, "nxm:// handler self-test: could not query xdg-mime"),
}
```

### Bundled-binary audit (release-time, reproducible)
```bash
# Source: docs.appimage.org + standard ldd workflow. Run on the built artifact.
APP=NexTwist_0.1.0_amd64.AppImage
./"$APP" --appimage-extract                 # → squashfs-root/
# 1. The shipped binary's dynamic deps (proves rustls, not OpenSSL, for TLS):
ldd squashfs-root/usr/bin/nextwist
# 2. Every bundled shared library:
find squashfs-root/usr/lib -name '*.so*' | sort
# 3. Prove NO non-free RAR code is bundled (DIST-02 / STAGE-03):
find squashfs-root -iname '*unrar*' -o -iname '*libunrar*'   # expect: no output
grep -rIl --binary-files=text -e 'UnRAR' squashfs-root/usr/bin/nextwist || echo "no UnRAR string"
# 4. Confirm WebKitGTK is NOT bundled (uses host) — expect no libwebkit2gtk in usr/lib.
find squashfs-root/usr/lib -iname '*webkit*'                 # expect: no output
```

### License gate (already in ci.yml; re-run for the audit record)
```bash
cargo deny check advisories bans licenses sources
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Hand-written AppRun/.desktop for scheme handlers | `tauri-plugin-deep-link` reads `Env.appimage` and writes the durable Exec | Tauri v2 | No custom writer needed — locked decision aligns. |
| Build releases on `ubuntu-latest` | Pin `ubuntu-22.04` for the glibc/WebKitGTK floor | Tauri v2 docs guidance | Wider runtime compatibility. |
| SBOM (`cargo sbom`/`cargo auditable`) for compliance | `cargo-deny` source/license gate + `--appimage-extract` review | — | For a single-binary GPL Linux app, cargo-deny + a bundled-`.so` review is sufficient, standard evidence. SBOM adds ceremony with little marginal value here — **not recommended for v1**. |

**Deprecated/outdated:**
- `tauri-action@v0` vs `@v1`: docs reference both refs; `@v0` (release line `action-v0.6.2`) is the broadly-used current line. Confirm the exact ref at plan time. [CITED: tauri-apps/tauri-action]

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `tauri-action@v0` is the correct current ref for AppImage-only x86_64 releases | Standard Stack / Pattern 3 | Low — if the ref changed, the workflow fails fast at the action step; confirm at plan time. |
| A2 | Building on `ubuntu-22.04` yields a binary that runs on the project's target Linux/Proton-gamer distros | Pitfalls 3 | Medium — the WebKitGTK 4.1 floor is a real host requirement; document the minimum and verify on real hardware UAT. |
| A3 | The AppImage type-2 runtime exports `APPIMAGE` for a normally-launched `.AppImage` (no AppRun edit) | Pattern 1 / Pitfall 2 | Low — confirmed by AppImage docs + plugin reads it; verify during UAT by inspecting the written `Exec=`. |

**Note:** Claims about the plugin's registration mechanics, the self-test API, the desktop-file name (`nextwist-handler.desktop`), and the MIME string (`x-scheme-handler/nxm`) are `[VERIFIED]` from the installed plugin source and Tauri source this session — not assumed.

## Open Questions

1. **Exact `tauri-action` ref (`@v0` vs a pinned `action-v0.x.y` SHA/tag).**
   - What we know: `@v0` is the current line; pinning to a release tag is best practice for reproducibility.
   - What's unclear: the precise latest tag at execution time.
   - Recommendation: pin to the current release tag found at plan time; fall back to `@v0`.

2. **Does the project want the release workflow to also run `cargo deny` as a release gate, or rely solely on the per-push `ci.yml` gate?**
   - What we know: Decision says keep cargo-deny per-push and run the bundled-binary review at release time.
   - Recommendation: have `release.yml` re-run `cargo deny check` once and capture its output into the `DIST-AUDIT.md` evidence, so the audit record is reproducible from the release run.

## Environment Availability

| Dependency | Required By | Available (dev host) | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `xdg-mime` | nxm:// registration + self-test | ✓ | 1.2.1 | warn-and-continue (locked decision) |
| `update-desktop-database` | nxm:// registration | ✓ | 0.28 | warn-and-continue |
| `ldd` | bundled-binary audit | ✓ | glibc 2.43 | — |
| `file` | icon-size + binary inspection | ✓ | 5.46 | — |
| `cargo tauri` (Tauri CLI) | local AppImage build / `tauri icon` | ✗ | — | Install `cargo install tauri-cli` or rely on CI; not required for code work |
| `cargo-deny` | local license gate | ✗ | — | CI installs it (taiki-e/install-action); local `cargo install cargo-deny` optional |
| `linuxdeploy` / `appimagetool` | AppImage assembly | ✗ | — | Auto-downloaded by Tauri's bundler at build time |
| `patchelf` | AppImage RPATH fixups | ✗ | — | apt-install on the runner (in `release.yml`) |

**Missing dependencies with no fallback:** none (all build tooling is CI-provisioned or bundler-provisioned).
**Missing dependencies with fallback:** `cargo tauri`, `cargo-deny`, `patchelf`, `linuxdeploy` — all provided in CI; the actual AppImage build is expected to happen in (or be reproduced from) the `release.yml` runner, not the dev host.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` (workspace) + manual/real-hardware UAT for the AppImage |
| Config file | none (cargo workspace) |
| Quick run command | `cargo test -p nextwist --locked` |
| Full suite command | `cargo test --workspace --locked` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DIST-01 | AppImage builds with `--bundles appimage` | CI / manual | `cargo tauri build --bundles appimage` (in `release.yml`) | ❌ Wave 0 (release.yml) |
| DIST-01 | Icon ≥128×128 present | unit-ish | `test -f src-tauri/icons/128x128.png && file src-tauri/icons/128x128.png \| grep -q '128 x 128'` | ❌ Wave 0 (regen icons) |
| DIST-01 | `is_registered("nxm")` self-test wired in setup | unit (shell crate) | `cargo test -p nextwist` (assert the self-test call path compiles + non-fatal) | ⚠️ self-test is OS-shelling; assert wiring/non-fatal, true E2E is manual UAT |
| DIST-01 | Launched AppImage writes a durable `Exec=` (not `/tmp/.mount_`) | manual UAT | inspect `~/.local/share/applications/nextwist-handler.desktop` after first run | manual-only (needs a real AppImage launch) |
| DIST-01 | `nxm://` click from browser routes to live instance | manual UAT | click a Nexus "Mod Manager Download" with the AppImage running | manual-only (real hardware, per NEXUS-04/NXM-01 precedent) |
| DIST-02 | cargo-deny passes (licenses/bans/sources/advisories) | CI | `cargo deny check advisories bans licenses sources` | ✅ exists (ci.yml) |
| DIST-02 | No UnRAR / non-free `.so` bundled in the AppImage | release-time audit | `--appimage-extract` + `find -iname '*unrar*'` (expect empty) | ❌ Wave 0 (audit script + DIST-AUDIT.md) |
| DIST-02 | `.rar` shells out to system unrar/7z (no bundled RAR code) | unit (existing) + audit | existing extract tests + the bundled-binary review | ✅ extract tests exist; audit ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p nextwist --locked` (shell crate compiles + self-test wiring)
- **Per wave merge:** `cargo test --workspace --locked` + `cargo deny check`
- **Phase gate:** full suite green + `release.yml` produces an AppImage + `DIST-AUDIT.md` recorded; real-hardware UAT for the durable-Exec + nxm:// click.

### Wave 0 Gaps
- [ ] `src-tauri/icons/128x128.png` (+ full icon set) — regenerate via `cargo tauri icon`; covers DIST-01 icon gap.
- [ ] `.github/workflows/release.yml` — tag-triggered AppImage build + upload; covers DIST-01.
- [ ] `DIST-AUDIT.md` — checked-in audit record (cargo-deny result + bundled-binary review); covers DIST-02.
- [ ] A small audit helper (script or documented commands) for `--appimage-extract` + `ldd`/`find usr/lib` enumeration.
- [ ] Self-test wiring in `src-tauri/src/lib.rs` (call `is_registered("nxm")`, non-fatal).

## Security Domain

> `security_enforcement: true`, ASVS level 1. This is a packaging phase; most ASVS categories do not apply. The relevant concerns are supply-chain integrity and not shipping non-free/vulnerable code.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | (auth is Phase 3, unchanged) |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | partial | `nxm://` parsing is in `nexus::NxmLink::parse` (existing); this phase only registers the handler, adds no new parsing. URL is never logged (V7, preserved). |
| V6 Cryptography | yes | reqwest stays rustls-only (no OpenSSL/native-tls) — the bundled-binary audit confirms no system-OpenSSL TLS path. |
| V14 (Build/Dependency) | yes | `cargo-deny` (bans/advisories/licenses/sources) gates the supply chain per-push; release-time bundled-binary review confirms the artifact. |

### Known Threat Patterns for {Tauri AppImage distribution}

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Shipping non-free/GPL-incompatible code (UnRAR) | Information disclosure / legal | `unrar`/`unrar_sys` banned in `deny.toml`; `.rar` shells out; bundled-binary review proves absence. |
| Vulnerable transitive dep in the shipped binary | Tampering | `cargo deny check advisories` (yanked + vuln = fail). |
| Dynamic OpenSSL pulled in, breaking the self-contained/rustls guarantee | Tampering | rustls-only reqwest; `ldd` review confirms no app-path libssl/libcrypto. |
| Stale/ephemeral `Exec=` hijack surface | Tampering | Durable `$APPIMAGE` path via the plugin; self-test verifies the registered default. |
| Unsigned artifact (no provenance) | Spoofing | Out of scope for v1 (code-signing deferred to v2); GitHub Release over HTTPS is the v1 distribution channel. |

## Sources

### Primary (HIGH confidence)
- `tauri-plugin-deep-link` 2.4.9 installed source (`.../src/lib.rs`) — Linux `register()`, `register_all()`, `is_registered()`, `template.desktop` — read directly this session.
- `tauri-utils` 2.9.3 installed source (`.../src/lib.rs` `Env::default`) — `appimage: std::env::var_os("APPIMAGE")`.
- `Cargo.lock` — tauri 2.11.3, tauri-plugin-deep-link 2.4.9 (version verification).
- `src-tauri/tauri.conf.json`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`, `deny.toml`, `.github/workflows/ci.yml` — current project state.
- `file src-tauri/icons/icon.png` → 32×32 (icon gap).

### Secondary (MEDIUM confidence)
- v2.tauri.app/distribute/pipelines/github — workflow shape, `permissions: contents: write`, ubuntu deps.
- v2.tauri.app/distribute/appimage — glibc floor, WebKitGTK runtime expectation, build-on-22.04 guidance, bundle.linux.appimage keys.
- github.com/tauri-apps/tauri-action — `tagName`/`releaseName`/`args`/`projectPath`, `--bundles appimage`, auto release+upload.
- docs.appimage.org/packaging-guide/environment-variables — `APPIMAGE` = full stable path; mount at `/tmp/.mount_*`.

### Tertiary (LOW confidence)
- tauri-apps/tauri issues #14796 / #15106 (linuxdeploy icon failures), #10388 (CI AppImage FUSE) — corroborating pitfalls; verify against the exact toolchain at build time.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — versions read from Cargo.lock; no new crates.
- nxm:// registration + self-test mechanics: HIGH — read directly from installed plugin + Tauri source.
- CI / release workflow: MEDIUM — official docs + action README; confirm exact action ref at plan time.
- License audit technique: HIGH (cargo-deny already in place) / MEDIUM (bundled-binary review is a standard but manual procedure).
- Icon gap: HIGH — confirmed via `file`.

**Research date:** 2026-06-22
**Valid until:** 2026-07-22 (stable tooling; re-verify the `tauri-action` ref and Ubuntu runner image if planning later).
