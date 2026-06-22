---
phase: 05-appimage-distribution
reviewed: 2026-06-22T00:00:00Z
depth: standard
files_reviewed: 6
files_reviewed_list:
  - .github/workflows/release.yml
  - DIST-AUDIT.md
  - scripts/dist-audit.sh
  - src-tauri/src/lib.rs
  - src-tauri/tauri.conf.json
  - src-tauri/tests/nxm_self_test.rs
findings:
  critical: 0
  warning: 3
  info: 4
  total: 7
status: issues_found
---

# Phase 5: Code Review Report

**Reviewed:** 2026-06-22
**Depth:** standard
**Files Reviewed:** 6
**Status:** issues_found

## Summary

Phase 5 packaging work (AppImage distribution, `nxm://` self-test, dist audit) is
solid on the dimensions the project cares most about. The convention requirements
hold up under scrutiny:

- `nxm_self_test` in `lib.rs` is genuinely non-fatal: it consumes every arm of the
  `Result`, returns `()`, and contains no `?`/`unwrap`/`expect`. It uses the plugin's
  `app.deep_link().is_registered("nxm")` rather than a hand-rolled `xdg-mime` shell-out.
  The headless test exercises all three arms. Verified.
- `release.yml` is tag-triggered (`v*`), runs on `ubuntu-22.04`, scopes
  `permissions: contents: write`, pins `tauri-apps/tauri-action@action-v0.6.2`, and
  builds `--bundles appimage`. The `${{ github.ref_name }}` / `${{ secrets.* }}`
  interpolations all flow into `with:` inputs / `env:`, not into `run:` shell bodies,
  so there is **no script-injection surface** in this workflow. Verified.
- `reqwest` is rustls-only at the workspace pin (`default-features = false`, no
  native-tls). No packaging logic leaked into `crates/*`. Verified.

The defects found are real but none are blockers. The most material is a CI
divergence: `release.yml` omits the `--locked` discipline and a clean-build sanity
step that `ci.yml` has, so a release can be cut from a dependency set that CI never
validated. The dist-audit script and doc have robustness/accuracy gaps worth fixing
before they are relied on as compliance evidence.

No Critical issues found.

## Warnings

### WR-01: Release build can drift from CI-validated dependency set (no `--locked`, no pre-release test gate)

**File:** `.github/workflows/release.yml:57-68`
**Issue:** The release pipeline builds the shipped artifact via `tauri-action` but
never runs `cargo test --workspace --locked` or `cargo clippy -- -D warnings` (the
gates `ci.yml:51-55` enforces), and the tauri-action build does not pass `--locked`.
Tag pushes are not guaranteed to have passed `ci.yml` (a tag can be pushed onto any
commit, including one CI never ran or one where CI is still pending/failed). Combined
with `Swatinem/rust-cache` and a fresh `Cargo.lock` resolution, the AppImage you
redistribute can be built from a dependency graph that no green CI run ever covered —
directly undercutting the DIST-02 claim that the release is reproducible and
supply-chain-clean. The `cargo deny` re-run at line 78 partially mitigates (it would
catch a newly-introduced banned/yanked crate) but does not catch a non-reproducible
lockfile resolution or a build/test regression.
**Fix:** Pass `--locked` into the tauri-action build args so the release fails if
`Cargo.lock` is stale, and either (a) gate the release job on a successful CI run, or
(b) add a `cargo test --workspace --locked` step before the bundle step:
```yaml
        with:
          tagName: ${{ github.ref_name }}
          releaseName: "NexTwist ${{ github.ref_name }}"
          projectPath: src-tauri
          args: --bundles appimage --locked
```
Plus a pre-bundle gate step (`cargo test --workspace --locked`) or a `needs:` on the
CI workflow via `workflow_run`/required status checks.

### WR-02: `dist-audit.sh` extracts into and pollutes the caller's CWD with no isolation or cleanup

**File:** `scripts/dist-audit.sh:35,43,47,52`
**Issue:** `"$APP" --appimage-extract` always writes a `squashfs-root/` tree into the
current working directory, and the script never creates a temp dir, never `cd`s, and
never removes the extraction. Consequences: (1) if a `squashfs-root/` already exists
(stale from a prior run or another AppImage), `--appimage-extract` merges into it and
the audit reports a **mix of two artifacts** — a silent correctness failure for a
document positioned as compliance evidence; (2) it litters the invoker's directory
(e.g. a repo checkout or CI workspace) with an un-gitignored multi-hundred-MB tree.
Because all four evidence sections (`ldd`, `find usr/lib`, UnRAR check, WebKit check)
read from this shared `squashfs-root`, a stale tree corrupts every section at once.
**Fix:** Extract into a fresh temp dir and resolve paths relative to it, with cleanup:
```bash
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"
[[ -e squashfs-root ]] && rm -rf squashfs-root
"$APP" --appimage-extract >/dev/null
```
(Or at minimum `rm -rf squashfs-root` before extracting and document that the script
owns the CWD.)

### WR-03: UnRAR-absence check scans only the main binary, not the bundled `.so*` libraries

**File:** `scripts/dist-audit.sh:47-48` / `DIST-AUDIT.md:88-89`
**Issue:** Section 3 proves "no UnRAR ships" via a filename `find` over all of
`squashfs-root` (good) plus a `grep -e 'UnRAR'` scoped only to
`squashfs-root/usr/bin/nextwist`. If the UnRAR algorithm were ever pulled in
statically through a *transitive* dependency that landed in a bundled shared object
under `usr/lib/*.so`, the string grep would miss it because it only inspects the one
binary. The audit narrative ("the explicit, by-name confirmation that the non-free
UnRAR code is absent") overstates the coverage the command actually provides. The
filename `find` covers the whole tree, but the content grep — the part that would
catch statically-linked-in code — does not.
**Fix:** Run the content grep over the same tree the filename check uses, or at least
over both the binary and every bundled `.so*`:
```bash
grep -rIl --binary-files=text -e 'UnRAR' squashfs-root/usr/bin squashfs-root/usr/lib \
  || echo "no UnRAR string in shipped native code"
```
And soften the DIST-AUDIT.md claim to match the actual scan scope, or widen the scan.

## Info

### IN-01: `ldd` failure is silently swallowed by `set -e` semantics on a non-zero exit

**File:** `scripts/dist-audit.sh:39`
**Issue:** With `set -euo pipefail`, if `ldd "$BIN"` returns non-zero (e.g. the binary
is not dynamically linked, or `ldd` errors), the script aborts mid-audit and the later
sections never run — but the partial output may look like a "clean" result to a reader
skimming for absence of `libssl`. This is an evidence-integrity nuance rather than a
bug.
**Fix:** Make the intent explicit, e.g. `ldd "$BIN" || echo "(ldd reported non-zero)"`,
so a linkage anomaly is visible rather than aborting the run.

### IN-02: Hardcoded default AppImage filename pins version `0.1.0`

**File:** `scripts/dist-audit.sh:21`
**Issue:** `APP="${1:-NexTwist_0.1.0_amd64.AppImage}"` hardcodes the current
`tauri.conf.json` version. After the next version bump the zero-arg invocation silently
points at a non-existent file (caught by the `-f` check, so not dangerous) but the
default becomes misleading. The version lives canonically in `tauri.conf.json:4`.
**Fix:** Either require the path argument (drop the default and print usage), or derive
the default from `tauri.conf.json` with a small `jq`/grep so it tracks the real version.

### IN-03: `realpath` is GNU/coreutils-specific; not guaranteed portable

**File:** `scripts/dist-audit.sh:30`
**Issue:** `realpath` is a coreutils utility present on the Ubuntu CI host but not
universal (some minimal/BSD-ish environments lack it). For a script whose whole purpose
is "runs on a minimal box to audit an artifact," depending on a non-POSIX tool is a mild
portability snag. Low impact given the documented CI/dev target.
**Fix:** Acceptable as-is for the Ubuntu target; if portability matters, fall back to a
shell-builtin canonicalization or guard with a `command -v realpath` check.

### IN-04: DIST-AUDIT.md asserts `cargo deny` "PASS (expected)" without committed literal evidence

**File:** `DIST-AUDIT.md:25-33,68-70`
**Issue:** The doc is transparent that literal output is captured per-release-run rather
than pasted (and explicitly avoids fabricating output, which is the right call). But as a
standalone artifact at this commit it states an *expected* result and defers the actual
proof to an external workflow-run log. This is a documentation-completeness observation,
not an inaccuracy — the claims about `deny.toml` policy, the rustls-only TLS posture, and
the host-WebKitGTK runtime requirement are all consistent with the verified code/config.
**Fix:** None required for correctness. Optionally link the specific release run URL into
the doc once a tag is cut, so the evidence is one click from the record.

---

_Reviewed: 2026-06-22_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
