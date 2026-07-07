---
status: all_fixed
phase: 05-appimage-distribution
source: [05-REVIEW.md]
fix_scope: critical_warning
findings_in_scope: 3
fixed: 3
skipped: 0
iteration: 1
created: 2026-06-22
note: >
  Reconstructed by the orchestrator. The fixer wrote this report inside its
  isolated worktree and it was lost when the worktree was force-removed after
  the fast-forward merge (a #2070-class uncommitted-artifact loss). The three
  fix COMMITS landed on the branch intact; only this report doc was
  regenerated from the fixer's return.
---

# Phase 5 — Code Review Fix Report

All 3 in-scope (Warning) findings from `05-REVIEW.md` were fixed, each committed
atomically. The 4 Info findings (IN-01..IN-04) were out of scope
(`fix_scope: critical_warning`) and left untouched.

| Finding | Severity | Status | Commit | Files |
|---------|----------|--------|--------|-------|
| WR-01 | Warning | fixed | `c438668` | `.github/workflows/release.yml` |
| WR-02 | Warning | fixed | `27dfff8` | `scripts/dist-audit.sh` |
| WR-03 | Warning | fixed | `6ba5289` | `scripts/dist-audit.sh`, `DIST-AUDIT.md` |

## What changed

- **WR-01** — `release.yml`: added `--locked` to the `tauri-action` build args and
  a pre-bundle validation gate (`cargo test --workspace --locked` +
  `cargo clippy --workspace --all-targets --locked -- -D warnings`) mirroring
  `ci.yml`, plus the `clippy` toolchain component so the lint gate runs. A `v*`
  tag can no longer ship an AppImage built from an unvalidated dependency graph
  (closes the DIST-02 reproducibility gap). No untrusted GitHub-event input
  flows into any `run:` body — no script-injection surface introduced.
- **WR-02** — `dist-audit.sh`: `--appimage-extract` output is now isolated in a
  fresh `mktemp -d` working dir with a `trap 'rm -rf "$WORK"' EXIT` cleanup, so a
  stale `squashfs-root` can no longer merge two artifacts and corrupt the
  evidence sections.
- **WR-03** — `dist-audit.sh` + `DIST-AUDIT.md`: the UnRAR content scan now covers
  the bundled `usr/lib/*.so*` shared libraries in addition to the `nextwist`
  binary; `DIST-AUDIT.md`'s command block and narrative were updated to describe
  the broadened scope so the doc no longer overstates coverage.

## Verification

- `release.yml`: YAML parse OK; `--locked` present on build + lockfile gate; test/clippy gate present.
- `dist-audit.sh`: `bash -n` OK; `mktemp -d` + `trap` cleanup + `usr/lib` scan present.
- These are config/script/doc changes (no Rust logic touched); the workspace
  test suite and `cargo deny` remain green and are unaffected.

## Out of scope (Info — not fixed)

- IN-01: `ldd` non-zero swallowed by `set -e`
- IN-02: hardcoded `0.1.0` default filename in the audit helper
- IN-03: `realpath` portability
- IN-04: `cargo deny` PASS asserted without committed literal evidence (defensible by design)

These remain documented in `05-REVIEW.md` for future consideration.
