---
phase: 06-starfield-detection-ce2-path-resolution
reviewed: 2026-07-07T00:00:00Z
depth: deep
files_reviewed: 12
files_reviewed_list:
  - crates/steam/src/ce2.rs
  - crates/steam/src/resolve.rs
  - crates/steam/src/lib.rs
  - crates/steam/Cargo.toml
  - crates/loadorder/src/loot.rs
  - crates/loadorder/src/masterlist.rs
  - crates/loadorder/src/scan.rs
  - crates/testkit/src/lib.rs
  - src-tauri/src/commands/games.rs
  - src-tauri/src/lib.rs
  - frontend/src/lib/api.ts
  - frontend/src/routes/+page.svelte
findings:
  critical: 0
  high: 1
  medium: 1
  low: 2
  total: 4
status: fixed
resolution:
  fixed: [HI-01, ME-01, LO-02]
  deferred: [LO-01]
  fixed_at: 2026-07-07
  commits:
    - eb860a9  # fix(06): harden My Games redirect traversal guard (HI-01) + regression tests (LO-02)
    - ab466a2  # fix(06): handle Wine str(2)/%USERPROFILE% Documents redirect (ME-01)
---

# Phase 6: Code Review Report

**Reviewed:** 2026-07-07
**Depth:** deep (cross-file: steam ↔ loadorder ↔ tauri ↔ frontend)
**Files Reviewed:** 12 (masterlist.yaml wiring confirmed, not line-reviewed per scope)
**Status:** issues_found

## Summary

Phase 6 adds Starfield (AppID 1716740) as an allow-listed Bethesda game plus a new
read-only CE2 config-path resolver (`crates/steam/src/ce2.rs`), the `installed_build`
ACF reader, and a thin `starfield_status` Tauri forwarder. The engine/adapter boundary
is respected (command is a pure forwarder; `testkit` is correctly a **dev**-dependency of
`steam`, so no test helpers leak into the shipped binary; no `tauri`/`reqwest`/UI deps
entered `steam`/`loadorder`/`testkit`). Fail-safe posture is mostly sound: `installed_build`
and the `user.reg` redirect read both funnel every error through `?`/`.ok()` and cannot
panic on missing/malformed/hostile input. `drift_notice` is correctly dormant at
`VALIDATED_BUILD == 0` and fires only on `installed > validated > 0`. Frontend renders build
numbers as escaped text (no `{@html}`), so no XSS.

**The one material defect is a real path-traversal bypass in the `user.reg` "Personal"
redirect guard** — the guard that focus item #1 and the T-06-01 requirement specifically
promise keeps resolution under `<prefix>/drive_c`. It rejects backslash-delimited `..` but
NOT forward-slash traversal embedded in a single segment, nor an absolute `/…` segment,
both of which escape the prefix. In Phase 6 the consequence is only an out-of-prefix
directory *read* (read-only), but `my_games_path` is the exact base that Phase 7/9 will
*write* `StarfieldCustom.ini` into — so this escalates to a byte-writing escape once the
later phases land on top of it. Fix it now while the surface is read-only.

## High

### HI-01: `user.reg` "Personal" traversal guard is bypassable via `/` and absolute segments (T-06-01 broken)

**Status:** FIXED (commit eb860a9) — `windows_path_to_components` now rejects any
segment with an embedded `/`, an absolute `/…` form, or `components().count() != 1`;
`my_games_path` adds a last-line lexical `starts_with(<prefix>/drive_c)` containment
check with default fallback. Regression tests added (see LO-02).

**File:** `crates/steam/src/ce2.rs:134-148` (`windows_path_to_components`)
**Issue:** The guard splits the Windows path on `\` only and rejects a segment that is
empty / `.` / `..`. It never inspects the *contents* of a segment for a forward slash or a
leading `/`. On Linux, `Path::join` treats a string containing `/` as multiple components
and treats a segment starting with `/` as an absolute path that **replaces** the accumulated
path. Verified empirically:

- `C:\foo/../../../../../../etc\bar` → components `["drive_c", "foo/../../../../../../etc", "bar"]`
  → lexical `…/drive_c/foo/../../../../../../etc/bar/My Games/Starfield` (escapes `drive_c`).
- `C:\/etc` → components `["drive_c", "/etc"]` → `current.join("/etc")` = `/etc` →
  `/etc/My Games/Starfield` (escapes the prefix entirely to an absolute path).

`resolve_cased` (ce2.rs:153-170) then `entry_ci`-probes each component; directory entries
never contain `/`, so the crafted segment never matches, `still_exists` flips false, and the
raw `current.join(comp)` performs the escape. `resolve_ce2_config` (ce2.rs:174-184) then
`read_dir`s the out-of-prefix path and returns it inside `Ce2ConfigState`. The doc comment
at ce2.rs:79-81 and 104-105 asserts this "never escapes `<prefix>/drive_c`" — that contract
is false. Read-only in Phase 6; becomes a write-outside-prefix escape once Phase 7/9 write
`StarfieldCustom.ini` to `my_games_path`, directly violating the project's non-destructive
guarantee.

**Fix:** Reject any segment that is not a plain single path component. Add to the per-`p`
loop in `windows_path_to_components`:
```rust
for p in parts {
    // Reject empty, dot-dirs, AND any segment that is not a single path
    // component: an embedded '/' (unix separator) or a raw absolute segment
    // would let Path::join escape drive_c / the prefix root.
    if p.is_empty()
        || p == "."
        || p == ".."
        || p.contains('/')
        || Path::new(p).components().count() != 1
    {
        return None;
    }
    out.push(p.to_string());
}
```
(Belt-and-suspenders: `resolve_cased`/`my_games_path` could also assert the final path
`starts_with(prefix.join("drive_c"))` before returning, so the guarantee holds regardless of
which producer feeds the components.) Add a regression test for the `/`-embedded and
absolute-`/` vectors — see LO-02.

## Medium

### ME-01: Redirect parser silently ignores Wine's `REG_EXPAND_SZ` (`str(2):`) form and `%VAR%` values

**Status:** FIXED (commit ab466a2) — `extract_personal` now strips an optional
`str(2):`/`str:` type tag and expands a leading `%USERPROFILE%` to
`C:\users\steamuser` (still routed through the HI-01 containment guard). Unit test
`documents_redirect_str2_userprofile_form_is_expanded` added.

**File:** `crates/steam/src/ce2.rs:114-130` (`extract_personal`)
**Issue:** Real Wine/Proton `user.reg` frequently stores `User Shell Folders\Personal` as an
expandable string, i.e. the line is `"Personal"=str(2):"%USERPROFILE%\\Documents"`, not
`"Personal"="C:\\…"`. After `strip_prefix("\"Personal\"")` and `strip_prefix('=')`, the
remainder starts with `str(2):` (or `%USERPROFILE%`), so `strip_prefix('"')` returns `None`
and the whole redirect is dropped. This is *fail-safe* (falls back to the default
`steamuser/Documents`, which is correct for the common non-redirected case), so it is not a
crash or data-loss bug — but a user who genuinely redirected Documents and whose registry
uses the `str(2):` form will have their redirect silently ignored and CE2 resolution pointed
at the wrong (default) directory, surfacing as a perpetual `FirstLaunchPending` or a
mis-located INI later. Note this is distinct from the intentionally-deferred *real-prefix
confirmation* (Phase 9): the parser itself ships now and is incomplete for a form Wine emits
by default.

**Fix:** Accept an optional `str(N):` type prefix before the quote, and expand a leading
`%USERPROFILE%`:
```rust
let rest = rest.trim_start().strip_prefix('=')?.trim_start();
// Optional Wine type tag, e.g. `str(2):`  (REG_EXPAND_SZ) or `str:`.
let rest = rest
    .strip_prefix("str(2):")
    .or_else(|| rest.strip_prefix("str:"))
    .unwrap_or(rest)
    .trim_start();
let inner = rest.strip_prefix('"')?;
let end = inner.find('"')?;
let value = inner[..end].replace("\\\\", "\\");
// %USERPROFILE% → C:\users\steamuser
let value = value.replace("%USERPROFILE%", "C:\\users\\steamuser");
return Some(value);
```
(Still fail-safe: anything it can't expand falls through the existing traversal guard and
default fallback.)

## Low

### LO-01: `STARFIELD` AppID literal duplicated as a private const across four files

**Status:** DEFERRED — hoisting a const into `nextwist_core` violates Phase 6's LOCKED
"no `core` change" decision. Left as-is (consistent with the existing Skyrim/Fallout
per-module-const pattern); a future milestone can hoist all Bethesda AppIDs together.

**File:** `crates/loadorder/src/loot.rs:56`, `crates/loadorder/src/masterlist.rs:29`,
`crates/loadorder/src/scan.rs:36` (each `const STARFIELD: u32 = 1716740;`), plus the source
of truth `crates/steam/src/resolve.rs` `pub const STARFIELD`.
**Issue:** The AppID magic number is re-declared in three `loadorder` modules (each with a
"mirrors …" comment) rather than shared. Skyrim/Fallout already follow this same
per-module-const pattern, so this is consistent with the codebase, but four independent
copies of a load-bearing identity constant are a drift hazard — the "mirrors" comments are
the only thing keeping them in sync, and nothing enforces it. `loadorder` deliberately does
not depend on `steam`, so a shared const would need to live in `nextwist_core`.
**Fix (optional):** Hoist the three Bethesda AppIDs into `nextwist_core` (e.g.
`core::appid::{SKYRIM_SE, FALLOUT4, STARFIELD}`) and have both `steam` and `loadorder`
reference them, eliminating the mirrored literals. Low priority; pre-existing convention.

### LO-02: Traversal test covers only the backslash-`..` vector, giving false confidence

**Status:** FIXED (commit eb860a9) — new test
`documents_redirect_forward_slash_and_absolute_are_rejected` exercises the `/`-embedded
(`C:\users/../../../etc`), absolute (`C:\/etc`), and mixed-separator
(`C:\foo/../../etc\bar`) vectors, asserting each falls back to the default and stays
under `drive_c`. These fail against the pre-fix guard and pass after HI-01.

**File:** `crates/steam/src/ce2.rs:267-283` (`documents_redirect_traversal_is_rejected`)
**Issue:** The test asserts `C:\users\..\..\etc` falls back to default — but that is the one
vector the current guard *does* catch. The forward-slash (`C:\foo/../../etc`) and
absolute-segment (`C:\/etc`) vectors from HI-01, which the guard does **not** catch, are
untested, so the suite currently reports green over a live traversal hole.
**Fix:** After applying the HI-01 fix, add cases:
```rust
// forward-slash traversal inside one backslash segment must fall back to default
"\"Personal\"=\"C:\\users/../../../etc\"\n"        // → default path
// absolute unix segment must fall back to default
"\"Personal\"=\"C:\\/etc\"\n"                       // → default path
```
asserting both resolve to `drive_c/users/steamuser/Documents/My Games/Starfield`.

---

## Verified clean (adversarially checked, no finding)

- **Fail-safe / no panic on hostile input:** `installed_build` (resolve.rs:183-190) and
  `read_personal_redirect`/`extract_personal` use only `.ok()?`/`?`; no `unwrap`/`expect`/
  index in library code. Malformed `user.reg` and malformed/absent ACF both return the safe
  default (tests `malformed_user_reg_falls_back_to_default`, `installed_build_…fail_safe`).
- **Zero writes:** every new engine fn is read-only (`read_to_string`, `read_dir`); the only
  writes in the diff are test fixtures on temp dirs.
- **Drift compare:** `drift_notice` = `Some` iff `installed > validated > 0`; equal/older/
  `None`/`validated==0` all yield `None`. No overflow (comparison only).
- **`library_root_of`:** `ancestors().nth(3)` correctly maps `…/steamapps/common/<game>` →
  `<root>` and returns `None` when too shallow (surfaced as `NotInstalled`).
- **Engine boundary:** `starfield_status` command is a one-line forwarder; `testkit` is a
  `[dev-dependencies]` entry in `steam/Cargo.toml` (not a runtime dep); no UI/HTTP deps
  entered the engine crates.
- **Allow-list consistency:** the `STARFIELD` arm is present and the `_ => None`/`_ => ""`/
  `_ => "Unknown Game"` default is preserved at every sibling `match appid` site
  (`default_name`, `expected_exe`, `SUPPORTED_APPIDS`, `game_type_for`, `appdata_folder_name`,
  `game_slug`, `bundled_snapshot`, `game_id_for`).
- **`entry_ci` reuse:** deterministic (exact-case-first, else lexicographically smallest),
  `pub(crate)` shared verbatim by `ce2::resolve_cased` — case-mismatch test passes.
- **masterlist.yaml wiring:** `include_str!("../assets/starfield/masterlist.yaml")` resolves
  to the bundled 979-line file; offline-fallback test seeds the cache from it.
- **Frontend:** build numbers/paths rendered as escaped text (no `{@html}`, T-06-05);
  `starfield` state reset to `null` on game change; fetch gated to Starfield + explicit
  Re-check (no surprise reads).

---

_Reviewed: 2026-07-07_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
