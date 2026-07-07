# Project Research Summary

**Project:** NexTwist — v1.1 Starfield (Creation Engine 2) Support
**Domain:** Adding a new Bethesda game to an existing Rust/Tauri Linux/Proton mod manager
**Researched:** 2026-07-07
**Confidence:** HIGH (stack + integration verified against source and registry); MEDIUM on the CE2 INI/loose-file specifics, which are gated on owner's on-hardware verification.

## Executive Summary

Adding Starfield is a **data-and-mapping extension, not a re-architecture.** The v1.0
engine already contains everything structurally required: `core::Game` is a plain
appid-keyed struct (no per-game enum), the SQLite store is fully appid-generic (**no
refinery migration**), and the two hard external libraries already ship Starfield
support in the exact versions v1.0 pins — `libloot 0.29.5` exposes
`GameType::Starfield` and `esplugin 6.1.4` exposes `GameId::Starfield` (with the CE2
medium-master semantics). **Zero MSRV movement, zero dependency bump required.** The
only genuinely new crate is a small INI editor (`rust-ini 0.21`, MIT, MSRV 1.64), and
even that is optional. "Supported game" is expressed as an allow-list of `match appid`
arms duplicated across ~6 sites; adding Starfield is one arm at each plus a bundled
masterlist snapshot.

The milestone's real work — and its real risk — is CE2's two "make the mod visible
in-game" behaviors and one path fork. For a Starfield mod to actually load, NexTwist
must (a) reversibly edit `StarfieldCustom.ini` (`[Archive] bInvalidateOlderFiles=1` +
`sResourceDataDirsFinal=`) so loose files load, and (b) write an asterisk-format
`plugins.txt` via `libloot GameType::Starfield` with the base ESMs excluded. Crucially,
**both files live under the Proton prefix's `Documents/My Games/Starfield/`, NOT
`AppData/Local/`** — a new path branch the v1.0 seam does not have. A "successfully
deployed" mod is invisible in-game if either the INI edit or the correct CE2 path is
wrong, so deploy and INI activation must ship and be verified together.

The highest-risk new surface is the reversible INI edit, because it inverts an
assumption baked into the v1.0 reversibility model. v1.0 reversibility is
"backup-before-overwrite → restore original bytes," which assumes the target *existed*.
`StarfieldCustom.ini` usually does **not** exist — NexTwist creates it — so purge must
be able to **restore ABSENCE** (delete the file NexTwist created), which requires
explicit `PreExisting`-vs-`CreatedByNexTwist` provenance, plus CRLF/BOM byte-fidelity,
a surgical (never whole-file) merge that preserves the user's existing keys, and
journaled idempotency so crash-replay yields one `[Archive]` section. The good news:
the existing path-generic `backup.rs` + journal primitives cover this if reused
verbatim through a small new `crates/deploy/src/gameconfig.rs` (~120 LOC). Because
"deployed OK ≠ loaded in-game" (and Starfield has a history of loose-file regressions
after patches), on-hardware verification against the owner's real Proton install is a
required gate, not optional polish.

## Key Findings

### Recommended Stack

The stack **delta is near-zero**. Both load-order-critical crates already support
Starfield at the pinned versions, so there is no version churn, no MSRV move (stays
1.89), and no cargo-deny change. The masterlist repo `loot/starfield` publishes a
`v0.29` branch — the same `MASTERLIST_BRANCH` constant already used for SSE/FO4 — so
only a slug arm + a bundled CC0 snapshot are needed. `.ba2` v3 archives stay **opaque**
to deploy: the engine links/reverts whole files by path and never inspects archive
interiors, and conflict detection is already path-level, so no BA2 reader is added.

**Core technologies (delta only):**
- **libloot 0.29.5** (already pinned): `GameType::Starfield` present today — LOOT sort + implicit-master rules. Optional patch bump to 0.29.6 for bug fixes only; MSRV unchanged.
- **esplugin 6.1.4** (already a dep): `GameId::Starfield` present — CE2 header parsing incl. the medium-master flag. No change.
- **rust-ini 0.21** (NEW, only new dependency): surgical `StarfieldCustom.ini` `[Archive]` key merge. MIT, MSRV 1.64, permissive deps — clean under `deny.toml`. Optional (a ~40-line hand-roll works since the ledger, not the writer, owns reversibility).
- **`.ba2` reader**: deliberately **NOT added** — opaque file, out of scope.

### Expected Features

**Must have (table stakes — the pass/fail gate for "mod loads in-game"):**
- Starfield detection under Proton (AppID 1716740) — reuse v1.0 steamlocate + prefix resolver.
- CE2 `Documents/My Games/Starfield` path resolver — **gates both** INI and plugins.txt; a different prefix subtree than v1.0's `AppData/Local`.
- Reversible `StarfieldCustom.ini` archive-invalidation edit — the headline new behavior; loose files are invisible without it.
- Asterisk `plugins.txt` at the CE2 path via `libloot GameType::Starfield`, base ESMs excluded.
- LOOT sort for Starfield (near-free on the existing seam) + verify/repair tolerant of the game rewriting plugins.txt on launch.
- Reversible deploy of Starfield mods — the reused v1.0 engine, unchanged.

**Should have (competitive, aligned with the safety Core Value):**
- INI edit under the **same journaled + byte-for-byte reversibility guarantee** as deployment — the differentiator no other Linux manager offers.
- "Will this load?" pre-flight check (archive-invalidation off / missing master).
- Medium-master tier surfaced in the load-order UI (display-only; esplugin already classifies it).

**Defer (v2+):**
- BA2-packed-mod detection (register/unpack orphan BA2s) — ~95% of mods are loose files.
- Blueprint-plugin ordering nuances; broader CE2 games as Bethesda ships them.

**Explicit anti-features (scope guard):** no Xbox/Game Pass support (Steam/Proton only), no Creation Kit / paid-Creations integration, no authoring `Starfield.ccc` (the game owns it), no editing `StarfieldPrefs.ini`, no BA2 repacking, and do NOT "fix" plugins.txt after the game legitimately rewrites it.

### Architecture Approach

No `core` change, no DB migration. Starfield is added as ~6 allow-list `match appid`
arms (steam resolve/discover, loadorder loot/scan/masterlist, and the Tauri
`appid_for_domain`) plus a bundled `assets/starfield/masterlist.yaml`. The one new
capability — reversible INI management — reuses the **path-generic** `backup.rs` +
journal primitives verbatim via a new sibling module, gated behind `is_starfield` and
wired at a single choke point in the engine so every entry point (deploy,
deploy_winners, purge, profile switch, collections, recover_on_launch) inherits it
without per-caller edits. The Data/ deploy-root path guard is left untouched; the INI
rides on its own tiny activate/deactivate path with a sentinel key.

**Major components:**
1. **Allow-list mappings (~6 sites)** — register AppID 1716740 → GameType/GameId/slug/folder/exe/nexus-domain.
2. **CE2 path resolver** — new `steam::my_games_path(prefix, "Starfield")` sibling of `appdata_local_path`, reusing Wine case-folding (`casing.rs`); handles Documents redirection + folder-absent-until-first-launch.
3. **`crates/deploy/src/gameconfig.rs`** (~120 LOC) — reversible INI activate/deactivate reusing `backup::backup_vanilla_if_absent` / `restore_vanilla` + journal; a `pre_existing` bool drives restore-vs-delete on purge.
4. **loadorder** — unchanged machinery; Starfield selected purely by the mapping arms; `reconcile_order` already defers the early-loader prefix to libloot.

### Critical Pitfalls

1. **Purge must restore ABSENCE, not just content** — NexTwist typically *creates* the INI, so purge must DELETE it (and clean the empty `My Games/Starfield` dir it made). Requires explicit `PreExisting{hash}` vs `CreatedByNexTwist` provenance; extend the `DIR_SENTINEL` assertion to lock both branches. *(Phase 8)*
2. **Never whole-file-write the INI (surgical merge)** — users have hand-tuned `StarfieldCustom.ini` (`[Display]`, `[Controls]`, existing `[Archive]` keys). Parse → set only the two owned keys → re-serialize preserving everything else; if `sResourceDataDirsFinal` already has a user value, surface a conflict, don't clobber. *(Phase 8)*
3. **CRLF/BOM byte-fidelity** — restore must replay stored original bytes, never a re-serialized version; preserve line-ending/encoding on the forward edit (Bethesda INIs are CRLF, no BOM). *(Phase 8)*
4. **Idempotent, journaled INI op** — deploy re-runs (redeploy, profile switch, crash-replay); use set-key (find-or-create), never append, so N runs = identical bytes and one `[Archive]` section. Back up exactly once, gated on provenance. *(Phase 8)*
5. **Never touch protected base masters** — `Starfield.esm`, `Constellation.esm`, `OldMars.esm`, `BlueprintShips-Starfield.esm`, and the `SFBGS0xx.esm` update masters (the set **grows with patches**, plus `BlueprintShips-*` are stripped/auto-activated by the game). Delegate the implicit-active + medium-tier determination to **libloot**; never hard-code the list; lock/grey them in the UI. *(Phase 7)*
6. **Wrong Proton-prefix INI path** — the file is inside `compatdata/1716740/pfx/...Documents/My Games/Starfield`, may not exist until first game launch, and needs Wine case-folding + `user.reg` Documents-redirection handling. Reuse the v1.0 steam/casing seam; never a Linux-side `~/Documents` path. *(Phase 6)*
7. **"Deployed OK != loaded in-game"** — Starfield's loose-file mechanism has regressed across patches (Vortex removed its loose-file feature after ~1.10.31.0). Record which game build the INI recipe was validated against, warn on version drift, and require an on-hardware in-game check. *(Phase 9 / drift warning Phase 6)*

## Implications for Roadmap

Research points to a clean 4-phase sequence continuing from v1.0 (Phase 6+). Phases 6->7
and 6->8 fan out from detection; Phase 9 is a gate that both feed.

### Phase 6: Starfield Detection & CE2 Path Resolution
**Rationale:** Nothing can be managed or activated until the game is detected and the CE2 `My Games/Starfield` path resolves correctly under Proton. It blocks both 7 and 8.
**Delivers:** AppID 1716740 registered across the ~6 allow-list sites; name/exe; `assets/starfield/masterlist.yaml` bundled; new `steam::my_games_path` resolver with Wine case-folding, Documents-redirection, and folder-absent (first-launch) handling; game-version-drift signal.
**Addresses:** Starfield detection + CE2 AppData path resolver (table stakes).
**Avoids:** Pitfall 6 (wrong/absent prefix path, casing).

### Phase 7: Starfield Load Order
**Rationale:** With a resolved `Game`, plugin scan -> LOOT sort -> asterisk `plugins.txt` reuses the v1.0 seam; only the mapping arms + masterlist are new.
**Delivers:** `plugins.txt` at the CE2 path via `GameType::Starfield`, base ESMs excluded; Starfield-masterlist sort; medium-master tier classified via esplugin; verify/repair tolerant of the game rewriting plugins.txt.
**Uses:** libloot 0.29.5, esplugin 6.1.4 (no bump).
**Avoids:** Pitfall 5 (protected masters), Pitfall 7 (medium tier), Pitfall 9 (masterlist currency).

### Phase 8: Reversible StarfieldCustom.ini Activation (HIGHEST RISK)
**Rationale:** The one genuinely new safety-critical capability; lands after 7 so its lifecycle hook is exercised against a working Starfield deployment. Warrants its own SECURITY.md and a dedicated testkit reversibility suite.
**Delivers:** `crates/deploy/src/gameconfig.rs` (activate/deactivate) reusing `backup.rs` + journal, wired at the single engine choke point; provenance-driven restore-vs-delete; surgical CRLF/BOM-preserving merge; idempotent + crash-replay safe.
**Implements:** the reversible-INI architecture component.
**Avoids:** Pitfalls 1-4 (absence-restore, clobbering, byte-fidelity, idempotency).

### Phase 9: On-Hardware In-Game Verification (GATE)
**Rationale:** Not code — it validates the MEDIUM-confidence CE2 assumptions (exact INI keys, loose-file loading, `.ba2` v3) that 7-8 encode. The owner's live Proton Starfield install is the only way to close them.
**Delivers:** Proof a real Nexus Starfield loose-file + plugin mod deploys AND is **visible in-game**; purge leaves game + INI pristine; sort agrees with LOOT desktop; post-patch regression sanity.
**Avoids:** Pitfall 8 (deployed-vs-loaded gap, post-update regression).

### Phase Ordering Rationale
- 6 blocks everything (path + detection gate both INI and plugins.txt).
- 7 and 8 are independent after 6 but 8 is sequenced after 7 to hook a working deploy (lower integration risk than wiring the INI hook blind).
- 9 gates on 7 AND 8 because in-game visibility requires both the plugin activation and the loose-file INI edit to be correct together.

### Research Flags

Phases likely needing deeper research / careful planning:
- **Phase 8:** highest-risk surface — reversible absence-restore + provenance + byte-fidelity + journaled idempotency on a safety-critical path. Give it its own SECURITY.md and reversibility test suite. (`/gsd-plan-phase --research-phase 8` warranted.)
- **Phase 9:** on-hardware; the exact CE2 INI keys have been finicky across patches — treat the recipe as validated-against-a-build, not a constant.

Phases with standard/well-understood patterns (lighter research):
- **Phase 6:** mapping arms + a path resolver that mirrors the existing `appdata_local_path` seam.
- **Phase 7:** reuses the v1.0 libloot sort/apply machinery verbatim; only new inputs are `GameType::Starfield` + slug.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Crate versions + Starfield enum variants verified against crates.io/docs.rs; zero bump / zero MSRV move confirmed. |
| Features | MEDIUM-HIGH | Load-order facts from Ortham (libloot author, authoritative); INI/loose-file recipe from Nexus/modding.wiki consensus; Proton paths MEDIUM. |
| Architecture | HIGH (registration/load order) / MEDIUM (reversible INI) | Extension points read directly from source; INI primitives verified reusable, but exact CE2 keys need on-hardware confirmation. |
| Pitfalls | HIGH (load order) / MEDIUM (loose-file regression, prefix folder lifecycle) | Load-order rules authoritative; regression history and first-launch folder behavior are community/inference. |

**Overall confidence:** HIGH for the plan shape and integration; MEDIUM on the CE2 loose-file specifics, which is precisely why Phase 9 exists.

### Gaps to Address
- **Exact CE2 loose-file INI keys / behavior** — validate on the owner's real Proton prefix before locking the merge logic (Phase 9; recipe belongs in one place with build-validation notes).
- **CE2 plugins.txt location** — STACK/FEATURES/PITFALLS agree it is `Documents/My Games/Starfield` (not `AppData/Local`); the ARCHITECTURE doc's one `AppData/Local/Starfield` mention for plugins.txt should be reconciled to `My Games` and **verified on hardware**.
- **Proton `My Games/Starfield` folder lifecycle** — may not exist until first launch; detect and guide "launch once" rather than silently writing nowhere useful (Phase 6).
- **Masterlist currency** — bundle a fresh CC0 snapshot with an age indicator / refresh path; SFBGS update masters grow per patch (Phase 7).

## Sources

### Primary (HIGH confidence)
- crates.io / docs.rs — libloot 0.29.5/0.29.6 `GameType::Starfield`, esplugin 6.1.4 `GameId::Starfield`, rust-ini 0.21.3 (MIT, MSRV 1.64); GitHub `loot/starfield` `v0.29` branch.
- Direct in-repo source — `crates/{core,store,steam,loadorder,deploy}`, migrations V1-V5, `src-tauri/commands/*` (extension points, path seam, `with_local_path` invariant).
- Ortham (LOOT/libloot author) blog — Starfield load order, implicit masters, medium-master tier, `FDxxyyyy`, BlueprintShips stripping, `.ccc` semantics.
- NexTwist PROJECT.md / CLAUDE.md — v1.0 journal, vanilla-backup ledger, `DIR_SENTINEL`, casing/prefix seam, milestone scope.

### Secondary (MEDIUM confidence)
- Nexus mods/273, articles/116/578; modding.wiki loose-file page — canonical `[Archive]` INI recipe.
- ProtonDB / Steam Deck guides — `compatdata/1716740/.../Documents/My Games/Starfield` path.
- ModOrganizer2 issue #2051 (protected-master UI-locking); Nexus `game-starfield` Vortex extension (loose-file regression history).

---
*Research completed: 2026-07-07*
*Ready for roadmap: yes*
