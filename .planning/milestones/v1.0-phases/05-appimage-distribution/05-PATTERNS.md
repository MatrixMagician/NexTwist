# Phase 5: AppImage Distribution - Pattern Map

**Mapped:** 2026-06-22
**Files analyzed:** 6 (2 new files, 2 modified files, 1 icon regen, 1 new doc)
**Analogs found:** 5 / 6 (DIST-AUDIT.md is a doc — no code analog)

> RESEARCH.md (`05-RESEARCH.md`) already pinpoints the exact analog and code
> location for every file in this phase. This map binds those to the **current
> verified source** so the planner can copy concretely. Where RESEARCH already
> gives a verified excerpt (plugin source, workflow skeleton, audit commands),
> this map references it rather than re-deriving.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `.github/workflows/release.yml` (NEW) | config (CI) | event-driven (tag push) | `.github/workflows/ci.yml` | role-match (CI workflow; different trigger) |
| `src-tauri/src/lib.rs` (MODIFY) | provider (setup wiring) | event-driven (startup hook) | in-file: existing `register_all()` + `on_open_url` block (lines 84-99) | exact (same closure, same plugin, same non-fatal pattern) |
| `src-tauri/tauri.conf.json` (MODIFY) | config (bundle) | n/a (static config) | self (existing `bundle` block, lines 33-40) | exact (extend existing keys) |
| `src-tauri/icons/*` (REGEN) | asset | n/a (build input) | existing `icon.png` (32×32 — too small) | regen, not copy |
| `DIST-AUDIT.md` (NEW) | doc | n/a | project markdown style (`CLAUDE.md`, planning SUMMARY docs) | doc-only, no code analog |
| audit helper (script or documented cmds) | utility (build/release) | batch (shell enumeration) | RESEARCH "Bundled-binary audit" code block (verified commands) | derived from RESEARCH |

## Pattern Assignments

### `.github/workflows/release.yml` (config, event-driven)

**Analog:** `.github/workflows/ci.yml` (read in full this session — 66 lines).

**Copy these structural pieces from `ci.yml`** (do NOT invent new apt sets / cache steps):

**Checkout + apt deps** (`ci.yml` lines 25-36) — reuse verbatim, then ADD `patchelf`
(linuxdeploy RPATH fixups, per RESEARCH Pitfall / Standard Stack):
```yaml
      - uses: actions/checkout@v4
      - name: Install Tauri Linux build deps
        run: |
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

**Toolchain + cache** (`ci.yml` lines 38-44) — reuse verbatim (drop `components: clippy`,
not needed for a bundle job):
```yaml
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
```

**Frontend build** (`ci.yml` lines 46-49) — reuse verbatim:
```yaml
      - name: Build frontend (static SPA Tauri embeds)
        run: |
          npm --prefix frontend ci
          npm --prefix frontend run build
```

**DEVIATIONS from `ci.yml` (locked decisions / RESEARCH):**
- `on:` is `push: tags: ["v*"]` — NOT `branches`/`pull_request` (RESEARCH Pattern 3).
- `runs-on: ubuntu-22.04` — NOT `ubuntu-latest` (RESEARCH Pitfall 3: glibc/WebKitGTK floor).
- Add top-level `permissions: contents: write` (RESEARCH Pitfall 5: default token is read-only).
- Replace the test/clippy/deny steps with the **tauri-action** step (RESEARCH Pattern 3,
  verified skeleton at RESEARCH lines 213-243): `uses: tauri-apps/tauri-action@v0`,
  `env: GITHUB_TOKEN`, `with: tagName / releaseName / projectPath: src-tauri / args: --bundles appimage`.
  Confirm exact `@v0` vs pinned `action-v0.6.2` ref at plan time (RESEARCH Open Q1).
- Per RESEARCH Open Q2: re-run `cargo deny check advisories bans licenses sources` once
  in this workflow and capture its output as the reproducible `DIST-AUDIT.md` evidence
  (copy the deny step shape from `ci.yml` lines 57-65).

---

### `src-tauri/src/lib.rs` (provider, setup wiring)

**Analog:** the existing block in the SAME file, `setup` closure, lines 84-99 (read this
session). The phase ADDS the self-test immediately after the existing `register_all()` call.

**Existing pattern to extend** (lines 84-99) — note the `#[cfg(any(windows, target_os = "linux"))]`
guard, the `DeepLinkExt` import, and the non-fatal `if let Err(e) => tracing::warn!` shape:
```rust
            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(e) = app.deep_link().register_all() {
                    tracing::warn!(error = %e, "nxm:// deep-link registration failed (xdg-mime/update-desktop-database missing?)");
                }
                // ... existing on_open_url block ...
            }
```

**Insert AFTER the `register_all()` call, INSIDE the same cfg block** (RESEARCH Code
Examples lines 314-319; verified against plugin 2.4.9 `is_registered()`):
```rust
                // Phase-5 self-test (DIST-01 "self-test passes"). Calls the plugin's own
                // is_registered() — do NOT hand-roll the xdg-mime query (filename-mismatch risk).
                match app.deep_link().is_registered("nxm") {
                    Ok(true)  => tracing::info!("nxm:// handler self-test: PASS"),
                    Ok(false) => tracing::warn!("nxm:// handler self-test: NexTwist is not the default handler"),
                    Err(e)    => tracing::warn!(error = %e, "nxm:// handler self-test: could not query xdg-mime"),
                }
```

**Conventions honored:** non-fatal warn-and-continue (matches the existing `register_all()`
handling and the locked decision); `tracing` macros already in use throughout this file;
`anyhow` at the boundary (no new `thiserror` types needed — this is shell code). The marker
comment on lines 82-83 (`the AppImage .desktop MIME registration is a Phase-5 concern`) can
be updated/removed since this phase resolves it.

---

### `src-tauri/tauri.conf.json` (config, bundle)

**Analog:** the existing `bundle` block in the same file, lines 33-40 (read this session).

**Existing block to extend:**
```json
  "bundle": {
    "active": true,
    "targets": ["appimage"],
    "icon": ["icons/icon.png"],
    "category": "Utility",
    "shortDescription": "Safe, reversible Linux mod manager",
    "longDescription": "NexTwist installs and uninstalls mods safely on Linux: ..."
  }
```

**Changes (RESEARCH "Recommended Project Structure" line 165 + Pitfall 1):**
- After regenerating icons, ensure `bundle.icon` lists a **≥128×128 PNG** (e.g. add
  `"icons/128x128.png"` / the full `cargo tauri icon` output set), not just the 32×32 `icon.png`.
- Add a `bundle.linux.appimage` object ONLY if RESEARCH-flagged keys are actually needed
  (RESEARCH line 165 marks it "if needed" — default appimage bundling works without it;
  do not add empty/speculative keys).
- `version: "0.1.0"` stays as the single source of truth (locked decision; the `v0.1.0`
  tag drives `release.yml` `tagName`). Do NOT duplicate the version into the workflow.

---

### `src-tauri/icons/*` (asset, REGEN — BLOCKER)

**Current state (verified this session):** `src-tauri/icons/icon.png` is **32×32** (only icon
present). This is RESEARCH Pitfall 1 — a hard blocker for clean AppImage bundling.

**Action (not a copy — a regeneration):** run `cargo tauri icon <source-≥1024.png>` to emit the
standard set (`32x32.png`, `128x128.png`, `128x128@2x.png`, …) into `src-tauri/icons/`.
**Verify:** `file src-tauri/icons/128x128.png | grep -q '128 x 128'` (the DIST-01 icon test,
RESEARCH Test Map line 405). No code analog — this is a tooling step.

---

### `DIST-AUDIT.md` (doc, NEW — no code analog)

**No code analog.** Match the project's existing markdown style: a top title + `**Date:**`/
`**Status:**` metadata header (as in the planning `*-PLAN.md` / `*-SUMMARY.md` docs) and
fenced command/output blocks (as `CLAUDE.md` uses). Location: repo root (alongside
`CLAUDE.md`/`README.md`) per RESEARCH structure (line 175), TBD at plan discretion.

**Required contents (locked decisions DIST-02 + RESEARCH lines 322-341):**
1. The `cargo deny check advisories bans licenses sources` result (captured from the
   `release.yml` run for reproducibility — RESEARCH Open Q2).
2. The bundled-binary review findings from the audit helper (below): `ldd` of
   `usr/bin/nextwist`, the `usr/lib/*.so*` list, explicit **UnRAR-absence** evidence
   (named explicitly per CONTEXT specifics line 109), and the rustls-only / no-system-OpenSSL
   confirmation (V6) + no-bundled-WebKitGTK confirmation (uses host).

---

### Audit helper (utility, batch — script or documented commands)

**Analog / source:** RESEARCH "Code Examples → Bundled-binary audit" (lines 322-336),
verified reproducible commands. Copy these directly (as a `scripts/dist-audit.sh` or as a
documented block inside `DIST-AUDIT.md` — discretion):
```bash
APP=NexTwist_0.1.0_amd64.AppImage
./"$APP" --appimage-extract                 # → squashfs-root/
ldd squashfs-root/usr/bin/nextwist           # prove rustls, not OpenSSL (V6)
find squashfs-root/usr/lib -name '*.so*' | sort
find squashfs-root -iname '*unrar*' -o -iname '*libunrar*'   # expect: NO output (DIST-02)
grep -rIl --binary-files=text -e 'UnRAR' squashfs-root/usr/bin/nextwist || echo "no UnRAR string"
find squashfs-root/usr/lib -iname '*webkit*'                 # expect: NO output (uses host)
```

## Shared Patterns

### Non-fatal warn-and-continue (OS-integration calls)
**Source:** `src-tauri/src/lib.rs` lines 87-89 (`register_all()`) and 32-51 (`recover_all_on_launch`).
**Apply to:** the new `is_registered("nxm")` self-test. Every OS-shelling call returns a
`Result` and must `tracing::warn!`/`error!` and continue — never `?`-propagate to abort startup
(locked decision; the app must still open on minimal distros lacking `xdg-mime`).

### CI build prerequisites (apt + toolchain + cache + frontend)
**Source:** `.github/workflows/ci.yml` lines 25-49.
**Apply to:** `release.yml`. Reuse the exact apt set (+`patchelf`), `dtolnay/rust-toolchain@stable`,
`Swatinem/rust-cache@v2`, and the `npm --prefix frontend ci && run build` step. Keeps the two
workflows consistent and avoids drift in the WebKitGTK dep list.

### cargo-deny supply-chain gate
**Source:** `.github/workflows/ci.yml` lines 57-65 + `deny.toml` (UnRAR ban, license allowlist).
**Apply to:** `release.yml` (re-run once to capture evidence for `DIST-AUDIT.md`). `deny.toml`
itself needs **no change** — it already bans `unrar`/`unrar_sys` and gates licenses/sources.

### tracing for shell-side logging
**Source:** used throughout `src-tauri/src/lib.rs` (`tracing::info!/warn!/error!`).
**Apply to:** the self-test. Use structured fields (`error = %e`) consistent with existing calls.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `DIST-AUDIT.md` | doc | n/a | No code analog; it is a checked-in audit record. Match project markdown style only. |

## Metadata

**Analog search scope:** `.github/workflows/`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`,
`src-tauri/icons/`, repo-root markdown. RESEARCH.md (`05-RESEARCH.md`) supplied verified plugin/Tauri
source excerpts (consumed, not re-derived).
**Files scanned:** 4 source/config files read in full + icon inventory + doc-style check.
**Pattern extraction date:** 2026-06-22
