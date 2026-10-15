# REN-D2-2026-10-14-03: ReSTIR ownership-claim and decal-substrate paths are unguarded; `substrateValid` restates `TEMPORAL_NORMAL_COS` as a bare `0.906`

**Labels**: low,renderer,shaders,test-gap,bug
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D2-2026-10-14-03`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `crates/renderer/shaders/triangle.frag` (`substrateValid` block: `0.906`, `max(8.0, …)`, 0.5% depth term, `max(abs(dot(geomN, V)), 0.1)`; the `if (stableTemporalSurface)` write)

## Description
`TEMPORAL_NORMAL_COS` is function-local to the temporal-reuse `if`, so the substrate block re-types its value (three `0.906` sites plus `SPATIAL_NORMAL_COS`). No `shader_contract`/`shader_constants`/`restir` test mentions `substrate`, `claimed` or `reservoirsCurr`, so dropping the `stableTemporalSurface` write gate or the `isAlphaBlend && !isGlass` substrate gate stays green. #5617 closed the same class of problem for the history cap/floor.

## Evidence
`grep -n 0.906 triangle.frag` returns three sites; grep of the three test files for those identifiers returns no hits.

## Impact
Maintenance/regression risk only. Related: REN-D2-2026-10-14-01 (do together), #5617.

## Suggested Fix
Hoist the cosines and tolerances into `shader_constants_data.rs` (generated header), and pin that blended/glass fragments never reach the `reservoirsCurr` store and that the substrate path reads `reservoirsPrev`, not `reservoirsCurr`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
