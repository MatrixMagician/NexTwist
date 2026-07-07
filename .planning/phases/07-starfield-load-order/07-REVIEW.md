---
phase: 07-starfield-load-order
reviewed: 2026-07-07T00:00:00Z
depth: deep
files_reviewed: 13
files_reviewed_list:
  - crates/loadorder/src/scan.rs
  - crates/loadorder/src/loot.rs
  - crates/loadorder/src/reconcile.rs
  - crates/loadorder/src/error.rs
  - crates/loadorder/src/masterlist.rs
  - crates/loadorder/src/lib.rs
  - crates/loadorder/Cargo.toml
  - crates/testkit/src/lib.rs
  - src-tauri/src/commands/plugins.rs
  - src-tauri/src/lib.rs
  - frontend/src/lib/api.ts
  - frontend/src/routes/+page.svelte
  - crates/loadorder/tests/plugins.rs
findings:
  critical: 1
  warning: 0
  info: 4
  total: 5
status: fixed
resolution:
  CR-01: fixed
  IN-01: deferred
  IN-02: fixed
  IN-03: fixed
  IN-04: deferred
---

# Phase 7: Code Review Report

**Reviewed:** 2026-07-07
**Depth:** deep (cross-file: engine guard ↔ Tauri adapter ↔ frontend save flow)
**Files Reviewed:** 13
**Status:** issues_found (highest severity: CRITICAL)

## Summary

Phase 7 adds Starfield load-order support: medium-master classification (`PluginView.medium`),
a libloot-derived protected-master set (SFLO-03), a pure `plugins.txt` reconciliation verdict
(SFLO-04), and the masterlist snapshot date. The reconcile engine, medium classification, serde
wire contract, graceful-degrade in `list_plugins`, and the engine/Tauri boundary are all sound
and well-tested.

However, the **SFLO-03 protected-master guard in `apply_load_order` is miscalibrated and breaks
the core PLUGIN-02 save flow for Starfield** (CR-01). The guard treats `enabled == false` on an
implicitly-active master as a "disable attempt", but `enabled == false` is the *normal resting
state* of every protected master in NexTwist's own data model — that is precisely the condition
under which a plugin is classified `protected`. The result: once the UI locks a base master
(which requires `protected == true`, which requires `enabled == false`), every subsequent
`save_plugin_order` for that game is rejected with a spurious `ProtectedMaster` error. The
feature is internally self-contradictory: a master can be *either* locked in the UI *or*
saveable, never both. The engine unit test that "passes" (`starfield_asterisk`) only does so by
hand-passing `enabled: true`, which is the exact opposite of what `list_plugins` produces — so
the integration gap masks the defect.

The remaining four items are low-impact quality/robustness notes (Info).

## Structural Findings (fallow)

None supplied for this review (no `<structural_findings>` block provided).

## Critical Issues

### CR-01: Protected-master guard rejects every legitimate save that includes an implicitly-active master

**Status: FIXED** (commit `037be7d`). Removed the NexTwist-side protected-master guard block in
`apply_load_order` (both the resting-state disable-check and the false-positive reorder-check) and
dropped the now-unconstructable `LoadOrderError::ProtectedMaster` variant. No non-false-positive
typed condition is cleanly expressible: a protected master's resting state IS `enabled == false`
and its request order carries no user intent (the UI locks masters; the merge name-sorts them).
Protection is genuinely libloot-delivered — `reconcile_order` forces every master into libloot's
canonical position unconditionally, libloot pins its early-loader prefix, and masters are never
asterisk-written — plus the UI lock (this IS SFLO-03 "determined via libloot"). The two synthetic
tests (`protected_reorder_rejected`, `protected_disable_rejected`) were replaced by real-flow tests:
`starfield_locked_master_saves_at_resting_state` (a locked master at `enabled == false` saves with
NO false rejection and writes the asterisk file for regular plugins) and
`starfield_pinned_master_reorder_is_neutralized` (a genuine master swap is neutralized — game master
stays pinned first — not honored).

**File:** `crates/loadorder/src/loot.rs:391-424` (guard); reached via
`src-tauri/src/commands/plugins.rs:245` → `save_plugin_order_inner`; UI at
`frontend/src/routes/+page.svelte:759-766` (`onSavePluginOrder`).

**Issue:**

The protected set is defined as `is_plugin_active(name) && !enabled_names.contains(name)`
(`implicit_protected_set`, loot.rs:169-175), where `enabled_names` is derived from the request's
`Plugin.enabled` flags. A plugin is therefore `protected` **iff it is active AND carries
`enabled == false`**. The guard then does (loot.rs:418-423):

```rust
if let Some(p) = plugins.iter().find(|p| !p.enabled && protected.contains(&p.name)) {
    return Err(LoadOrderError::ProtectedMaster(p.name.clone()));
}
```

By construction, *every* member of `protected` is a request entry with `enabled == false`, so
this `find` matches whenever `protected` is non-empty. The check "reject a protected master the
caller marked disabled" is thus equivalent to "reject any save that contains a protected master
at all" — because a protected master is *defined by* being `enabled == false`.

Cross-file trace proving reachability in the real (non-test) path:

1. `merged_plugins_locked` (plugins.rs:28-86) sets `view.enabled` from the stored `plugin_state`
   row, else `false`. There is **no seeding** of `plugin_state` (rows only appear after a save,
   `crates/store/src/plugins.rs:16`), so a fresh Starfield base master (`Starfield.esm`,
   `BlueprintShips-Starfield.esm`, the CC masters) has `enabled == false`.
2. That same base master is `is_plugin_active == true` (implicit master), so
   `protected_set` (plugins.rs:79) marks it `protected == true`. The UI locks it: checkbox
   `disabled={busy || p.protected}` and reorder buttons `disabled={... || p.protected}`
   (+page.svelte:1802-1807, 1795), and `onPluginToggle` early-returns for protected
   (+page.svelte:748). So the user **cannot** flip it to `enabled == true`.
3. `onSavePluginOrder` sends the **full** `plugins` array unchanged
   (`plugins.map((p,idx)=>({...p, order: idx}))`, +page.svelte:761) — including the locked base
   master at `enabled == false`.
4. `apply_load_order` recomputes `protected` (base master `is_plugin_active && !enabled` →
   protected), `!protected.is_empty()` is true, and the disable-check `find` returns the base
   master → `Err(ProtectedMaster("Starfield.esm"))`. **The save always fails.**

The `starfield_asterisk` engine test (plugins.rs:436-442) hides this: it hand-passes
`plugin("Starfield.esm", Esm, true, 0)` with the comment "user-enabled (normal case) → never
protected → guard dormant." But that is the *inverse* of the integration reality — a base master
that arrives `enabled == true` is excluded from `protected`, so the UI would **not** lock it
either (contradicting SFLO-03). No test drives the true path
(`list_plugins` → protected/locked master at `enabled == false` → `save_plugin_order`), which is
why the contradiction was not caught. `protected_disable_rejected` (plugins.rs) actually codifies
the broken behaviour as "correct."

Secondary defect in the same block: the reorder-check (loot.rs:408-417) compares
`desired_protected` (protected names in *request* order) to `canonical_protected` (libloot order).
On first load the base masters have `order == 0` and are name-sorted by
`merged.sort_by(order, then name)` (plugins.rs:84), so with ≥2 implicit masters the request order
is alphabetical, which need not equal libloot's canonical order → the reorder-check *also*
misfires (fires before the disable-check). Both halves of the guard defend a path libloot already
makes safe: masters are never written to the asterisk file (enabled flag is a no-op for them, see
`writes_asterisk_masters_first` assertion) and `reconcile_order` rebuilds master positions from
`canonical` regardless of the request order — so a protected master genuinely *cannot* be disabled
or reordered through `apply_load_order`, yet the guard rejects the attempt to leave it untouched.

**Fix:**

The guard must not treat a protected master's normal `enabled == false` / implicit position as
tampering. Because libloot already ignores the request's enabled flag and canonical order for
implicit masters, the safe and minimal fix is to remove the two checks that fire on the normal
resting state, and (if defense-in-depth is still wanted) reject only a genuine *effective* change.
Concretely:

```rust
// Drop the disable-check entirely: a protected master is *defined* by enabled == false,
// so `!p.enabled && protected.contains(..)` is always true for it — it flags the normal
// state, not a disable attempt. Masters are never written to the asterisk file, so the
// enabled flag on a protected master has no effect and needs no guard here.

// For the reorder-check, compare against the order that will ACTUALLY be written
// (post-reconcile), not the raw request order, so a name-sorted first-load list is not a
// false positive. Since reconcile_order forces masters into `canonical` unconditionally,
// this check can only ever pass — prefer to delete it and rely on libloot's own pinning,
// or keep it purely as a typed re-throw of libloot's error AFTER set_order_and_save fails.
```

Minimum viable change: delete lines 391-424 (the whole protected block) — the reversibility
guarantee is unaffected because libloot pins the early-loaders and masters are not asterisk-written.
Then add an integration test that drives the real path: build a merged view where an
implicitly-active master is `enabled == false` / `protected == true` and assert
`save_plugin_order_inner` (or `apply_load_order`) **succeeds** and writes the expected asterisk
file. If a typed `ProtectedMaster` error is still desired for UX, raise it only by mapping
libloot's rejection when it actually rejects, not pre-emptively on the untouched normal state.

## Info

### IN-01: `reconcile_plugins_txt` over-lists reordered plugins in the Drift set

**Status: DEFERRED** (noisy-but-SAFE — the Drift branch only ever ADDS names, never produces a
false `InSync`, so it cannot mask real drift). Known tightening opportunity: push only the names
whose position actually differs rather than the whole common set. Left as-is for this phase.

**File:** `crates/loadorder/src/reconcile.rs:92-110`

**Issue:** When the relative order of the common (present-in-both, non-protected) plugins
diverges, step (3) pushes **every** common name into `drift`, not just the ones that actually
moved. Given recorded `[A, B, C]` vs disk `[A, C, B]`, the Drift verdict lists `A, B, C` even
though `A` did not move. This never produces a false `InSync` (drift is only ever added, never
removed — safe direction), but the user-facing "these changed unexpectedly" list names plugins
that did not change, undercutting the precise remediation the Drift branch is meant to give.

**Fix:** Only push the names whose position differs, e.g. compare the two ordered vectors
element-wise and add a name when `disk_common[i] != recorded_common[i]` (both sides), rather than
dumping the whole `recorded_common`. Keep the existing sort+dedup afterward.

### IN-02: `reconcile_plugins` does not graceful-degrade the protected probe (inconsistent with `list_plugins`)

**Status: FIXED** (commit `8be2072`). `reconcile_plugins` now reuses the same `protected_set` helper
`list_plugins` uses, which logs and degrades to an EMPTY protected set on a probe error instead of
propagating a hard boundary error — consistent handling across both probe sites.

**File:** `src-tauri/src/commands/plugins.rs:354-356`

**Issue:** `list_plugins` deliberately swallows a `protected_plugins` probe failure into an empty
set and logs (plugins.rs:102-112, the SFLO-03 no-regression guarantee). `reconcile_plugins` calls
the same probe but propagates the error (`.map_err(boundary_err)?`), so a prefix/libloot hiccup
turns the best-effort, Starfield-only reconciliation surface into a hard error toast in the UI
(`run("Reconcile plugins", ...)`, +page.svelte:655). Not a safety bug — failing is a safe verdict
— but the two Starfield probe sites treat identical failures inconsistently.

**Fix:** On a probe error in `reconcile_plugins`, log and fall back (e.g. treat `protected` as
empty, or return `ReconcileState::InSync` with a logged note) so a transient probe failure does
not surface a scary error for an advisory feature. Mirror the `protected_set` degrade pattern.

### IN-03: Reconciliation name comparison is case-sensitive under Wine case-folding

**Status: FIXED** (commit `f9e13b5`). Both sides of the recorded-vs-on-disk comparison (set
membership and the relative-order check) are now folded to ASCII-lowercase per the plugins.txt
convention, keeping the original-cased name for Drift output. New test
`reconcile_case_only_difference_is_insync` asserts a pure case difference → `InSync`.

**File:** `crates/loadorder/src/reconcile.rs:59-69, 126-134`

**Issue:** On-disk `plugins.txt` names and recorded names are matched with exact,
case-sensitive `HashSet<&str>` / `==` comparisons. The rest of the engine explicitly handles
Wine case-folding (`steam/casing.rs`); if the game rewrites `plugins.txt` with different casing
than the recorded name (`MyMod.esp` vs `mymod.esp`), the plugin is seen as both "absent from
disk" and "unexpected on disk" → false Drift. Low likelihood (Bethesda generally preserves case,
and NexTwist writes the file), but it is a latent false-positive under the very case-folding the
project treats as a first-class concern.

**Fix:** Normalize both sides with the same case-fold used elsewhere (ASCII-lowercase the
filename for the set keys) before comparing, keeping the original-cased name for the Drift output.

### IN-04: `masterlist_snapshot_date` hardcodes a date that must be hand-bumped

**Status: DEFERRED** (accepted decision — the bundled `include_str!` snapshot has no runtime mtime to
stat; Phase 9 re-validates). The `"2026-07-07"` literal MUST be bumped in lock-step whenever
`assets/starfield/masterlist.yaml` is refreshed. Left as-is for this phase.

**File:** `crates/loadorder/src/masterlist.rs:64-69`

**Issue:** The Starfield snapshot date is a string literal `"2026-07-07"` that must be updated
in lock-step whenever `assets/starfield/masterlist.yaml` is refreshed. If a future masterlist bump
misses this const, the UI silently shows a stale "masterlist from {date}" note — an incorrect
freshness claim to the user. The coupling is documented but relies on a human remembering it.

**Fix:** Acceptable for this phase (the bundled snapshot has no runtime file to `stat`, as the
doc notes). To harden: derive the date from a build-time artifact (e.g. a committed
`assets/starfield/masterlist.date` read via `include_str!` next to the `include_str!`'d YAML) so
the date and the data cannot drift apart, or add a test asserting the two are refreshed together.

---

_Reviewed: 2026-07-07_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
