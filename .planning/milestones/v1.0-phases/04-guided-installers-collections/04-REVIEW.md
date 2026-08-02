---
phase: 04-guided-installers-collections
reviewed: 2026-06-21T18:33:33Z
depth: deep
files_reviewed: 20
files_reviewed_list:
  - crates/core/src/model.rs
  - crates/fomod/src/parse.rs
  - crates/fomod/src/condition.rs
  - crates/fomod/src/resolve.rs
  - crates/fomod/src/model.rs
  - crates/fomod/src/error.rs
  - crates/fomod/src/lib.rs
  - crates/fomod/Cargo.toml
  - crates/extract/src/staging.rs
  - crates/nexus/src/client.rs
  - crates/nexus/src/collection.rs
  - crates/nexus/src/resolve.rs
  - crates/nexus/src/replay.rs
  - crates/nexus/src/error.rs
  - crates/store/src/collections.rs
  - crates/store/src/db.rs
  - crates/store/src/migrations/V5__collections.sql
  - crates/deploy/tests/collection_round_trip.rs
  - src-tauri/src/commands/fomod.rs
  - src-tauri/src/commands/collections.rs
findings:
  blocker: 1
  critical: 1
  warning: 4
  info: 2
  total: 7
status: resolved
resolution:
  resolved_at: 2026-06-21T00:00:00Z
  branch: gsd/phase-04-guided-installers-collections
  fixed:
    - id: BL-01
      commit: dff6415
      note: wired nexus::compute_collection_ranks into download_collection; collection_round_trip now drives the real parse→rank→persist→deploy path and FAILS without the wiring
    - id: CR-01
      commit: 25ffb06
      note: download_collection now enforces the same domain↔appid gate as resolve_collection
    - id: WR-01
      commit: b08f0c1
      note: resolve now evaluates installStep <visible>; hidden-step files are excluded (corpus tests added)
    - id: WR-02
      commit: 4250cd1
      note: added fomod::validate_selection (server-side group cardinality), called in resolve_fomod + apply_fomod
    - id: WR-03
      commit: 2763a40
      note: AppState lock released before the blocking switch_profile/purge in deploy_collection + uninstall_collection
    - id: WR-04
      commit: a03a784
      note: collection mod Nexus identity resolved honestly; a missing nexus pin errors instead of coercing to 0/0
  deferred:
    - id: IN-01
      reason: accepted — ConflictClass::Resolvable/Blocking + model::Dependency are forward-looking serialized/contract scaffolding; retained intentionally (cross-mod classification not yet built). Tracked, not removed.
    - id: IN-02
      reason: accepted — only ARCHIVED is distinguished in file_availability; other non-installable categories fall through to a caught download-time failure (documented behavior, low impact since the download path surfaces the real failure).
---

# Phase 04: Code Review Report

**Reviewed:** 2026-06-21T18:33:33Z
**Depth:** deep
**Files Reviewed:** 20
**Status:** resolved (BL-01, CR-01, WR-01..04 fixed; IN-01/IN-02 accepted — see `resolution:` frontmatter)

## Summary

Phase 04 adds the headless FOMOD engine (`crates/fomod`), the Collection manifest
parser + resolve-before-download gate + choice-replay/rank mapping (`crates/nexus`),
the V5 collection store facade, the archive root-detection in `extract`, the FOMOD +
Collection Tauri adapters, and the wizard/Collections UI. The safety-critical headless
boundary is well respected: `fomod` is genuinely Tauri/reqwest/keyring-free, `resolve`
is a pure no-write dry-run (asserted by test), the V5 migration is additive/idempotent
with correct FK CASCADE/SET-NULL, the store keeps `rusqlite` out of its public API, the
pristine round-trip is regression-locked, and the off-Nexus SSRF classification + the
Premium gate + the stale-choice "fail clearly" contract are all implemented and tested.

However, the central **rule→rank mapping is dead code in the production lifecycle**: the
adapters never call `map_rules_to_ranks`, so every collection mod is persisted with
`rank: 1` and the manifest's `modRules`/`fileOverrides` conflict ordering is silently
dropped. The round-trip test hand-assigns ranks 1 and 2, which masks the defect. This is
the headline correctness bug. A second, security-relevant gap: `download_collection`
omits the `appid_for_domain` domain↔appid check that `resolve_collection` enforces, so a
mismatched/wrong-game manifest can stage mods into the selected game's staging dir.

## Blocker Issues

### BL-01: Collection `modRules`/`fileOverrides` ranks are never applied — conflict ordering is silently lost

**File:** `src-tauri/src/commands/collections.rs:424-432` (also `:289-294`); dead consumer in `crates/nexus/src/replay.rs:149-199`
**Issue:**
`map_rules_to_ranks` is the entire mechanism that translates a Collection's `modRules`
(`after`/`before`) and per-mod `fileOverrides` into the Phase-2 conflict-rank model. It
is exported (`crates/nexus/src/lib.rs:46`) and unit-tested, but it has **zero production
callers** — verified by searching `src-tauri/` and `crates/` for non-test references.

In `download_collection`, every mod is persisted via `persist_collection_mod`, which
hardcodes `rank: 1`:

```rust
let cm = nextwist_core::CollectionMod {
    mod_id: dl.mod_id,
    nexus_mod_id: m.source.mod_id.unwrap_or(0),
    file_id: m.source.file_id.unwrap_or(0),
    md5: m.source.md5.clone(),
    phase: m.phase,
    rank: 1,          // <-- every collection mod gets the same rank
    choices_json,
};
```

`deploy_collection` then reads that stored rank back (`set_profile_mod(profile_id,
cm.mod_id, true, cm.rank)`), so all mods deploy at rank 1. The Collection's authored
conflict order (`modRules`) and force-win paths (`fileOverrides`) never reach the deploy
engine. Two mods that contest a destination resolve by the engine's tie-break (first-seen
/ insert order), NOT by the author's intent — the opposite of the manifest's contract and
of invariant #6 ("after ⇒ higher rank = winner"). `phase` is persisted but is likewise
only used for `list_collection_mods` ORDER BY, never to sequence deployment, so phase-based
overwrite intent is also lost.

The `collection_round_trip` test does not catch this because it bypasses the adapter and
hand-writes `rank: 1` / `rank: 2` directly into `add_collection_mod`, then asserts modA
wins. The real `download_collection` path can never produce those ranks.

**Fix:** In `download_collection`, after the manifest is parsed and the available set is
known, build a stable per-mod key map and call `map_rules_to_ranks`, then seed each mod's
baseline rank (manifest order) and apply `rank_delta` before persisting:

```rust
// key every resolved mod the same way map_rules_to_ranks' key_for resolves a ModReference
let key_for = |r: &ModReference| resolve_ref_to_mod_key(r, &collection.mods);
let overrides: HashMap<String, Vec<String>> = collection.mods.iter()
    .filter(|m| !m.file_overrides.is_empty())
    .map(|m| (mod_key(m), m.file_overrides.clone()))
    .collect();
let adj = nexus::map_rules_to_ranks(&collection.mod_rules, &overrides, key_for);

// baseline = manifest index (1-based); apply the delta; clamp to >= 1
let baseline = (idx as i64) + 1;
let rank = (baseline + adj.get(&mod_key(m)).map(|a| a.rank_delta).unwrap_or(0))
    .max(1) as u32;
```

and pass `rank` into `CollectionMod`. Add an adapter-level (or store-level) test that
drives `download`→`deploy` end-to-end through the real persisted ranks and asserts the
`after` winner, so the round-trip can no longer be satisfied by hand-set ranks.

## Critical Issues

### CR-01: `download_collection` does not bind the manifest domain to the selected appid (wrong-game install)

**File:** `src-tauri/src/commands/collections.rs:124-243` (missing the check present at `:71-78`)
**Issue:**
`resolve_collection` correctly enforces the domain↔appid binding:

```rust
let resolved_appid = appid_for_domain(&domain)
    .ok_or_else(|| format!("This Collection is for '{domain}', which NexTwist does not manage"))?;
if resolved_appid != appid { return Err("...not the selected game".into()); }
```

`download_collection` parses the same manifest, derives `domain`, but **never re-checks
it against `appid`**. It calls `run_download_to_window(state, window, &dl_id, appid,
&domain, ...)`, and that function only re-resolves the appid from the domain when
`appid == 0` (the nxm-retry sentinel) — for a real appid it trusts the caller
(`downloads.rs:119-127`). So invoking `download_collection` with a Skyrim manifest while
`appid` is Fallout 4 (a direct IPC call, or any UI path that doesn't re-run resolve)
downloads the manifest's mods and stages them under the *selected* game's `staging_dir`.
This is the resolve-before-download gate being only advisory at the boundary that actually
writes to disk; a thin adapter must enforce the same safety invariant the resolve command
does (invariants #1 and #5). It does not breach byte-for-byte purge (staged mods are still
reversible), hence Critical rather than Blocker, but it is a real cross-game integrity
hole.

**Fix:** Add the identical gate at the top of `download_collection`, before the Premium
check and before `add_collection`:

```rust
let domain = collection.info.domain_name.clone();
match appid_for_domain(&domain) {
    Some(d) if d == appid => {}
    Some(_) | None => return Err(format!(
        "This Collection is for '{domain}', not the selected game")),
}
```

## Warnings

### WR-01: `resolve` ignores `installStep` `<visible>` conditions — hidden-step files always install

**File:** `crates/fomod/src/resolve.rs:82-110`
**Issue:**
`resolve` walks `module.steps.steps` unconditionally and never evaluates
`step.visible`. The model parses `<visible>` (`model.rs:62-64`) and `condition::eval`
can evaluate it (proven by `corpus.rs::eval_reads_flag_condition`), but resolve never
consults it. A step that the FOMOD spec hides (its `<visible>` dependency is false) still
contributes its selected/Required options to the install plan. The wizard mirrors the
same gap: `fomodVisibleSteps` is just `fomodProj?.steps` (no live visibility filter,
acknowledged in the source comment at `+page.svelte:436`), and `tryOpenFomodWizard`
pre-selects every `Required` option across *all* steps, including ones that should be
hidden. Result: a conditional-installer FOMOD installs files it should not, diverging
from the spec. Reversibility is preserved (still staged + manifested), so this is a
correctness/robustness Warning, not a pristine Blocker.

**Fix:** In `resolve`, skip a step whose `visible` dependency does not hold against the
current flags/files:

```rust
for step in &steps.steps {
    if let Some(vis) = &step.visible {
        if !eval(vis, &selection.flags, &selection.files) { continue; }
    }
    // ... existing group/plugin walk
}
```

Add a corpus test asserting a hidden step's files are absent from the plan, and have the
wizard re-derive visible steps from a resolve pass (or at minimum not pre-select Required
options inside an invisible step).

### WR-02: `apply_fomod` does not enforce group selection cardinality server-side

**File:** `src-tauri/src/commands/fomod.rs:287-335` (and `resolve_fomod` `:261-277`)
**Issue:**
Group constraints (`SelectExactlyOne` ⇒ exactly 1, `SelectAtLeastOne` ⇒ ≥1) are enforced
only in the webview (`+page.svelte:480-490` `fomodStepValid`). The `apply_fomod` adapter
re-resolves and rejects only a *blocking* same-destination conflict; it does not validate
that the incoming `SelectionDto` honors each group's cardinality. A crafted IPC call (the
webview is not a trust boundary) can submit a `SelectExactlyOne` group with two options
chosen, or a `SelectAtLeastOne` group with none, and the engine will resolve whatever was
sent. This does not break reversibility, but it lets the server stage a selection the
FOMOD author declared invalid — the "fail clearly, never mis-install" posture should hold
on the server, not only in the UI.

**Fix:** Add a pure validation pass in the engine (e.g. `fomod::validate_selection(module,
selection)` returning `FomodError::MalformedSchema`/a new `InvalidSelection` variant) and
call it at the top of both `resolve_fomod` and `apply_fomod`, counting chosen options per
group against `group.group_type`. Cover it with a unit test (two-in-a-radio rejected).

### WR-03: Long-running `deploy::switch_profile` / `deploy::purge` run while holding the `AppState` mutex

**File:** `src-tauri/src/commands/collections.rs:257-298` and `:312-359`
**Issue:**
`deploy_collection` takes `let guard = state.lock().await;` and then calls
`deploy::switch_profile(&guard.store, &game, profile_id)` — a journaled purge → deploy →
load-order → set-active pass that performs many filesystem syscalls — entirely under the
held lock. `uninstall_collection` does the same around `deploy::purge` plus a
`std::fs::remove_dir_all` loop. Because `AppState` is a single `tokio::sync::Mutex`, every
other command (including download progress bookkeeping and cancel) is blocked for the full
duration of a large Collection deploy/purge. This is a responsiveness/lock-contention
defect, not a correctness one; flagged because the other adapters in this phase
(`download_collection`) deliberately scope their locks narrowly (`let guard = ...; ...; }`)
and this one does not, so it is an inconsistency that will manifest as a frozen UI on big
load orders. (Pure throughput/perf is out of scope, but holding a global lock across a
blocking FS engine call is a structural locking issue, which is in scope.)

**Fix:** Clone/extract what the engine needs (the `Store` handle is behind the guard, so
either move the deploy call off the lock by restructuring `AppState` to hold the `Store`
in an `Arc`, or run the blocking deploy via `tokio::task::spawn_blocking` after copying
the needed owned data). At minimum, scope the lock to the store reads
(`get_collection`/`list_collection_mods`/`create_profile`/`set_profile_mod`) and release
it before the `switch_profile`/`purge` call if the store can be shared.

### WR-04: `nexus_mod_id`/`file_id` silently coerce to 0 when a pin is missing

**File:** `src-tauri/src/commands/collections.rs:426-427`
**Issue:**
`persist_collection_mod` records `nexus_mod_id: m.source.mod_id.unwrap_or(0)` and
`file_id: m.source.file_id.unwrap_or(0)`. The fetchable partition already guarantees both
are `Some` for `nexus` sources (a missing id is pushed to `report.failed` at `:178-181`),
and only successfully-downloaded mods reach `persist_collection_mod` — so in practice the
`unwrap_or(0)` is unreachable for nexus mods. But for a `bundle` mod that gets persisted
through a future code path, a `0/0` Nexus identity would be written silently as if it were
a real pin, defeating later file-matching/provenance. This is a latent data-integrity trap
hidden behind a current invariant.

**Fix:** Either assert the invariant (`expect`/return a `boundary_err` if a nexus mod
reaches persistence without ids) or model the absence honestly (store `Option`/skip the
provenance row for non-nexus sources) rather than coercing to a sentinel `0`.

## Info

### IN-01: `ConflictClass::Resolvable`/`Blocking` and `Dependency` enum are unused contract scaffolding

**File:** `src-tauri/src/commands/fomod.rs:196-206`; `crates/fomod/src/model.rs:332-355`
**Issue:** `classify_plan` always returns `ConflictClass::None` (the engine never emits a
plan with an unresolved contest), so `Resolvable`/`Blocking` are never constructed
server-side — flagged dead with `#[allow(dead_code)]`. Likewise `model::Dependency` is a
"convenience" enum the evaluator never consumes (`eval` walks the `Vec`s directly). Both
are documented as forward-looking contract surface, which is reasonable, but they are dead
today and worth a tracking note so they don't rot.
**Fix:** Keep if the cross-mod classification work is imminent; otherwise drop `Dependency`
and compute `Resolvable` honestly in `classify_plan` (it already has the plan + priorities
to detect an authored same-destination overwrite) so the variant stops being dead.

### IN-02: `file_availability` treats any non-`archived` category as Available, including unreleased/hidden categories

**File:** `crates/nexus/src/client.rs:318-322`
**Issue:** A 200 whose `category_name` is not `archived` is classified `Available`. Nexus
also exposes categories like `OLD_VERSION` and (for some files) hidden/under-moderation
states; only `ARCHIVED` is special-cased. A pinned file in another non-downloadable
category would be reported as Available and then fail at download time (caught, but it
weakens the resolve report's promise). Low impact because the download path surfaces the
real failure, but the resolve report is the user's pre-download trust signal.
**Fix:** Consider mapping known non-installable categories to `Archived`/`Unavailable`, or
document that only `ARCHIVED` is distinguished and other categories fall through to a
download-time failure.

---

_Reviewed: 2026-06-21T18:33:33Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
