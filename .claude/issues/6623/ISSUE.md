# PRDR2-2026-11-03-05: Six stale claims in ROADMAP and docs/engine (LOD scheme, BC2 cause, patrol routes, GetDayOfWeek, NAVM sub-records, menu close)

Labels: documentation, low, doc-rot

**Severity**: LOW (documentation states the opposite of HEAD)
**Dimension**: Doc rot (ROADMAP / docs/engine)
**Source**: surfaced by the round-2 reviews of the wave-1 PRDs (2026-11-03), each line re-read at HEAD `eba979dfb`
**Status**: NEW

## Sites

| # | Location | Says | Actually |
|---|---|---|---|
| 1 | `ROADMAP.md:44` | distant LOD includes "`_far.nif` on Oblivion/FO3/FNV (M35)" | FO3/FNV use `blocks\` object LOD, not `_far.nif` (#3321); #5749 corrected only the M35 row |
| 2 | `ROADMAP.md:623` (BC2 known issue) | "Suspected cause: a `dds::mip_size` rounding gap" | #4512 (`80813026a`) ruled that out; the texture-streaming PRD (TXS-UP-001, Phase 0) settles the real cause |
| 3 | `docs/engine/npc-spawn-ai-packages.md:486-488` | patrol-idle route data "this codebase decodes nowhere" | `XLKR` is decoded (`crates/plugin/src/esm/cell/walkers.rs:742-766`); Patrol does not follow it |
| 4 | `docs/engine/scripting.md:2637` | `GetDayOfWeek` is declined | it runs (`crates/scripting/src/obscript_more/extras.rs:193-194`, command `0x10aa`) |
| 5 | `docs/engine/navmesh-pathfinding.md:69` | the typed form's `NVDP`/`NVCA`/`NVGD` "aren't even walked" | `parse_navm` decodes all three (`crates/plugin/src/esm/records/misc/world.rs:379-394`, `decode_nvgd` at `:569`) |
| 6 | `docs/engine/ui.md:704-705` | closing or replacing a menu "sends the corresponding focus transition" | no close exists since #2723 deleted `UiManager::close` (`crates/ui/src/lib.rs:356-364`); only replacement moves focus (`install_player`) |

## Suggested Fix
Correct each line. ROADMAP is at its line ceiling (`workspace_hygiene_tests::readme_and_roadmap_hold_their_line_ceilings`), so rows 1-2 must be edited in place without adding lines.

## Completeness Checks
- [ ] **SITES**: all 6 rows corrected
- [ ] **CEILING**: ROADMAP line count does not grow

