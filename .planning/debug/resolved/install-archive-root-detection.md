---
slug: install-archive-root-detection
status: resolved
trigger: "Mod archives with a top-level wrapper folder deploy double-nested (Data/<Wrapper>/Data/Plugin.esp) so the Creation Engine never loads the mod. Found 2026-06-21 during Phase 1 UAT-1 in-game Fallout 4 test. Blocker halting v1.0 milestone close. Full repro + fix direction in .planning/todos/pending/install-archive-root-detection.md."
created: 2026-06-23
updated: 2026-06-23
resolved: 2026-06-23
resolved_by: "commit 2fa9821 (fix(extract): exclude non-game wrapper siblings from staged mod root)"
human_verify: "Deferred — user chose 'trust the suites, commit now'. Automated round-trip proves correct placement (Data/Plugin.esp) + junk exclusion + reversibility; in-game spot-check with a wrapper-folder mod recommended post-fix (placement now identical to a non-wrapper mod, which UAT-3 already confirmed loads in-game)."
severity: major
blocks: v1.0-milestone-close
---

# Debug: install-archive root-detection (mod deploys double-nested)

## Symptoms

- **Expected:** Installing a mod archive whose top-level entry is a wrapper folder
  (e.g. `Super Cheat Legendary Weapon Fountain/Data/Plugin.esp`) stages and deploys so the
  plugin lands at `Data/Plugin.esp` and loads in-game. Non-game wrapper files
  (`Info.txt`, `Screenshot/`, readme) are excluded from the game `Data/`.
- **Actual:** The wrapper folder is staged and deployed VERBATIM, so the plugin lands at
  `Data/<Wrapper>/Data/Plugin.esp` (double-nested) and non-game files are copied into the
  game `Data/`. The Creation Engine only loads `Data/*.esp`, so the mod never loads.
- **Error messages:** None — no crash, no error. Silent wrong placement; the mod simply does
  not appear in-game. Reversibility is intact (every placed file is tracked in `deployed_file`),
  but the placement is wrong.
- **Timeline:** Found 2026-06-21 during Phase 1 UAT-1 in-game test (Fallout 4). Present since the
  Phase 1 staging path was built — archives that happen to be `Data/`-rooted work; wrapper-folder
  archives (a very common Nexus layout) do not.
- **Reproduction:** Install a mod archive whose top-level entry is a single wrapper directory that
  *contains* `Data/` (rather than `Data/` at the archive root) → Deploy → inspect the game `Data/`:
  the plugin is at `Data/<Wrapper>/Data/...` instead of `Data/...`.

## Current Focus

hypothesis: PARTIALLY FIXED already. Commit 8bd99c9 (feat 04-01) added `detect_archive_root` in
  crates/extract/src/staging.rs which DOES strip a single cosmetic wrapper dir when it directly
  contains a recognized root (Data/SKSE/...). This fixes the Data/<Wrapper>/Data double-nesting.
  REMAINING GAP per guardrail #1/#3: when the wrapper is unwrapped, its NON-GAME sibling files
  (Info.txt, Screenshot/, readmes, fomod/ config dir) are moved into staging alongside Data/ and
  then deploy into the game install root next to Data/ — they are NOT excluded. The fix must exclude
  recognized wrapper-junk so only game-relevant content (Data/ + recognized roots) is staged.
test: Add a wrapper-folder fixture (Wrapper/Data/Plugin.esp + Wrapper/Info.txt) and assert the
  staged tree is Data/Plugin.esp with Info.txt EXCLUDED.
next_action: Implement wrapper-junk exclusion in extract::staging — when a wrapper is unwrapped,
  stage ONLY the recognized-root children (Data/, SKSE/, ...) and drop non-game siblings.
expecting: After fix, staged tree = Data/Plugin.esp only; round_trip_pristine + crash_recovery green.

reasoning_checkpoint:
  hypothesis: "The non-FOMOD plain-archive staging path leaks non-game wrapper siblings (Info.txt,
    Screenshot/, readmes, fomod/) into the game Data/ directory. Cause: detect_archive_root unwraps
    a cosmetic wrapper dir but move_into_staging then moves the wrapper's ENTIRE contents; deploy's
    resolve_target re-roots any non-Data/-prefixed relpath under <install>/Data, so the junk lands
    in Data/."
  confirming_evidence:
    - "staging.rs detect_archive_root returns the wrapper dir; move_into_staging renames it whole — no per-child filtering."
    - "deploy/lib.rs resolve_target: a relpath without leading Data/ falls through to root.join(staged_rel) = <install>/Data/<junk>."
    - "Unit tests assert ONLY non-double-nesting (wrapper_folder_is_flattened) — none assert junk exclusion, so the leak is uncovered."
  falsification_test: "After staging the Wrapper/Data/Plugin.esp + Wrapper/Info.txt fixture, if
    Info.txt is STILL present in StagedMod.files (or under staging_root), the hypothesis/fix is wrong."
  fix_rationale: "Exclude at the unwrap boundary: when a wrapper is detected, stage only its
    recognized-root children (Data/, SKSE/, F4SE/, ...) and drop the rest. This addresses the root
    cause (wrong effective-root content selection at stage time) — NOT a deploy-side symptom patch.
    Loose-file / already-Data-rooted / multi-folder mods take the NO-unwrap path and are untouched,
    so legitimate non-wrapper layouts keep every file (guardrail #6)."
  blind_spots: "A wrapper that legitimately ships a loose .esp as a sibling of Data/ would have that
    .esp dropped — but that layout is non-standard (Bethesda content belongs in Data/) and mirrors
    Vortex/MO2 fixup which also treats Data/-siblings as documentation. FOMOD path is separate and
    already verified (04-UAT Test 1) so it is out of scope here."

## Evidence

- timestamp: 2026-06-23 — Pre-existing diagnosis captured in
  `.planning/todos/pending/install-archive-root-detection.md` (severity major; problem + fix
  direction). FOMOD apply path already verified to avoid `Data/` double-nesting in Phase 4 UAT
  (04-UAT.md Test 1 PASS) — so the defect is specifically the non-FOMOD plain-archive staging path.

- timestamp: 2026-06-23
  checked: crates/extract/src/staging.rs (git log: detect_archive_root added in commit 8bd99c9,
    "feat(04-01) archive root-detection", dated 2026-06-21 — the SAME day UAT-1 found the bug).
  found: `install_archive` already calls `detect_archive_root` between validate and move. That fn
    DOES strip a single cosmetic wrapper dir when the dir directly contains a recognized root
    (Data/SKSE/F4SE/...). So the `Data/<Wrapper>/Data` DOUBLE-NESTING is already fixed and unit
    tested (wrapper_folder_is_flattened, etc).
  implication: The pure double-nesting half of the symptom is resolved. The detection moves the
    ENTIRE wrapper CONTENTS though — including non-game siblings.

- timestamp: 2026-06-23
  checked: crates/deploy/src/lib.rs::resolve_target + deploy_root, crates/deploy/src/engine.rs::
    deploy_inner/deploy_one_file.
  found: deploy maps each staged relpath onto the game install root. `resolve_target` strips a
    leading `Data/` segment and re-roots under `<install>/Data`. CRUCIAL: a relpath WITHOUT a
    leading `Data/` segment (e.g. `Info.txt`, `Screenshot/x.png`) falls through to
    `root.join(staged_rel)` = `<install>/Data/Info.txt`, `<install>/Data/Screenshot/x.png`.
  implication: After the wrapper is unwrapped, the non-game siblings (Info.txt, Screenshot/,
    readmes, fomod/) are staged and then LEAK into the game `Data/` directory. This is the
    remaining half of the defect and exactly what guardrail #1/#3 requires excluding.

- timestamp: 2026-06-23
  checked: round_trip_pristine semantics (testkit snapshot_tree + assert_trees_identical) and
    deploy::purge (manifest-driven, restores byte-for-byte).
  found: Reversibility is per-file-manifest driven, so even the leaked Info.txt is tracked and
    purged. The defect is WRONG PLACEMENT, not lost reversibility. Excluding the junk at stage
    time keeps the manifest/purge invariants untouched (fewer files staged → fewer deployed).
  implication: The fix belongs entirely in extract (stage time); deploy/purge/journal need no
    change. round_trip_pristine + crash_recovery must stay green by construction.

## Eliminated

(none yet)

## Resolution

root_cause: |
  Two-part defect in the non-FOMOD plain-archive staging path (crates/extract/src/staging.rs).
  Part A (DOUBLE-NESTING) was ALREADY fixed in commit 8bd99c9: detect_archive_root strips a single
  cosmetic wrapper directory when it directly contains a recognized game root (Data/, SKSE/, F4SE/,
  ...), so `Wrapper/Data/Plugin.esp` no longer staged as `Data/Wrapper/Data/Plugin.esp`.
  Part B (NON-GAME FILE LEAK) was the remaining live bug: after unwrapping, move_into_staging moved
  the wrapper's ENTIRE contents into the staging root — including non-game siblings (Info.txt,
  Screenshot/, readmes, fomod/ config dir). deploy::resolve_target (crates/deploy/src/lib.rs)
  re-roots any staged relpath WITHOUT a leading `Data/` segment under `<install>/Data`, so those
  junk siblings leaked into the live game Data/ directory at deploy time.
fix: |
  Made the wrapper unwrap content-selective in the extract crate (stage time only; deploy/journal/
  purge untouched). detect_archive_root now returns a MoveSource plan:
    - Whole { root }           — no wrapper (already-Data-rooted, loose-file, or multi-folder mod):
                                 stage the validated tree VERBATIM, drop nothing (guardrail #6).
    - WrapperChildren {..}     — a cosmetic wrapper was detected: stage ONLY its recognized-root
                                 children (Data/, SKSE/, F4SE/, ...) and drop every non-game sibling.
  move_into_staging consumes the plan: it renames the whole tree for Whole, or moves only the
  selected children for WrapperChildren (atomic-rename fast path with a recursive-move cross-device
  fallback for each). Recognized-root children are collected sorted for deterministic staging.
  Errors stay thiserror (ExtractError::io) — no anyhow, matching the engine style.
verification: |
  - cargo test -p nextwist-extract: 8 unit + 4 integration + 4 zip-slip tests pass, incl. new
    regression `wrapper_folder_stages_data_root_and_excludes_non_game_files` (full install_archive
    round-trip: Wrapper/Data/Plugin.esp + Wrapper/Info.txt + Wrapper/Screenshot/ ⇒ staged tree is
    exactly [Data/Plugin.esp], read-only; Info.txt and Screenshot/ excluded) and unit
    `wrapper_non_game_siblings_are_excluded` + `wrapper_keeps_all_recognized_roots` +
    `loose_file_mod_is_kept_verbatim`.
  - cargo test -p nextwist-deploy: round_trip_pristine (3), crash_recovery (2), vanilla_restore (2),
    conflict_redeploy (3), profile_switch (3), verify_drift (8) — all green: reversibility +
    byte-for-byte pristine purge + crash-recovery journaling intact.
  - cargo test --workspace --locked: 0 failures across every crate (incl. FOMOD corpus 23/23 — the
    FOMOD path does NOT route through install_archive so it is unaffected, consistent with 04-UAT
    Test 1).
  - cargo clippy --workspace --all-targets -- -D warnings: clean.
files_changed:
  - crates/extract/src/staging.rs  # MoveSource plan + selective wrapper-child staging; updated unit tests
  - crates/extract/tests/extract_formats.rs  # new wrapper-folder regression test + build_zip_from helper
