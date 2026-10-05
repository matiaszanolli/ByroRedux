# #5289: PERF-D7-2026-10-05-02: LOD-water recentering became a full mesh rebuild with blocking uploads at every grid crossing, re-folding every worldspace cell's 33×33 heightmap

**Labels**: medium,performance,water,terrain-exterior,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5289

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-05.md` — `PERF-D7-2026-10-05-02` (HEAD `a2c24b16e`)

- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**:
  - `byroredux/src/streaming.rs:989-1020` (`recenter_lod_water`), called from `byroredux/src/app_step.rs:114-129` on every `grid_changed`
  - `byroredux/src/cell_loader/water.rs:1202-1276` (`rebuild_lod_water_mesh`), `:982-997` (`distant_water_cells`), `:911` (`build_distant_water_mesh`)
  - `crates/renderer/src/mesh.rs:804-858` (`upload_scene_mesh`)
- **Status**: NEW. It arrived with `98061ec58` (#5243) and `3e258fa36` (#5244), both closed correctness fixes. Before them, recentering translated the plane's `Transform` by the grid delta.
- **Description**: at each boundary crossing, `rebuild_lod_water_mesh`:
  1. Calls `distant_water_cells(cells)`. That walks **every** exterior cell of the worldspace (`record_index.cells.exterior_cells[worldspace]`) and folds each cell's `LandscapeData.heights` (1089 f32) for `land_min`, collecting a fresh Vec. The projection depends only on session-invariant record data.
  2. Rebuilds the ring geometry.
  3. Uploads it with `upload_scene_mesh`. Its per-mesh buffers go through `create_device_local_buffer` → `with_one_time_commands`: a blocking submit and fence wait, about "2 synchronous fence-waits" per upload according to `upload_scene_mesh_global_only`'s own doc at `mesh.rs:850-858`.
  4. Appends to the global vertex/index pool and drops the previous mesh.
- **Evidence**: `water.rs:1214-1216` and `:1246-1248`; `streaming.rs:1010-1019`.
- **Impact**: main-thread work at every crossing, outside the `FrameTimeBudget` apply slices:
  - O(worldspace cells × 1089) min-folds, about 11 M f32 for Tamriel's roughly 10 k LAND cells (estimated, unmeasured);
  - one Vec of every cell;
  - about two GPU round-trip waits;
  - global-pool churn that waits for compaction.
  
  It falls on exactly the boundary-cross frame that the streaming work (#3659, #4810 and the prefetch pipeline) was built to keep flat. No quantitative guard exists for this site.
- **Related**: #5243, #5244; skill Dim 7 boundary-cross stall checklist.
- **Suggested Fix**:
  - Compute the per-cell projection (grid, effective height, `land_min`) once when the plane spawns and store it on `LodWaterPlane`.
  - Rebuild only the ring geometry per crossing, through the batched/staging-pool upload. Alternatively, upload one full-reach mesh and cut the streaming-boundary hole in the shader, since the hole radius fits in a uniform.

## Publisher note

Related in the same rebuild path: **EXT-D5-2026-10-05-04** (`AUDIT_EXTERIOR_2026-10-05.md`, filed separately) — a failed rebuild retries a full-worldspace scan every frame.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
