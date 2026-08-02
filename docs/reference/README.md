# Reference documents

Background research and UI contracts that the code cites directly. These are **historical
records of decisions already made**, not live planning documents: source comments across
`crates/` and `src-tauri/` refer to them by name (`RESEARCH Pitfall 1`, `UI-SPEC §B.2`,
`T-04-16`), which is why they are kept.

| File | What it is |
| --- | --- |
| `PITFALLS.md` | The Linux/Proton/Wine traps the engine is designed around. The numbered "Pitfall N" citations throughout the code point here. |
| `STACK.md` | Dependency choices and their rationale. `.claude/CLAUDE.md` is the condensed version. |
| `RESEARCH-ARCHITECTURE.md` | The deployment-strategy research behind the `reflink → hardlink → symlink → copy` ladder. |
| `NN-RESEARCH.md` | Per-area research: verified API behaviour, pitfalls, and the reasoning behind a subsystem's design. |
| `NN-UI-SPEC.md` | The visual/interaction contract for a UI area. The `§A`/`§B` citations in `+page.svelte` and the command adapters point here. |

The numbers are historical build order, not a roadmap:

| Prefix | Area |
| --- | --- |
| 01 | Safe local round-trip (the deploy engine + reversibility) |
| 02 | Multi-mod management (conflicts, ranks, profiles) |
| 03 | NexusMods login and download |
| 04 | Guided installers (FOMOD) and Collections |
| 05 | AppImage distribution |
| 06 | Starfield detection and CE2 path resolution |
| 07 | Starfield load order |
| 08 | Reversible `StarfieldCustom.ini` activation |

Treat these as read-only history. When a decision changes, change the code and its tests,
and record the new decision in an ADR under `docs/adr/` rather than editing the research
that justified the old one.
