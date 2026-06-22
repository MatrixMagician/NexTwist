# NexTwist Distribution Audit (DIST-02)

**Date:** 2026-06-22
**Status:** Procedure documented and reproducible; literal release-time output captured per tagged release.
**Scope:** The single-file `NexTwist_<version>_amd64.AppImage` produced by `.github/workflows/release.yml` (`tauri-action --bundles appimage`, `ubuntu-22.04`).

This document is the DIST-02 compliance record. It carries two independent evidence streams:

1. the **source-license gate** (`cargo-deny`), reproducible from the release run, and
2. the **bundled-binary review** of the shipped AppImage (`scripts/dist-audit.sh`).

Both prove the redistributed AppImage ships **no non-free code (UnRAR)** and **no app-path
system OpenSSL**, and that it uses the **host's WebKitGTK 4.1** rather than bundling it.

---

## 1. Source-license gate — `cargo deny`

**Command (re-run in `release.yml` for every tagged release):**

```bash
cargo deny check advisories bans licenses sources
```

**Result:** PASS (expected). The gate is configured in [`deny.toml`](deny.toml) and runs
per-push in `ci.yml`; `release.yml` re-runs the identical check so the result is
**reproducible from the release run itself**. The authoritative literal output for a given
release is the `cargo deny check (DIST-02 audit evidence)` step log of that tag's
`release` workflow run.

> Note: `cargo-deny` is provisioned in CI (via `taiki-e/install-action@v2`), not necessarily
> on a local dev host. This record therefore states the expected-pass result and points to
> the release-run log rather than pasting locally-fabricated output.

**What `deny.toml` enforces (the cited policy):**

- **Bans (load-bearing):** the non-free UnRAR C++ source crates `unrar` and `unrar_sys` are
  explicitly denied (`[[bans.deny]]`). Their license forbids reusing the source to recreate
  the RAR algorithm, which makes them non-free and GPL-incompatible — a redistribution
  liability in an AppImage. `.rar` archives are instead handled by **shelling out to a system
  `unrar`/`7z` binary**, so no RAR algorithm code is compiled into or bundled with NexTwist.
- **Licenses:** only the permissive / project-compatible allowlist (MIT, Apache-2.0,
  Apache-2.0 WITH LLVM-exception, BSD-2/3-Clause, ISC, Zlib, MPL-2.0, Unicode-3.0,
  CC0-1.0, CDLA-Permissive-2.0, bzip2-1.0.6). NexTwist itself is **GPL-3.0-or-later**; the
  LOOT/libloot family (`libloot` GPL-3.0-or-later, `libloadorder`/`esplugin` GPL-3.0) is
  allowed and is license-compatible with NexTwist's GPL-3.0-or-later conveyance.
- **Advisories:** security vulnerabilities and **yanked** crates fail the build;
  informational "unmaintained" advisories inside Tauri's own transitive GTK3 tree are
  scoped to the workspace so they do not gate releases on upstream hygiene.
- **Sources:** unknown registries / git sources surface as warnings.

This covers threats **T-05-04** (non-free UnRAR) and **T-05-05** (vulnerable/yanked
transitive dep) from the phase threat register.

---

## 2. Bundled-binary review — `scripts/dist-audit.sh`

The reproducible enumeration of what native code the AppImage ships. Run it against the
artifact uploaded to the GitHub Release for the tagged version:

```bash
./scripts/dist-audit.sh NexTwist_<version>_amd64.AppImage
```

The helper extracts the AppImage (`--appimage-extract`, no FUSE required) and emits four
evidence sections. The **literal output is a release-time / manual-UAT capture** taken
against the built artifact (the AppImage is produced by `release.yml`, so there is no
artifact to enumerate at planning/commit time — output is **not** fabricated here). The
expected findings, which the audit must confirm, are:

### 2.1 TLS path — `ldd usr/bin/nextwist` (V6, threat T-05-06)

**Expected:** the shipped `nextwist` binary links **no application-path `libssl` / `libcrypto`**.
`reqwest` is configured **rustls-only** (project convention; never native-tls/OpenSSL), so TLS
is satisfied in-process by rustls with no system-OpenSSL dependency. Any `libssl`/`libcrypto`
that appears must resolve to a **host system** path (a transitive system concern), never to a
path inside `squashfs-root/usr/lib`.

### 2.2 Bundled shared libraries — `find squashfs-root/usr/lib -name '*.so*'`

**Expected:** the AppImage's `usr/lib` `.so*` inventory. Recorded verbatim from the release-time
run as the bundled-library manifest.

### 2.3 UnRAR / non-free RAR absence (DIST-02, threat T-05-04)

```bash
find squashfs-root \( -iname '*unrar*' -o -iname '*libunrar*' \)   # expect: no output
# Content grep over ALL shipped native code — the main binary AND every bundled library:
grep -rIl --binary-files=text -e 'UnRAR' \
  squashfs-root/usr/bin/nextwist squashfs-root/usr/lib   # expect: no match
```

**Expected:** **no UnRAR library and no `UnRAR` string** in the shipped native code — both the
main `nextwist` binary and the bundled `usr/lib/*.so*` libraries. Scanning the libraries (not
just the binary) catches the case where the UnRAR algorithm is pulled in statically through a
transitive dependency that lands in a shared object. This is the explicit, by-name confirmation
that the non-free **UnRAR** code is absent — consistent with the `deny.toml` ban and the design
decision to shell out to a system `unrar`/`7z` for `.rar`.

### 2.4 WebKitGTK absence — uses the host (runtime requirement)

```bash
find squashfs-root/usr/lib -iname '*webkit*'   # expect: no output
```

**Expected:** **no bundled WebKitGTK**. The AppImage relies on the **host system's WebKitGTK 4.1**.
This is a documented **runtime requirement**: target systems must provide WebKitGTK 4.1 (the
reason the release builds on `ubuntu-22.04` — the glibc / WebKitGTK 4.1 compatibility floor).

---

## Accepted v1 limitation

**No code-signing / provenance** ships in v1 (threat T-05-07, accepted). The AppImage is
distributed over the HTTPS GitHub Release channel; signing/notarization is deferred to a later
milestone.

---

*Audit helper: [`scripts/dist-audit.sh`](scripts/dist-audit.sh) · Policy: [`deny.toml`](deny.toml) · Release pipeline: [`.github/workflows/release.yml`](.github/workflows/release.yml)*
