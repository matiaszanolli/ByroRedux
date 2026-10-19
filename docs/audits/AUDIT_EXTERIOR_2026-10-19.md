**HEAD**: 807c58890 (suite launched at 2a6f8be2b; the one newer commit touches the renderer shadow filter, not this area) · **Baseline**: [AUDIT_EXTERIOR_2026-10-18.md](AUDIT_EXTERIOR_2026-10-18.md) (HEAD `6f44874ac`, 39 commits since) · **Audited**: Dim 1 (EXAL boundary: `cell_loader/exterior.rs`, `app_step.rs` per-tick calls only), Dim 2 (terrain spawn + splat in `cell_loader/terrain.rs`), Dim 5 (water: `cell_loader/water.rs`, `streaming/mod.rs` LOD water), Dim 6 (distant LOD + trees, the bulk of this pass); scope=streaming-deep AREA-SCOPED · **Unchanged since baseline (skimmed)**: Dim 1 climate/weather resolution (only `2c203278f` touched it, Starfield WTHS) · **Out of area (one line each)**: Dim 3 (ground-cover renderer pipeline: out of area), Dim 4 (sky / weather / sun renderer + `systems/weather.rs`: out of area), Dim 7 (acceptance gates / harness scripts: out of area, but the gate semantics are read where a finding depends on them).

# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD), streaming-deep area, 2026-10-19

**This report is AREA-SCOPED (streaming-deep: `byroredux/src/streaming/`, `npc_spawn/`, `cell_loader/`), not a full-codebase exterior audit.** The next `/audit-exterior` must not treat it as a full baseline; the 2026-10-18 report remains the last full-scope one for Dims 3, 4 and 7. Dimensions 3, 4 and 7 were not re-run.

**Method.** One auditor, every dimension synchronously. No sub-agents, no engine launch, no capture, no Vulkan device, no game data. Read-only on code; the unrelated working-tree edits (`crates/renderer/src/source_scan.rs`, `shadow_filter.rs`, `.claude/issues/606{2,3,4}/`) were left alone. Delta scope: `git log 6f44874ac..HEAD` over the area, then a `--deep` structural read.
- Delta in area since the baseline: `a5958adf3` (#6054, LOD ring reports a capacity-starved reconcile as stopped, not settled) and `2c203278f` (#5627, WTHS instance values; touches `load_order.rs` and `env_translate/climate.rs`). The remaining 37 commits are scripting / dialogue / renderer / NPC and have no exterior-area path. Both were read in full.
- Structural read (grep first, chunks under 1500 lines): `streaming/mod.rs` (all), `streaming_helpers.rs` (all production code), `app_step.rs` `step_streaming`, `cell_loader/exterior.rs` (persistent CELL, apply job, world context), `cell_loader/terrain.rs` (splat build, spawn), `cell_loader/terrain_lod.rs`, `object_lod.rs` (spawn + stream), `tree_lod.rs` (stream), `placement_lod.rs` (stream), `lod_bands.rs` (descent + authored selection), `lod_support.rs` (budget), `lod_coverage.rs`, `legacy_lod_index.rs`, `cell_loader/water.rs` (distant water + plane), `cell_loader/unload.rs` (victim GPU-handle sweep), `spawn/authored_cover.rs`, `work_budget.rs`, `streaming/telemetry.rs` (settle path), `bench_camera.rs` (grid-cross clock).
- Two scratch simulations (plain-Python ports of `select_authored_lod_quads` and of the `select_lod_quads` descent with walking residency) live in `/tmp/audit/streaming-deep/exterior/sim_select.py` and `sim_descent.py`. They support D6-03 and D6-04 below. They are ports for reasoning, not new code in the repo; the suite deletes the directory.

**Test state (HEAD, rustc 1.96.0 pinned toolchain, `TMPDIR=/mnt/data/tmp`, repo `target/`):**

| Suite | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux -- env_translate terrain water lod resident_vwd climate legacy_lod default_weather tree_lod streaming stream_ unload cell_loader` | 904 passed, 0 failed, 21 ignored (game data / device) |

Not run: the `--ignored` real-data tests (tree-LOD corpus, legacy-LOD corpus pins), the GPU prefilter test, every smoke gate (`m-exteriors.sh`, `w1-water-traversal.sh`: need Vulkan + data), a live LOD or capture check of any finding below. All four new findings are static analyses plus the scratch simulations; none was confirmed on screen.

## Executive Summary

**5 findings: 0 CRITICAL · 0 HIGH · 4 MEDIUM · 1 LOW.** 4 NEW (3 MEDIUM, 1 LOW); 1 Existing (#6053, MEDIUM, root cause found here).

The delta (`a5958adf3`, `2c203278f`) is correct against its own tests, but the first one makes the distant-LOD ring report honestly that the geometry pool turned quads away without making the ring able to recover from it. The structural read of the LOD stack then found that two of its pieces rest on premises that stopped being true when the default exterior radius became 12.

- **D6-01 (MEDIUM, NEW).** Every LOD provider caches a capacity-refused upload as a permanent known miss (terrain `missing_blocks`, object / tree / placement empty sentinels). When a cell unload reopens geometry admission, nothing retries those quads while they stay in range, and nothing re-arms the stopped reconcile. The `#6054` warning ("until a cell unload frees pool space") promises a recovery the code cannot deliver.
- **D6-02 (MEDIUM, NEW).** The `Starved` arm never calls `settle_lod`, so the boundary telemetry stays "in progress" forever. The `grid-cross` / `grid-soak` bench clock freezes while a boundary is in progress, so a saturating run never advances or finishes. `LodCoverageStats.settled` is still true for the same ring.
- **D6-03 (MEDIUM, Existing #6053).** Root cause of the live "384 overlapping terrain-LOD pairs": the authored-quad seam suppression drops a finer quad only when a selected quad exactly one level coarser contains it. When the intermediate level is itself dropped, its children survive under the selected grandparent. A port of the selection reproduces the issue's numbers exactly (12 level-32 x 16 level-8 + 12 level-16 x 16 level-4 = 384).
- **D6-04 (MEDIUM, NEW).** Coarse terrain-LOD bands (8 / 16 / 32) carry hole mask 0 by the premise "they sit far outside a ~6-cell streaming radius". At the default radius 12 (unload 13) that is false on FO3 / FNV (authored ladder) and for Skyrim / FO4 `.btr` quads under band hysteresis. Legacy synth coarse blocks bake the spawn-time residency into their index buffer and never regenerate. The guard that pins the premise sweeps radius_unload 6 to 8; the smoke gates run at `--radius 1`.
- **D2-05 (LOW, NEW).** Doc rot: displaced doc blocks in `terrain.rs`, the stale radius premise in comments, an `authored_cover` return doc.

Everything else in the area was re-read and holds: the stale-payload / generation machinery, the early-`CellRoot` cancel path, the victim GPU-handle sweep, the distant-water rebuild gate, the persistent-CELL job, the terrain / tile-slot release paths on every early return, the climate and default-weather rules (including the new WSLT branch of `2c203278f`), and the tree-LOD ring teardown.

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| **single-boundary** | **Holds.** `translate_sky` / `translate_weather` / `translate_exterior_cell_lighting` still have only the `scene/world_setup.rs` callers; no `SkyParamsRes {` / `WeatherDataRes {` literal outside `env_translate/`, `components/environment.rs` and tests. `GameKind` in the area is table-shaped (`DefaultLandTexture::for_game`, `terrain_lod_layout`, `object_lod_scheme`, `combined_lod_supported`; the `terrain_lod_btr.rs:154` FO4 arm is data). No `game ==` in logic. |
| **no-fabrication** | **Holds, one soft spot.** D6-04: the "coarse bands are far outside the streaming radius" premise is an undocumented-radius assumption, not a sourced value. The lattice origin remains #2586 (open). |
| **no-leak** | **Holds.** Every early return in `spawn_terrain_mesh` releases the splat-layer refcounts; `unload_lod_water_plane`, `unload_lod_block`, `unload_object_lod_block` and `drain_streaming_state` release every handle. |
| **no-render-time-fallback** | **Holds** for the area (nothing downstream of the boundary branches on game). |

## Per-Category Matrix

✓ = holds, ~ = dented (see Findings), n/a = out of area this pass.

| Category (boundary fn) | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Findings |
|---|:---:|:---:|:---:|:---:|---|
| **Terrain / splatting** (`spawn_terrain_mesh`, `build_cell_splat_layers`) | ✓ | ✓ | ✓ | ✓ | D2-05 (doc only) |
| **Sky** (`translate_sky`) | n/a | n/a | n/a | n/a | out of area |
| **Weather / sun / climate** (`resolve_default_weather`, `apply_cell_climate_override` per-tick call) | ✓ | ✓ | ✓ | ✓ | none (`2c203278f` WSLT branch verified) |
| **Water** (`spawn_water_plane`, `spawn_lod_water_plane`, `rebuild_lod_water_mesh`) | ✓ | ✓ | ✓ | ✓ | none |
| **Ground cover** (host side: `spawn_authored_cover_templates`) | ✓ | ✓ | ✓ | ✓ | D2-05 (one doc line); GPU pipeline out of area |
| **Distant LOD + trees** (`stream_lod_blocks`, `stream_object_lod_blocks`, `select_authored_lod_quads`, `reconcile_lod_rings`) | ✓ | ~ D6-04 | ✓ | ✓ | D6-01, D6-02, D6-03, D6-04 |

## Findings

### EXT-D6-2026-10-19-01: A capacity-refused LOD upload is cached as a permanent known miss, and nothing re-arms the stopped reconcile when admission reopens
- **Severity**: MEDIUM. The distant horizon stays incomplete for as long as the quad is in range, after the cause (pool saturation) is gone. Reachable at the default radius on FNV (the #6054 route, 352 refused uploads) and on FO4, whose pool saturates first.
- **Dimension**: Distant LOD and trees
- **Location**:
  - `byroredux/src/cell_loader/terrain_lod.rs:681-685` (`missing_blocks.insert((level, qx, qy), mask)` on any `None`), and `:601-603` (a recorded miss at the same mask is skipped).
  - `byroredux/src/cell_loader/object_lod.rs:305-309` and `:519-525` (`ObjectLodBlock::empty()`; a refused sub-mesh upload is a `continue`, so a quad can also be kept with only the sub-meshes that fit).
  - `byroredux/src/cell_loader/tree_lod.rs:430-431`, `byroredux/src/cell_loader/placement_lod.rs:386-392` and `:525-531` (same shape).
  - `byroredux/src/app_step.rs:358-367` (`Starved` clears `lod_reconcile_pending`).
  - `crates/renderer/src/mesh/geometry_ssbo.rs:266-268` (admission reopens at the next compaction), `crates/renderer/src/mesh.rs:703-706,809-812` (`scene_geometry_admission_open`).
- **Status**: NEW (gap in the #6054 fix, `a5958adf3`; the known-miss caching predates it).
- **Tier Violated**: n/a
- **Game Affected**: FNV, FO3, Skyrim, FO4 (any game whose radius-12 residency saturates the 4 M-vertex pool)
- **Description**:
  - `upload_scene_mesh_global_only` fails with "scene geometry admission is closed" once the pool hits its hard cap. Each ring treats that exactly like "this quad has no asset": the terrain ring records the quad in `missing_blocks` at its current hole mask, the object / tree / placement rings insert an empty sentinel so "we don't re-extract on every boundary crossing".
  - `scene_geometry_admission_closed` is cleared at the next pool compaction after a cell unload (`geometry_ssbo.rs:268`). By then the refused quads are already sentinels. The terrain ring skips a quad whose recorded mask equals its current mask (`:601`), and coarse bands always have mask 0, so they are never retried. The object / tree / placement candidate filter is `!blocks.contains_key(coord)`, so a sentinel is never a candidate.
  - #6054 made the verdict `Starved` and cleared `lod_reconcile_pending`. Only `grid_changed`, a finished cell apply or world setup set it again, and even then the sentinels veto the retry. The #6054 warning says "the horizon is incomplete until a cell unload frees pool space"; freeing pool space does not restart anything.
  - A partially uploaded object quad (some sub-meshes refused) is stored as a normal resident block with no record that parts are missing.
- **Evidence**:
  - `stream_lod_blocks`: `else { missing_blocks.insert((level, qx, qy), mask); }` follows `block = btr_block.or_else(|| ... spawn_lod_block(..))`, whose `None` covers a refused upload (`spawn_lod_block` returns `None` from the `upload_scene_mesh_global_only` `Err` arm).
  - `stream_object_lod_blocks`: `None => { blocks.insert((level, qx, qy), ObjectLodBlock::empty()); }`.
  - `MeshRegistry::scene_geometry_admission_open()` exists and is already the gate the full-detail cell path uses (`cell_loader/spawn/mesh_instance.rs:826,830,893`); no LOD ring consults it.
- **Disproof attempted**: could the sentinels be cleared by a later `retain_resident`? Only when the quad leaves the desired set (a band switch or the player moving far enough); a stationary or slowly moving player keeps the horizon quads in range. Could `lod_reconcile_pending` be re-armed by the compaction itself? No: `grep lod_reconcile_pending` shows only `streaming_helpers.rs:1054,1151`, `app_step.rs:152`, `scene/world_setup.rs:1178,1188` as writers. Not confirmed at runtime (no engine launch).
- **Impact**: After the saturating transition (the #6054 door exit), distant terrain and objects beyond the refused set never fill in, even once the pool has room, until the player has moved far enough to change those quads' band or leave and re-enter the ring.
- **Related**: #6054 (closed, introduced `Starved`), D6-02, the full-detail analogue (a cell loaded under starvation keeps its missing terrain / meshes until it is reloaded; same class, outside this finding).
- **Suggested Fix**: Treat a capacity refusal as a retryable state, not a miss. Smallest change: before an attempt, `if !ctx.mesh_registry.scene_geometry_admission_open() { starved = true; break; }` (also saves the archive read and decode); never write a sentinel or `missing_blocks` entry from a refused attempt; re-arm `lod_reconcile_pending` on the admission closed -> open edge. Pin it with a test that drives a fake admission state through two reconciles.

### EXT-D6-2026-10-19-02: The `Starved` verdict never settles the boundary telemetry; the grid-cross bench clock freezes and the `lod-coverage` gate still reads `settled=1`
- **Severity**: MEDIUM. Dev / harness path only, but it is the acceptance harness, and the hang is introduced by the fix for #6054.
- **Dimension**: Distant LOD and trees (and the Dim 7 gate semantics)
- **Location**:
  - `byroredux/src/app_step.rs:338-367` (only the `Settled` arm calls `state.telemetry.settle_lod`).
  - `byroredux/src/streaming/telemetry.rs:176-178,295-312,315-321` (`boundary_in_progress`, `settle_lod`, `clear_completed_boundary`).
  - `byroredux/src/bench_camera.rs:334-350` and `byroredux/src/app_step.rs:699-720`, `byroredux/src/app_events.rs:1192-1200` (the logical clock).
  - `byroredux/src/streaming_helpers.rs:262-274` (`update_lod_coverage(world, state, complete)` / `coverage.settled = complete`).
- **Status**: NEW (introduced by `a5958adf3`).
- **Tier Violated**: n/a
- **Game Affected**: FNV, FO4 (any saturating radius); bench harness only
- **Description**:
  - `begin_boundary` opens `ActiveBoundaryTelemetry { full_detail_settled: false, lod_settled: false }`. It is cleared only when both flags are true; `lod_settled` is set only by `settle_lod`, which `app_step.rs` calls solely in the `Settled` arm.
  - `Starved` clears `lod_reconcile_pending`, so no later frame reaches the reconcile. For that boundary `lod_settled` stays false for ever, so `boundary_in_progress()` stays true until another `grid_changed` replaces it.
  - The `--bench-camera grid-cross` / `grid-soak` clock holds the path frame while a boundary is in progress (`advance_grid_cross_frame(.., true) == frame.min(total)`), and `grid_cross_complete` requires `!boundary_in_progress`. The camera therefore never moves to the next boundary and the bench never prints its summary.
  - The same ring is also invisible to the `lod-coverage` line: `reconcile_lod_rings` computes `complete` before it knows the verdict and writes `coverage.settled = complete` (`true` for a starved ring), so `m-exteriors.sh`-style gates keep reading `settled=1` for a horizon that lost 352 quads.
- **Evidence**:
  - `grep -rn 'settle_lod\|lod_settled' byroredux/src` outside `telemetry.rs`: only `app_step.rs:346` and a test.
  - The new test `a_capacity_starved_lod_reconcile_is_not_reported_settled` pins `lod_settle_verdict` only, not what the caller does with `Starved`.
- **Disproof attempted**: could another path settle the boundary? `settle_lod` has no other caller. The door-exit arrival has no active boundary (`last_player_grid` is pre-seeded), which is why #6054's own repro did not hang; a crossing with an active boundary does.
- **Impact**: A radius-12 FNV / FO4 `grid-cross` or `grid-soak` run that saturates the pool never finishes (until killed). The bench line reports `unsettled_lod=1`, and the next real crossing bumps `superseded_lod` for a boundary that was merely stuck.
- **Related**: D6-01, #6054.
- **Suggested Fix**: Give the starved verdict an explicit telemetry outcome (settle it with a `starved` flag, or count it as its own terminal state) so the boundary closes; carry `Starved` into `LodCoverageStats` (a `starved`/`rejected` counter the gate verdict can fail on). Add a caller-level test that a `Starved` progress closes the telemetry boundary.

### EXT-D6-2026-10-19-03: Root cause of #6053: the authored-quad seam suppression checks only the immediately coarser level
- **Severity**: MEDIUM (already filed as #6053; the severity there is medium)
- **Dimension**: Distant LOD and trees
- **Location**: `byroredux/src/cell_loader/lod_bands.rs:584-594` (`select_authored_lod_quads`, the `swallowed_by_coarser` test: `l2 == level * 2`); consumer `byroredux/src/cell_loader/terrain_lod.rs:275-365` (`legacy_desired_lod_quads`); test `lod_bands.rs:1491` (`authored_selection_partitions_between_levels`).
- **Status**: Existing: #6053 (open; "root cause unknown" there). This pass found it.
- **Tier Violated**: n/a
- **Game Affected**: FNV, FO3 (terrain with a 3+ level authored table; object LOD for any FO3 worldspace baked at three or more levels)
- **Description**:
  - Selection runs coarse to fine. A quad is skipped when its centre is inside the refine threshold, and a finer quad is dropped when a **selected quad exactly one level coarser** contains its centre. The doc says "a finer quad whose centre a selected immediately-coarser quad contains is dropped", mirroring a descent that "never emits a child of a quad it drew itself". The descent drops every descendant; this check drops only children.
  - With all four levels authored (FNV terrain): a selected level-32 quad swallows its level-16 children (they are never selected), so no level-16 quad exists to swallow the level-8 grandchildren. The level-8 quads survive (their centre is farther than the level-8 threshold) and overlap the level-32 quad. The same applies to level-4 under a selected level-16 quad (its level-8 children are swallowed, so the level-4 grandchildren are not).
  - The unit test uses a two-level table (4 and 8), the only shape where the immediate-parent test is enough.
- **Evidence**:
  - A faithful port of `select_authored_lod_quads` with the FNV ladder (`refine_cells` 12 / 18 / 27, `max_cells` 64) over a full four-level tiling (`/tmp/audit/streaming-deep/exterior/sim_select.py`) selects overlapping level pairs `(4, 16)` and `(8, 32)` and no other pair, for every player position tried.
  - The pairs match #6053 exactly: its census is `{L4: 238, L8: 196, L16: 12, L32: 12}` with 384 overlaps and the example `(8,-64,24) vs (32,-64,0)`. Each level-32 quad holds 16 level-8 children and each level-16 quad holds 16 level-4 children: 12 x 16 + 12 x 16 = 384.
  - `lod_coverage::find_overlaps` is correct (it counts footprint intersections); the gate is reporting a real double selection, not a counting artefact.
- **Disproof attempted**: could the live level-4 / level-8 sets come from `legacy_desired_lod_quads`' level-4 completion tiles instead? Its zero-overlap guard (`terrain_lod.rs:356-358`) skips any tile a selected quad touches, and the completion tiles are level 4 only, so they cannot produce the `(8, 32)` pairs. The object ring is unaffected on FNV because FNV bakes objects at level 4 only (issue census: `object overlaps 0`).
- **Impact**: Two terrain-LOD surfaces cover the same far-field ground after any traversal (z-fight / double draw; unproven on screen, as in #6053). FO3 object LOD is exposed the same way wherever a worldspace authors three or more levels (not censused here).
- **Related**: #6053, #5222, #5387, #5520.
- **Suggested Fix**: Drop a finer quad when **any** selected coarser quad (not only `level * 2`) contains its centre; add a four-level tiling case to `authored_selection_partitions_between_levels`. That closes #6053's guard (`full_detail_overlaps` / `overlaps`) for the terrain ring.

### EXT-D6-2026-10-19-04: Coarse terrain-LOD bands assume a ~6-cell streaming radius; at the default radius 12 they overlap resident full-detail cells (and legacy synth coarse blocks keep stale holes)
- **Severity**: MEDIUM. A wrong draw (double terrain, or a hole) at 11 to 13 cells; not confirmed on screen.
- **Dimension**: Distant LOD and trees
- **Location**:
  - `byroredux/src/cell_loader/terrain_lod.rs:564-571` (`let mask = if level == k { block_hole_mask(..) } else { 0 }`), `:1069-1073` (stored `hole_mask` is 0 for coarse blocks), `:190` (premise comment).
  - `byroredux/src/cell_loader/terrain_lod.rs:1376-1440` (`coarse_terrain_bands_never_reach_the_full_detail_region`, sweeps radius_unload 6 and 6..=8).
  - `byroredux/src/cell_loader/terrain_lod.rs:845-852` (`spawn_lod_block` skips every cell resident at spawn time, at any level).
  - `byroredux/src/scene.rs:40` (`DEFAULT_EXTERIOR_RADIUS = 12`), `byroredux/src/app_step.rs:1462-1466`.
- **Status**: NEW (the closed #4505 pinned the premise only up to `radius_unload` 8; the default became 12 later).
- **Tier Violated**: n/a
- **Game Affected**: FO3, FNV (authored ladder, synth coarse blocks); Skyrim, FO4 (`.btr` coarse bands)
- **Description**:
  - Only the finest band (level 4) has a hole mask. Coarse bands rely on being selected "far beyond the near ring (their refine thresholds start at 15 cells against a ~6-cell streaming radius)". The default exterior radius (cold start and door transitions) is 12 (hysteresis 13); `parse_exterior_radius` allows 1..=12.
  - **FO3 / FNV.** `select_authored_lod_quads` has no full-detail exclusion and no hysteresis. A level-8 quad is selected once its centre is more than 12 cells away, so its nearest cell is about 10 away, inside the resident ring; a level-16 quad (threshold 18) reaches about 12. The port in `sim_select.py` over a full four-level authored lattice shows coarse quads covering resident cells for 60 of 64 player positions at radius 12 (up to 141 cells). Each of those synth blocks is built by `spawn_lod_block`, which skips the cells resident **at spawn time**, then stores `hole_mask = 0`. `stream_lod_blocks` compares that to `mask = 0` for ever, so the block is never regenerated: cells that load afterwards are overdrawn, cells that unload afterwards leave a permanent moat in the distant ground.
  - **Skyrim / FO4.** `.btr` coarse quads have no holes at all. With the band hysteresis (`sticky = -1` for a resident quad) a level-8 quad stays emitted until its centre is within 14 cells, i.e. its nearest cell is 11 away. `sim_descent.py` (a walk along +x and +y) records level-8 quads (and level-32 on FO4) with their nearest cell at 11, inside the radius-12 ring.
  - The guard pins the premise for radius_unload 6..=8 and calls that "every playable radius" (#4505). The smoke gates (`m-exteriors.sh` boundary / static) run at `--radius 1`, so neither the guard nor the live `full_detail_overlaps` metric ever sees the default.
- **Evidence**: `stream_lod_blocks` computes `mask` for level 4 only (`:567-571`); `LodBlock.hole_mask` for coarse synth blocks is `0` (`:1069-1073`); `find_terrain_full_detail_overlaps` only treats level-4 cells as holed (`lod_coverage.rs`), so the runtime backstop would report these overlaps, but only when run at the default radius.
- **Disproof attempted**: could the full-detail terrain win by depth ordering? The LOD terrain is `RenderLayer::Architecture` (zero bias, same as the near terrain) and only its TLAS membership differs (`static_meshes.rs:131-140`); nothing biases it behind the near mesh. Could the ring not reach 12 resident cells in practice? The load set is the full Chebyshev square (`compute_streaming_deltas`), and the doc says the default is 12.
- **Impact**: At 46 to 53 thousand BU, near terrain and a decimated LOD mesh occupy the same ground (z-fight or a LOD sheet poking through), and on FO3 / FNV a stale hole can leave the distant ground missing next to a cell that unloaded.
- **Related**: #4505 (closed, the test-only guard), #1866 / #1871 (the finest-band hysteresis fix), D6-03 (same legacy selection function).
- **Suggested Fix**: Exclude quads that intersect `resident_full_cells` from the coarse bands the way the object ring already does (`quad_intersects_full_detail` in `stream_object_lod_blocks`), or hole the coarse mesh the way the finest band does (and then track a coarse mask so a residency change regenerates the block). Extend the guard to the default radius (`radius_unload` 13) and run the live `lod.coverage` check at radius 12 on FNV and Skyrim.

### EXT-D2-2026-10-19-05: Doc rot in the terrain / LOD stack: displaced doc blocks, stale radius premises, a wrong return doc
- **Severity**: LOW (doc-rot)
- **Dimension**: Terrain / Distant LOD
- **Location**:
  - `byroredux/src/cell_loader/terrain.rs:356-369` (the "Resolve the authored `LTEX.GNAM` grass association" doc sits on `cover_affinity_key` at `:378`; `authored_grass_for_splat_layers` at `:389` has none).
  - `byroredux/src/cell_loader/terrain.rs:586-590` (the "Generate a terrain mesh from LAND heightmap data" doc and a stray `#[allow(clippy::too_many_arguments)]` sit on `splat_indices_to_release` at `:597`; `spawn_terrain_mesh` at `:818` has neither).
  - `byroredux/src/cell_loader/terrain_lod.rs:63` ("the default full-detail radius-5 view"), `:190` ("~6-cell streaming radius"), and `byroredux/src/cell_loader/terrain_lod_btr.rs:28-30` ("far outside the streaming radius").
  - `byroredux/src/cell_loader/spawn/authored_cover.rs:23-24` ("or `None` when no record's model loaded"; the function returns `Some(cover_root)` unconditionally).
  - `docs/engine/exterior-grid-streaming.md` §4 (describes LOD "settle" but not the #6054 `Starved` stop, nor that no ring retries a refused quad).
- **Status**: NEW
- **Tier Violated**: n/a
- **Game Affected**: all (docs)
- **Description**: Same family as #6046 / #6064 (a file split spliced doc comments onto the wrong item). The radius comments are the premises D6-04 depends on; the default moved from 3 / 5 to 12 without them.
- **Impact**: A reader trusts the 6-cell premise (the cause of D6-04) and the misplaced `GNAM` / `spawn_terrain_mesh` docs describe the wrong function.
- **Suggested Fix**: Move the two doc blocks (and the `allow`) back onto their functions; restate the radius premises against `DEFAULT_EXTERIOR_RADIUS` once D6-04 is decided; correct the `authored_cover` return doc; add the `Starved` paragraph to the streaming doc.

## Findings count

| Dimension | CRITICAL | HIGH | MEDIUM | LOW |
|---|---|---|---|---|
| D1 EXAL boundary (area part) | — | — | — | — |
| D2 Terrain (spawn / splat) | — | — | — | 1 |
| D5 WATAL (area part) | — | — | — | — |
| D6 LOD / trees | — | — | 4 (1 Existing #6053) | — |
| D3 / D4 / D7 | out of area | | | |
| **Total** | **0** | **0** | **4** | **1** |

## Dimension notes (no findings)

- **Dim 1 (area part).**
  - `2c203278f`: the new WSLT branch of `resolve_default_weather` picks the highest non-negative-chance row that resolves in `weathers`; the `DefaultWeather` stand-in is demoted to a fallback and still fires only when a climate's seasonal rows exist and none resolves. `synthesize_wths_weathers` runs once after the whole load-order fold (`load_order.rs`), after every plugin merge, so cross-plugin RFDP chains resolve. Both callers (`build_exterior_world_context`, the per-cell XCCM re-resolve) still go through the one function. The cloud / sun-sprite side of WTHS (#6039) is open and outside this area.
  - `step_streaming` runs `apply_cell_climate_override` and `apply_forced_weather` before the `grid_changed` guard each tick, as the skill describes (cost one `Option` compare per frame); `climate_override_grid` caches failures. The flush ownership problem for those two installers is #6026 (open).
  - `ExteriorWorldContext::lod_worldspace_key()` is the single source for the LOD keys; the `wctx.worldspace_key` reads left in `stream_lod_blocks` are the hole-mask and cell-LTEX reads, as intended.
- **Dim 2.** `spawn_terrain_mesh`: every early return after `build_cell_splat_layers` releases the splat refcounts (no allocator, mesh upload failure, tile table full), and the success path hands them to the `TerrainTileSlot` whose `free_terrain_tile` returns `texture_indices()` (diffuse + normal + specular) to `release_entities_timed`. `collect_victim_gpu_handles` sweeps `MeshHandle`, `TextureHandle`, `NormalMapHandle`, `WaterNoiseMapHandles`, the secondary roles and the flipbook frames. The cell's `TerrainCoverInputs` density override was audited on 10-18 (D3-01 there is #6027).
- **Dim 5.** `refresh_lod_water` / `lod_water_rebuild_due` re-read: the dirty flag is consumed on every attempt, the rebuild uploads the new mesh before dropping the old one, an emptied ring removes `MeshHandle` and zeroes the histogram, `unload_lod_water_plane` releases the normal and noise refcounts and the mesh. `build_distant_water_mesh` is pure and tested. Distant water uses the global geometry pool (`upload_scene_mesh`), so under starvation a rebuild fails and the previous mesh is kept (retried on the next real change): consistent with #5339, no finding.
- **Dim 6, other.**
  - `compute_streaming_deltas`, `stale_pending_coords`, `classify_payload`, the early-`CellRoot` cancel (`ExteriorCellApplyJob::cancel` -> `unload_cell`), the stale-payload salvage (#5512) and the persistent-CELL job (`stamp_slice` cursor reseeded per `advance`) hold. A worldspace hole is re-queued to the worker at every crossing (it is never in `loaded`) and costs a channel round-trip with an empty payload; cheap enough not to file.
  - `drain_streaming_state` reclaims terrain, object, placement and tree rings plus the distant water plane; `state.lod_missing_blocks` and the availability memos die with the state.
  - `legacy_lod_index.rs` name parsing is bounded and rejects `.high.` variants and non-ladder levels; `select_authored_lod_quads` is also the function in D6-03.
  - `LodWorkBudget::for_reconcile` still honours a deadline alongside an unbounded attempt count (#3823 test passes).
  - #2586 (Tamriel lattice origin) is unchanged and still open; not re-reported.

## Dedup sources

- `/tmp/audit/issues.json` (121 open) plus fresh `gh issue list --state all --search` for each finding's keywords ("LOD capacity starved sentinel retry", "Starved LOD settle telemetry grid-cross", "authored LOD seam suppression grandchild", "coarse terrain LOD bands radius 12 hole mask", "boundary_in_progress bench hang", "terrain.rs doc comment displaced"): no open hit for D6-01, D6-02, D6-04 or D2-05. D6-03 is #6053 (open).
- Closed and checked: #6054 (the fix under D6-01 / D6-02; in place but incomplete), #4505 (closed 2026-09-20, guard-only; the premise it pinned no longer matches the default radius), #1866 / #1871 (finest-band hysteresis, in place), #5705 (distant-water gate, in place), #5222 / #5387 / #5520 (authored selection and overlap prune, in place), #5496 (LOD key, in place).
- The 10-18 findings #6026 to #6031 are all still open and unchanged; no area code they cite moved.
- Safety audit of the same day (`AUDIT_SAFETY_2026-10-19.md`) read for dedup: its three findings (ECS lock-order inversions, `ShadowFilterPipeline`, a misplaced `memory_barrier` doc) are outside this lens and were not redone.

## Known-Open Register (dated; what this pass changed)

| Item | Status this pass |
|---|---|
| #6053 (FNV `lod.coverage` red: 384 overlapping terrain-LOD pairs) | OPEN. Root cause found here (D6-03). |
| #2586 (Tamriel lattice origin) | OPEN, unchanged, still present at HEAD. |
| #5505 (Oblivion child worldspaces: no ground outside own LAND) | OPEN, unchanged. |
| #3307 (active VWD full-model culling) | OPEN, unchanged. |
| #6054 (capacity-starved LOD ring reported as settled) | CLOSED. Verdict half is in place; the recovery half and the telemetry half are D6-01 / D6-02. |
| #4505 (coarse-band boundary guard) | CLOSED. Its premise (radius_unload at most 8) no longer covers the default; see D6-04. |
| #6026 to #6031 (10-18 exterior findings) | OPEN, unchanged. |
| Live FO3 / FNV visual check, Skyrim Tamriel baked-LOD check, tree-LOD look, grass look / LOD tiers | Owed (no engine launch this pass). |
| Live `lod.coverage` at the default radius 12 (D6-04's runtime check) | Owed; the smoke gates run at `--radius 1`. |

## Skill drift (for the next `/audit-exterior` edit)

- **Dim 6**: add (a) the pool-admission contract of the four LOD providers (a refused upload is not a known miss; `scene_geometry_admission_open`), (b) the `Starved` verdict and its telemetry / coverage outputs, (c) `select_authored_lod_quads`' seam suppression must consider every coarser ancestor and its test needs a four-level table, (d) the coarse-band premise must be checked against `DEFAULT_EXTERIOR_RADIUS`, not against radius 6 to 8, and the smoke radius (1) never exercises it.
- **Dim 7**: `m-exteriors.sh boundary` and the LOD gates run at `--radius 1`; add one leg at the default radius.
- **Dim 1 / 5**: unchanged.

## Cross-audit routing

- **`/audit-renderer` / `/audit-performance`**: D6-01 (pool admission policy: reserve LOD headroom or cap the full-detail radius under pressure, per #6054's own suggested fix), D6-04 (z-fight with zero-bias LOD terrain).
- **`/audit-fnv` / `/audit-fo3`**: D6-03 and D6-04 (live verification of both on the authored legacy ladder).
- **`/audit-tooling`**: D6-02 (bench clock and gate semantics).

Suggested next step: `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-10-19.md` (D6-03 is already #6053: comment the root cause there rather than filing).

**Labels:**
- D6-01: `bug`, `terrain-exterior`, `medium`, `renderer`.
- D6-02: `bug`, `terrain-exterior`, `medium`, `tech-debt`.
- D6-03: comment on #6053 (no new issue).
- D6-04: `bug`, `terrain-exterior`, `medium`, `legacy-compat`.
- D2-05: `documentation`, `doc-rot`, `terrain-exterior`, `low`.
