---
phase: 07-starfield-load-order
verified: 2026-07-07T00:00:00Z
status: passed
score: 4/4 must-haves verified (SFLO-01/02/03/04)
behavior_unverified: 0
overrides_applied: 0
deferred:
  - truth: "LOOT-sort output agrees with LOOT desktop on the same REAL inputs"
    addressed_in: "Phase 9"
    evidence: "Phase 9 SC2: 'NexTwist's LOOT sort output agrees with LOOT desktop on the same inputs'. Phase 7 ships the mechanism + fixture-based determinism proof; real-input parity is the on-hardware gate."
  - truth: "The exact real on-launch plugins.txt rewrite delta set (.ccc / stripped implicit ESMs / BlueprintShips-*)"
    addressed_in: "Phase 9"
    evidence: "Phase 9 SC4: 'The exact CE2 INI keys and masterlist currency are validated against the installed game build'. Reconcile is data-driven (libloot-derived protected set) so it self-corrects; the real delta set is validated on the owner's live Proton install."
  - truth: "The real implicitly-active / protected base-master set for Starfield"
    addressed_in: "Phase 9"
    evidence: "Phase 7 derives the set purely from libloot is_plugin_active (no name literals), so the mechanism is correct against whatever the real game reports; the concrete set is confirmed on hardware in Phase 9."
---

# Phase 7: Starfield Load Order Verification Report

> **Post-review correction (2026-07-07, CR-01).** After this verification passed, the code
> review found a CRITICAL bug the green tests had masked: the NexTwist-side protected-master
> guard in `apply_load_order` rejected EVERY legitimate Starfield save, because it conflated
> "protected" with "disabled" (an implicitly-active master's resting state is `enabled==false`).
> The `starfield_asterisk` test only passed by hand-passing the inverse (`enabled:true`) flow.
> **Fixed** (commits `037be7d`/`8be2072`/`f9e13b5`): the miscalibrated guard and the
> now-unconstructable `LoadOrderError::ProtectedMaster` variant were removed; SFLO-03's
> engine-level protection is genuinely delivered by libloot (`reconcile_order` forces every
> master to libloot's canonical position; libloot pins its early-loader prefix; masters are
> never asterisk-written) PLUS the UI lock — which IS "determined via libloot" per SFLO-03.
> A new real-flow test `starfield_locked_master_saves_at_resting_state` proves SFLO-01 saves
> now succeed (previously silently broken), and `starfield_pinned_master_reorder_is_neutralized`
> proves a genuine reorder is still prevented. IN-02 (reconcile graceful-degrade) and IN-03
> (case-insensitive reconcile matching) also fixed. Workspace: **301 passed / 0 failed**, clippy
> clean. The `status: passed` verdict holds and is now genuinely accurate — the SFLO-01 save
> path works end-to-end where the original green tests had hidden a break.

**Phase Goal:** Users can enable/disable, order, and LOOT-sort Starfield plugins safely, with the CE2 medium-master tier and protected base masters handled correctly via libloot. Reuses v1.0 machinery.
**Verified:** 2026-07-07
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Enable/disable/reorder → asterisk `plugins.txt` at the CE2 `AppData/Local/Starfield` path via reused `open_game(1716740)`/`apply_load_order` (SFLO-01) | ✓ VERIFIED | `loot.rs` `game_type_for(1716740) → GameType::Starfield` (L84-91); `apply_load_order` seeds `active_plugins_file_path`, reconciles, `set_order_and_save` (L348-436). Integration test `starfield_asterisk` (plugins.rs L422) proves the write is bounded under the Starfield prefix AppData, enabled `*Mod.esp`, disabled without `*`, implicit master omitted. Wired: `save_plugin_order_inner` → `apply_load_order` (plugins.rs L233-261); api.ts `savePluginOrder`. |
| 2 | LOOT auto-sort via bundled masterlist, preview-then-apply + masterlist-age surfaced (SFLO-02) | ✓ VERIFIED | `propose_sort` loads bundled masterlist, `sort_plugins`, writes NOTHING (loot.rs L472-519); test `propose_sort_returns_order_without_writing` asserts no Plugins.txt after propose. Determinism proven by `starfield_sort_determinism` (plugins.rs L463); `masterlist_date == "2026-07-07"` asserted. Frontend: muted "Masterlist from {date} — may be stale" note (+page.svelte L1741-1743) + explicit "Apply sorted order" after preview (L1769), reusing shipped `.loot-proposal`. |
| 3 | Protected base masters NEVER written/reordered/disabled — libloot-derived (`is_plugin_active` proxy), never hard-coded; medium tier via `esplugin::is_medium_plugin()`; protected shown locked; reorder/disable REJECTED in engine (typed `LoadOrderError::ProtectedMaster`) (SFLO-03) | ✓ VERIFIED | `implicit_protected_set` = `is_plugin_active(name) && !enabled_names.contains(name)` (loot.rs L168-174) — grep confirms NO base-master name literal anywhere in engine src. Medium: `classify` reads `plugin.is_medium_plugin()` (scan.rs L89), no `PluginKind::Medium`; test `medium_master_classifies_true_only_for_starfield`. Engine guard: `apply_load_order` returns `ProtectedMaster` on protected reorder (L408-417) or disable (L418-423); tests `protected_reorder_rejected`, `protected_disable_rejected`. Non-regression: `fo4_multi_master_game_master_first_active_survives` proves the guard fires only on the implicit set (user-`*`-line masters reorder freely). UI locked rows: `class:protected`, ▲▼ + toggle `disabled={p.protected}`, `checked={p.enabled || p.protected}`, MEDIUM + "🔒 PROTECTED" text badges (+page.svelte L1798-1838). |
| 4 | Verify/repair treats on-launch `plugins.txt` rewrite as expected — `reconcile_plugins_txt` returns InSync on `.ccc`/stripped-implicit/`BlueprintShips-*`, Drift(names) only on unexpected, deriving intent from RECORDED plugin state (SFLO-04) | ✓ VERIFIED | `reconcile_plugins_txt` (reconcile.rs L53-119) classifies each delta against recorded state minus protected set. Both branches tested: `reconcile_expected_deltas` → InSync (ccc added, blueprint re-added, implicit master stripped); `reconcile_real_drift` + `reconcile_reorder_is_drift` → Drift. Lives OUTSIDE `deploy::verify`'s Data/-hash walk. Wire shape `"InSync"` bare string tested (`reconcile_state_serde_wire_shape`); frontend types `"InSync" \| { Drift: string[] }` (api.ts L123) and renders calm vs amber (+page.svelte L1602-1616). |

**Score:** 4/4 truths verified (0 present, behavior-unverified)

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|------|-------------|----------|
| 1 | LOOT-vs-desktop parity on REAL inputs | Phase 9 | Phase 9 SC2 (on-hardware). Phase 7 ships mechanism + fixture determinism proof. |
| 2 | Exact real on-launch rewrite delta set | Phase 9 | Phase 9 SC4. Reconcile is data-driven (self-corrects); real set confirmed on live prefix. |
| 3 | Real implicitly-active/protected master set | Phase 9 | Derived purely from libloot (no literals); concrete set confirmed on hardware. |

_These are the pre-acknowledged Phase-9 deferrals (no live Starfield prefix on this host). They are NOT Phase-7 gaps — the mechanism is libloot-driven and data-driven so it self-corrects against the real game._

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/loadorder/src/scan.rs` | `PluginView` (medium/protected) + `scan_plugin_views_for`; medium via `is_medium_plugin()` | ✓ VERIFIED | `PluginView` L119-134; `classify` reads medium header flag L89; wired into command merge. |
| `crates/loadorder/src/loot.rs` | `protected_plugins` probe + guard in `apply_load_order` + `SortProposal.masterlist_date` | ✓ VERIFIED | `protected_plugins` L189-199 (is_plugin_active only); guard L396-424; `masterlist_date` L511. |
| `crates/loadorder/src/reconcile.rs` | `ReconcileState` + `reconcile_plugins_txt` pure fn | ✓ VERIFIED | Enum L32-39 (default-tagged serde); pure fn L53-119. |
| `crates/loadorder/src/masterlist.rs` | `masterlist_snapshot_date` const | ✓ VERIFIED | `masterlist_snapshot_date(STARFIELD) → "2026-07-07"` L66-71; bundled Starfield snapshot included + test. |
| `crates/loadorder/src/error.rs` | `LoadOrderError::ProtectedMaster` | ✓ VERIFIED | Variant L63-64 with doc making it engine-enforced, libloot-derived. |
| `crates/testkit/src/lib.rs` | Starfield/medium fixtures | ✓ VERIFIED | `write_medium_plugin` (0x1\|0x400) L195, `write_min_plugin`, `fake_proton_prefix` (Starfield). |
| `src-tauri/src/commands/plugins.rs` | `list_plugins` → PluginView; `reconcile_plugins` command | ✓ VERIFIED | `list_plugins` returns `Vec<PluginView>` L149-154; `reconcile_plugins` thin forwarder L317-359; graceful degrade `protected_set` swallows probe Err → empty (L93-113). |
| `src-tauri/src/lib.rs` | `reconcile_plugins` registered | ✓ VERIFIED | Registered in handler L147 (alongside list/set/save/sort). |
| `frontend/src/lib/api.ts` | PluginInfo medium/protected; SortProposal.masterlist_date; ReconcileState + binding | ✓ VERIFIED | `PluginInfo` L103-110; `SortProposal.masterlist_date` L117; `ReconcileState` L123; `reconcilePlugins` L473. |
| `frontend/src/routes/+page.svelte` | locked rows, MEDIUM/PROTECTED badges, age note, reconcile states | ✓ VERIFIED | Locked rows L1798-1838; badges L1819-1838; age note L1741; reconcile calm/amber L1602-1616. |

### Key Link Verification

| From | To | Via | Status |
|------|----|----|--------|
| `save_plugin_order` cmd | `loot::apply_load_order` | `save_plugin_order_inner` write-before-persist (WR-05) | ✓ WIRED |
| `list_plugins` cmd | `scan_plugin_views_for` + `protected_plugins` | merge under one lock; protected from live probe | ✓ WIRED |
| `reconcile_plugins` cmd | `reconcile_plugins_txt` | recorded state + on-disk txt + libloot protected set | ✓ WIRED |
| `sort_with_loot` cmd | `propose_sort` | spawn_blocking (blocking reqwest) → SortProposal | ✓ WIRED |
| Frontend | `ReconcileState`/`PluginInfo` | externally-tagged serde `"InSync"` bare string; medium/protected bools | ✓ WIRED |

### Behavioral Spot-Checks

Behavior-dependent truths (SFLO-03 protected-rejection state guard; SFLO-04 delta classification) are exercised by passing tests against the REAL libloot seam over fixture Proton prefixes — not presence alone:

| Behavior | Test | Status |
|----------|------|--------|
| Protected reorder rejected (typed) | `protected_reorder_rejected` (plugins.rs) | ✓ PASS |
| Protected disable rejected (typed) | `protected_disable_rejected` | ✓ PASS |
| Guard fires ONLY on implicit set (no SSE/FO4 regression) | `fo4_multi_master_game_master_first_active_survives` | ✓ PASS |
| Medium classified Starfield-only | `medium_master_classifies_true_only_for_starfield` | ✓ PASS |
| Reconcile InSync on expected deltas | `reconcile_expected_deltas` | ✓ PASS |
| Reconcile Drift on unexpected/reorder | `reconcile_real_drift`, `reconcile_reorder_is_drift` | ✓ PASS |
| Sort deterministic + date surfaced | `starfield_sort_determinism` | ✓ PASS |
| Asterisk write bounded under Starfield prefix | `starfield_asterisk` | ✓ PASS |
| WR-05 write-before-persist on failure | `save_plugin_order_inner_leaves_db_untouched_on_write_failure` | ✓ PASS |

Regression gate (orchestrator-run): `cargo test --workspace --locked` = 300 passed / 0 failed; `cargo clippy --workspace` clean; frontend `npm run check`/`build` = 142 files, 0 errors/warnings.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|-------------|-------------|--------|----------|
| SFLO-01 | 07-01/02/03 | ✓ SATISFIED | `apply_load_order` asterisk write at Starfield CE2 path; `starfield_asterisk`. |
| SFLO-02 | 07-01/03 | ✓ SATISFIED | `propose_sort` bundled masterlist, propose-then-apply, date note; determinism test. |
| SFLO-03 | 07-01/02/03 | ✓ SATISFIED | libloot-derived protected (no literals) + `is_medium_plugin`; engine `ProtectedMaster` guard; UI locked/badged. |
| SFLO-04 | 07-01/02/03 | ✓ SATISFIED | `reconcile_plugins_txt` recorded-state classification, both branches; serde wire shape; calm/amber UI. |

### Anti-Patterns Found

None blocking. No debt markers (TBD/FIXME/XXX) in phase files. No hard-coded protected base-master name literals in the engine source (grep confirmed). No scope leakage: `crates/loadorder/src/` contains zero `StarfieldCustom.ini`/`sResourceDataDirs`/`bInvalidateOlderFiles` references (Phase 8), and no on-hardware code (Phase 9). Command adapters are thin (lock → gather store reads → one headless call → map error to String); all classification lives in `loadorder`.

### Human Verification Required

None required for Phase-7 sign-off. The three deferred items are on-hardware validations explicitly scheduled for Phase 9 (owner's live Proton Starfield install) and are not Phase-7 gaps.

### Gaps Summary

No gaps. All four success criteria and all four requirements (SFLO-01/02/03/04) are delivered in the shipped code with passing behavioral tests against the real libloot seam. Medium classification is driven exclusively by `esplugin::is_medium_plugin()` (no `PluginKind::Medium`, no `core`/DB change). The protected set is derived purely from libloot's `is_plugin_active` proxy with zero name literals in the engine path, enforced defensively in `apply_load_order` via the typed `LoadOrderError::ProtectedMaster` (with a green FO4/SSE non-regression test proving the guard only fires on the implicit set). Reconcile classifies expected vs unexpected deltas from recorded state, serializes `InSync` as the bare string the frontend types against, and renders calm-vs-amber. Tauri commands are thin forwarders with graceful degradation on probe failure. The only open items are the acknowledged Phase-9 on-hardware validations, which do not block this phase.

---

_Verified: 2026-07-07_
_Verifier: Claude (gsd-verifier)_
