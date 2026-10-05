# #5334: EXT-D5-2026-10-05-01: The distant-water hole is the `radius_unload` disc, but full-detail cells are only guaranteed out to `radius_load` — entry and the leading edge of travel leave a waterless ring at `radius_load + 1`

**Labels**: medium,terrain-exterior,water,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5334

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-05.md` — `EXT-D5-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: MEDIUM (a coverage gap built in by construction, the same class and severity as #5244)
- **Dimension**: Water translation (WATAL); this is the distant-water seam
- **Location**:
  - `byroredux/src/cell_loader/water.rs:851-865` (`distant_water_hole_radius` and its doc); `:932` (`if distance <= hole_radius … continue`).
  - `byroredux/src/streaming.rs:1995-2040` (`compute_streaming_deltas`: the desired set is `-radius_load..=radius_load`; a cell is unloaded only when `d > radius_unload`); `:907` (`radius_unload: radius_load + 1`).
  - Contrast `byroredux/src/cell_loader/terrain_lod.rs:113-118` (`cell_is_full_detail`, set-based) and its test `only_actual_residency_is_holed_from_lod` (`:1056`).
- **Status**: NEW. #5244 (CLOSED) is incomplete: its fix rests on a premise the terrain-LOD code had already disproved.
- **Tier Violated**: n/a (coverage)
- **Game Affected**: every game with distant LOD water (FO3, FNV, Skyrim, FO4)
- **Description**:
  - The new doc says "Full-detail water exists for loaded cells (Chebyshev ≤ `radius_unload`)", so the distant mesh skips `≤ radius_unload` and the two sets are "contiguous by construction".
  - Streaming never requests a cell beyond `radius_load`. A cell at exactly `radius_unload` is resident only if it was inside `radius_load` earlier and has not yet left `radius_unload`.
  - At worldspace entry (and after a teleport or load), the whole ring at `radius_load + 1` is not resident. It is still inside the hole, so neither water source covers it.
  - After a grid crossing, the same holds for the leading edge and for the lateral cells that were never within `radius_load`. Only the trailing edge keeps hysteresis residents there.
  - Terrain does not have this gap. Synthesized LOD blocks hole out only actually-resident cells, so LOD terrain (the lake bed) is drawn in that ring with no water over it.
- **Evidence**:
  - `streaming.rs:2009-2014`: `for dx in -radius_load..=radius_load { for dy in -radius_load..=radius_load { desired.insert(...) } }`.
  - `terrain_lod.rs:113-115`: "This is deliberately set-based: `radius_unload` bounds possible hysteresis residency but does not populate its outer ring."
  - #5244's live evidence ("a settled `water.dump` at grid (19,13) shows 40-48 planes at rings 0-4", `--radius 3`) fits a 7 × 7 load. 49 cells is rings 0–3; rings 0–4 would be 81.
  - The unit guard `distant_water_hole_is_exactly_the_streaming_boundary` asserts only `distant_water_hole_radius(4) == 4`, so it pins the premise rather than testing contiguity.
  - The acceptance gate cannot see this: `m-exteriors.sh … water` asserts only `Water dump: planes=` and no-sentinel (`:526-533`), and #5243's own issue notes that its frozen poses are near-field.
- **Impact**:
  - A one-cell (4096 BU) dry band in distant lakes, rivers and coast at `radius_load + 1` on entry, and on the forward side whenever the player moves.
  - At the default radius it sits ~16–20k BU out, inside the normal view. It shows where LOD terrain dips below the water line (Lake Mead, Potomac, Skyrim coast).
- **Related**: #5244 (closed, incomplete); #5243; PERF-D7-2026-10-05-02 (same rebuild path).
- **Suggested Fix**:
  - Hole the distant mesh by the actual resident set, as `block_hole_mask` does. Pass `state.loaded`'s key set to `build_distant_water_mesh` and skip only resident cells.
  - Trigger the rebuild on residency change, not only on `center_grid` change. Or do it in the same reconcile that regenerates terrain-LOD hole masks.
  - Replace the radius guard with a set-based contiguity test: every non-resident wet cell within reach gets a quad.
  - Add a distant-water assertion (quad count at a fixed pose) to `m-exteriors.sh water`.

## Publisher note

Incomplete fix of the closed #5244. When fixing, cache the per-cell projection (`distant_water_cells`) at worldspace entry as well — residency-change rebuilds would otherwise multiply the per-crossing cost tracked in #5289 (PERF-D7-2026-10-05-02).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
