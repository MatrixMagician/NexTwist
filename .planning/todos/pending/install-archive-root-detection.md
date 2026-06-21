# Install archive root-detection (mod deploys double-nested)

**Found:** 2026-06-21, during UAT-1 in-game test (Fallout 4).
**Severity:** major (breaks "install a mod and it loads" for archives with a wrapper folder).
**Scope:** likely Phase 4 (Guided Installers & Collections) or a Phase 1 staging follow-up — NOT the Phase 2 loadorder fix.

## Problem
Mod archives with a top-level wrapper folder (e.g. `Super Cheat Legendary Weapon Fountain/Data/...`)
are staged and deployed verbatim, so the plugin lands at
`Data/<Wrapper>/Data/Plugin.esp` (double-nested) and non-game files (`Info.txt`, `Screenshot/`)
are copied into the game `Data/`. The Creation Engine only loads `Data/*.esp`, so the mod never
loads. Reversibility is intact (tracked in `deployed_file`), but the placement is wrong.

## Fix direction
At install/stage time, detect the mod's effective root (the folder containing `Data/`, or a FOMOD
`ModuleConfig.xml`) and map its CONTENTS onto the game `Data/`, excluding non-game wrapper files.
Mirror Vortex/MO2 "fixup" / FOMOD root detection.
