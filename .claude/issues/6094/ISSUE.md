# EXT-D6-2026-10-19-04: Coarse terrain-LOD bands assume a ~6-cell streaming radius; at the default radius 12 they overlap resident full-detail cells (and legacy synth coarse blocks keep stale holes)

Labels: medium,terrain-exterior,legacy-compat,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6094

**Severity**: MEDIUM (a wrong draw - double terrain or a hole - at 11 to 13 cells; not confirmed on screen) · **Dimension**: Distant LOD and trees · **Games**: FO3, FNV (authored ladder, synth coarse blocks); Skyrim, FO4 (`.btr` coarse bands)

## Location
- `byroredux/src/cell_loader/terrain_lod.rs` (`let mask = if level == k { block_hole_mask(..) } else { 0 }` in `stream_lod_blocks`; stored `hole_mask` is 0 for coarse blocks; premise comment "~6-cell streaming radius")
- `byroredux/src/cell_loader/terrain_lod.rs` test `coarse_terrain_bands_never_reach_the_full_detail_region` (sweeps radius_unload 6 and 6..=8)
- `byroredux/src/cell_loader/terrain_lod.rs` `spawn_lod_block` (skips every cell resident at spawn time, at any level)
- `byroredux/src/scene.rs` (`DEFAULT_EXTERIOR_RADIUS = 12`), `byroredux/src/app_step.rs`

## Description
Only the finest band (level 4) has a hole mask. Coarse bands rely on being selected "far beyond the near ring (their refine thresholds start at 15 cells against a ~6-cell streaming radius)". The default exterior radius (cold start and door transitions) is 12 (hysteresis 13); `parse_exterior_radius` allows 1..=12.

- **FO3 / FNV.** `select_authored_lod_quads` has no full-detail exclusion and no hysteresis. A level-8 quad is selected once its centre is more than 12 cells away, so its nearest cell is about 10 away, inside the resident ring; a level-16 quad (threshold 18) reaches about 12. A port of the selection over a full four-level authored lattice shows coarse quads covering resident cells for 60 of 64 player positions at radius 12 (up to 141 cells). Each synth block is built by `spawn_lod_block`, which skips the cells resident at spawn time, then stores `hole_mask = 0`. `stream_lod_blocks` compares that to `mask = 0` for ever, so the block is never regenerated: cells that load afterwards are overdrawn, cells that unload afterwards leave a permanent moat in the distant ground.
- **Skyrim / FO4.** `.btr` coarse quads have no holes at all. With the band hysteresis (`sticky = -1` for a resident quad) a level-8 quad stays emitted until its centre is within 14 cells, i.e. its nearest cell is 11 away. A simulated walk records level-8 quads (and level-32 on FO4) with their nearest cell at 11, inside the radius-12 ring.
- The guard pins the premise for radius_unload 6..=8 and calls that "every playable radius" (#4505). The smoke gates (`m-exteriors.sh` boundary / static) run at `--radius 1`, so neither the guard nor the live `full_detail_overlaps` metric ever sees the default.

## Evidence
`stream_lod_blocks` computes `mask` for level 4 only; `LodBlock.hole_mask` for coarse synth blocks is `0`; `find_terrain_full_detail_overlaps` only treats level-4 cells as holed (`lod_coverage.rs`), so the runtime backstop would report these overlaps, but only when run at the default radius. Disproof attempted: the LOD terrain is `RenderLayer::Architecture` (zero bias, same as near terrain) and only its TLAS membership differs, so nothing biases it behind the near mesh; the load set is the full Chebyshev square (`compute_streaming_deltas`). Static analysis plus simulation; not confirmed at runtime.

## Impact
At 46 to 53 thousand BU, near terrain and a decimated LOD mesh occupy the same ground (z-fight or a LOD sheet poking through), and on FO3 / FNV a stale hole can leave the distant ground missing next to a cell that unloaded.

## Related
#4505 (closed, the test-only guard), #1866 / #1871 (the finest-band hysteresis fix), #6053 (same legacy selection function; root cause of its 384 overlaps is the immediate-parent seam test in `select_authored_lod_quads`).

## Suggested Fix
Exclude quads that intersect `resident_full_cells` from the coarse bands the way the object ring already does (`quad_intersects_full_detail` in `stream_object_lod_blocks`), or hole the coarse mesh the way the finest band does (and then track a coarse mask so a residency change regenerates the block). Extend the guard to the default radius (`radius_unload` 13) and run the live `lod.coverage` check at radius 12 on FNV and Skyrim.

Source: docs/audits/AUDIT_EXTERIOR_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
