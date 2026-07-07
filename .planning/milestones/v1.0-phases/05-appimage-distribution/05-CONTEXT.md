# Phase 5: AppImage Distribution - Context

**Gathered:** 2026-06-22
**Status:** Ready for planning

<domain>
## Phase Boundary

Package NexTwist as a single-file Linux AppImage that a user (or distro) can
download and run with no install friction, registers the `nxm://` MIME handler
automatically and durably when run from the AppImage, and passes a license-compliance
audit confirming no non-free bundled code is shipped (DIST-01, DIST-02).

In scope: AppImage build via Tauri's bundler, durable `nxm://` handler registration
from an AppImage context, a tag-triggered CI release pipeline, and a documented
license/bundled-binary audit.

Out of scope (v2 or non-goals): auto-updater, code-signing infrastructure,
multi-arch (aarch64) builds, Flatpak/other package formats, native Windows/macOS builds.
</domain>

<decisions>
## Implementation Decisions

### nxm:// Handler Registration from AppImage
- Rely on `tauri-plugin-deep-link`'s built-in `$APPIMAGE` handling for a stable
  `Exec=` path (appimagetool exports the `APPIMAGE` env var; the plugin writes the
  stable AppImage path into the `.desktop` file). No custom `.desktop` writer.
- Register the handler on first run automatically — keep the current
  `register_all()` call in the Tauri `setup` hook; registration is idempotent on
  relaunch.
- Add a lightweight handler self-test: a startup/diagnostic check that runs
  `xdg-mime query default x-scheme-handler/nxm` and surfaces a warning if the
  default is not NexTwist (satisfies the success-criterion "self-test passes").
- If `xdg-mime` / `update-desktop-database` are absent (minimal distros): keep the
  existing non-fatal warn-and-continue behavior (the app still opens) plus a
  one-line UI hint. Do not hard-fail startup.

### Build & Release Pipeline
- Build the AppImage with `cargo tauri build --bundles appimage` (Tauri's built-in
  linuxdeploy path). The `appimage` bundle target is already configured in
  `src-tauri/tauri.conf.json`.
- Automate release in CI: add a tag-triggered GitHub Actions job that builds the
  AppImage and uploads it to a GitHub Release. (Existing `ci.yml` test+deny job is
  untouched; release is a separate workflow/job.)
- First release version: keep `0.1.0`, tag `v0.1.0`. `tauri.conf.json` `version` is
  the single source of truth.
- Target architecture: x86_64 only for v1 (matches the Steam/Proton Linux desktop
  reality). aarch64 deferred to v2.

### License-Compliance Audit (DIST-02)
- Audit evidence = the existing `cargo-deny` gate (sources/licenses/bans/advisories)
  **plus** a one-time bundled-binary review of the actual built AppImage contents
  (enumerate bundled libraries/binaries; confirm no `unrar`/non-free code is shipped).
- Record the audit as a checked-in artifact: a short `DIST-AUDIT.md` capturing the
  cargo-deny result and the bundled-binary review findings.
- RAR support in the shipped binary: confirm and document zero bundled RAR code —
  `.rar` is handled by shelling out to a system `unrar`/`7z` (already enforced by the
  `unrar`/`unrar_sys` bans in `deny.toml`).
- CI gating shape: keep `cargo-deny` as the enforced per-push gate (already in
  `ci.yml`); run the bundled-binary review at release time (not per push).

### Claude's Discretion
- Exact placement/format of the self-test (startup log vs. settings panel surface),
  the UI-hint copy, the `DIST-AUDIT.md` layout, and the release-workflow file
  structure are at Claude's discretion, consistent with codebase conventions.
</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src-tauri/tauri.conf.json` — already declares `bundle.targets: ["appimage"]`,
  the `deep-link` plugin with `schemes: ["nxm"]`, icon, category, and `version 0.1.0`.
- `src-tauri/src/lib.rs` — single-instance and deep-link plugins already wired in the
  load-bearing order (single-instance BEFORE deep-link); `register_all()` already
  called in `setup` with a non-fatal warn path. Carries the explicit marker:
  `// the AppImage .desktop MIME registration is a Phase-5 concern (RESEARCH Pitfall 4)`.
- `deny.toml` — already bans `unrar`/`unrar_sys`, gates the license allowlist
  (including the GPL libloot family with documented rationale), and configures
  advisories. DIST-02 source-license audit is effectively pre-positioned here.
- `.github/workflows/ci.yml` — runs `cargo test --workspace`, `cargo clippy`, and
  `cargo deny check advisories bans licenses sources`. Installs the WebKitGTK build
  deps. Does NOT yet build/bundle the AppImage or publish a release artifact.

### Established Patterns
- Headless safety engine in `crates/*` (zero Tauri deps); `src-tauri/` is a thin
  adapter. Packaging work lives in `src-tauri/`, config, and CI — not in the engine.
- `reqwest` is rustls-only and the AppImage must stay self-contained (no system
  OpenSSL/native-tls) — a constraint the bundled-binary review should confirm holds.
- Errors: `thiserror` in engine crates, `anyhow` at the Tauri/app boundary.

### Integration Points
- nxm:// handler: `src-tauri/src/lib.rs` `setup` hook (deep-link registration +
  `on_open_url` routing already present).
- Build/release: `src-tauri/tauri.conf.json` bundle config + a new/extended
  `.github/workflows/` release workflow.
- License audit: `deny.toml` (source gate, exists) + a new `DIST-AUDIT.md` artifact.
</code_context>

<specifics>
## Specific Ideas

- The success criterion explicitly requires a stable Exec path and a passing
  self-test — the self-test (xdg-mime default query) is the concrete evidence for
  "self-test passes."
- DIST-02 wording calls out UnRAR by name as the canonical example of non-free code
  that must not ship; the bundled-binary review should name it explicitly in
  DIST-AUDIT.md.
</specifics>

<deferred>
## Deferred Ideas

- Auto-updater / self-update mechanism — v2.
- Code-signing / notarization infrastructure — v2.
- Multi-arch (aarch64) AppImage builds — v2.
- Flatpak or other package formats — v2 (AppImage is the v1 channel per PROJECT.md).
</deferred>
