# #5244 — WATAL W2-02: the distant-water hole margin cuts a permanent one-cell waterless ring at every streaming boundary

https://github.com/matiaszanolli/ByroRedux/issues/5244

Source: W2 boundary re-derivation 2026-10-04 (follow-on to #5243). Full body on the issue.

## Resolution (2026-10-04)

`distant_water_hole_radius(radius_unload)` = the streaming boundary itself, no margin: loaded full-detail covers <= radius_unload, the distant mesh emits >= radius_unload+1 — contiguous by construction. Live FNV fixture: hole=4, histogram 2600x262 -> 2600x277 (the freed ring-5 wet cells). Both m-exteriors water gates green.
**WATAL W2-02** — follow-on to #5243's per-cell distant water, found by re-deriving the streaming-boundary coverage.

- **Severity**: MEDIUM (construction-proven coverage gap; a one-cell waterless ring at every streaming boundary)
- **Dimension**: W2 distant-water coverage / seam — the hole-margin half.
- **Location**:
  - `byroredux/src/cell_loader/water.rs` — `LOD_WATER_HOLE_MARGIN_CELLS = 1` cuts distant quads at Chebyshev distance ≤ `radius_unload + 1`.
  - `byroredux/src/streaming.rs:907` — `radius_unload = radius_load + 1`; the loaded (full-detail-water) set extends to `radius_unload`.
- **Description**: with `--radius 3`, `radius_unload` is 4: full-detail water planes exist for cells at Chebyshev distance ≤ 4 (verified live: a settled `water.dump` at grid (19,13) shows 40-48 planes at rings 0-4), while the distant mesh (hole = 5) emits quads only at rings ≥ 6. **Every cell at exactly distance `radius_unload + 1` (ring 5 here — 11 wet cells around Lake Mead's fixture grid) is covered by neither.** The margin dates from the single-sheet annulus (#2449 era): with the sheet at the wrong height, overlap with near water double-blended visibly, so the annulus was cut back one cell. With #5243's per-cell heights the overlap the margin guards against is two coincident same-height quads for the few frames a freshly-distant cell takes to unload — while the margin's cost became a permanent traveling waterless ring one cell inside the distant water.
- **Evidence**:
  - Scene graph (settled engine at grid (19,13), `--radius 3`): full-detail planes at rings 0-4 only; distant-mesh spawn log `hole=5 cells`; `xclw_census.rs 19,13` per-ring counts show 11 wet cells at ring 5 that fall in neither set (mesh histogram arithmetic reconciles exactly: census rings ≥6 near-2600 = 256, +6 LAND-less deep-lake cells the mesh keeps conservatively = 262 = the live histogram).
  - Unit test `distant_water_hole_and_reach_window` pins the skip at `distance <= hole_radius` — the gap is by construction, not accident.
  - Photography from above the player grid (the only stable vantage — any closer inspection re-centers the hole) was inconclusive: the ring sits at ~20.5k BU where it lands on the extreme frame rows (fov 45°), and those rows read water-shaded. The construction proof stands regardless.
- **Suggested Fix**: drop the margin (`LOD_WATER_HOLE_MARGIN_CELLS = 0` / use `radius_unload` directly): hole skips ≤ `radius_unload`, emits ≥ `radius_unload`+1, loaded ≤ `radius_unload` — exactly contiguous. Cost: coincident same-height quads for the frames between a grid crossing and the reconcile that unloads the now-distant ring — a transient double-blend at matching heights, imperceptible next to a permanent dry ring. Update the doc comment's rationale (#1871's conservative margin was the wrong-height-era guard).

## Completeness Checks
- [x] **TESTS**: the hole-window unit test moves its skip expectation to `distance <= radius_unload` (hole-margin gone); a comment pins the contiguity contract
- [x] **LIVE**: `water.dump` histogram grows by the freed ring's wet cells at the fixture grid; both `m-exteriors.sh` water gates stay green
- [x] **DOCS**: the margin constant's history (why it existed, why it's gone) lands in the doc comment; watal.md §5.2 mentions contiguity
