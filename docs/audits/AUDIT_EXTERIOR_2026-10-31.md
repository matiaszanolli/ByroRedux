**HEAD**: 670f81917 · **Baseline**: [AUDIT_EXTERIOR_2026-10-27.md](AUDIT_EXTERIOR_2026-10-27.md) (HEAD `fa6be4a6b`; 136 commits since) · **Audited**: Dim 1 (EXAL boundary), Dim 2 (terrain), Dim 3 (ground cover), Dim 4 (sky / weather / sun), Dim 5 (water), Dim 6 (distant LOD / trees), Dim 7 (gates, static only) · **Unchanged since baseline (skimmed)**: none wholesale. Every dimension had at least one commit in its `Paths:`, but Dim 2 (comment-only and one 8-line cover fix), Dim 4 (a lock-order reorder, a duplicate `#[cfg(test)]`, one uniform lane) and Dim 5 (named-constant moves plus one sampler rule) were small deltas read in full.

# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-10-31

**Method.** This audit ran inside `/audit-suite --preset comprehensive` and followed its rules. It used no sub-agents and launched no engine. It built only in the repo `target/` with `CARGO_INCREMENTAL=0`. It created no issues, made no commits and edited no source. I scoped each dimension with `git log fa6be4a6b..HEAD -- <Paths>` and read every diff. Per-dimension notes are in `/tmp/audit/exterior/dim_<N>.md`. I deduplicated against `/tmp/audit/issues.json` (98 open issues), a closed-issue search, my 2026-10-27 report and § 6 of the suite rules. Every finding is a static trace. **None was confirmed on screen.**

**Test state (HEAD, repo `target/`, `TMPDIR=/mnt/data/tmp`, all green):**

| Suite | Result |
|---|---|
| `-p byroredux --bin byroredux -- env_translate climate default_weather forced fog weather exterior terrain groundcover water lod sky streaming_helpers pool_reserve shared_parse` | 819 passed, 0 failed, 25 ignored |
| `-p byroredux-renderer --lib -- groundcover sky_ cloud terrain water composite` | 263 passed, 1 ignored (GPU) |
| `-p byroredux-terrain-height` | 11 passed |
| `-p byroredux-physics` | 270 passed |
| `-p byroredux-core --features inspect -- water bed_field groundcover` | 37 passed |
| `scripts/check-shader-artifacts.sh` (glslang 16.4.0) | 38 shaders + opaque early-test variant match their committed SPIR-V |
| `docs/smoke-tests/m-exteriors.sh --self-test` | PASS |

Not run: the `--ignored` real-data and GPU tests, and every engine-launching smoke gate (`m-exteriors.sh` modes, `w1`, `w2`, `tsd1`, `m34`).

## Executive Summary

**4 findings: 0 CRITICAL · 0 HIGH · 0 MEDIUM · 4 LOW.** All 4 are NEW. Two are doc-rot and two are test-gaps.

- **The 2026-10-27 CRITICAL (EXT-D6-2026-10-27-01, `spawn_lod_block` indexing `holes[i]` past the grid samples) is FIXED.** `terrain_lod.rs:1125` now reads `vertex_bound(&vertices, |i| holes.get(i).copied().unwrap_or(false))`, with a comment that the skirt vertices are never holes. The fix landed inside `5af77a128`, which is an audit-report commit, so no `Fix #N` commit names it. Nothing guards it (EXT-D6-2026-10-31-01).
- All seven 10-27 LOWs are fixed with their tracker issues:

| 10-27 finding | Issue | Fixed by |
|---|---|---|
| D6-02 coarse-band skirt residual | #6452 | `36b2ab01b` |
| D6-03 gain table shadowed by a `combined` gate | #6453 | `4334b9846` |
| D6-04 two spliced doc blocks | #6454 | `bf6c41ec1` |
| D2-01 bare softening literals + wrong doc | #6455 | `36852df1a` |
| D2-02 `exal.md` silent on the LOD join | #6456 | `8a9401c24` |
| D3-01 stale 9-21 BU grass doc | #6457 | `0923c8747` |
| D7-01 count-only FO4 seam exemption | #6458 | `37a7e9ff9` |

- The 10-26 / 10-27 open set #6410-#6420 and #6445 is also fixed. I checked #6411, #6415, #6418, #6419, #6420 and #6445 against the code.
- The four new findings are all residue of those fixes. The #6464 change (FO4 joins the exterior-processing gate) left a stale spec sentence and a spliced test doc. The #6415/#6458 work left a gate self-test that no lane runs. The CRITICAL fix has no regression guard.

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| single-boundary | **Holds.** No new `translate_*` / `SkyParamsRes{` / `WeatherDataRes{` / `WaterMaterial{` site outside `env_translate/`, `components/` and `#[cfg(test)]`. The consumer-side `combined` re-gate on the albedo gain is gone (#6453). The shore-refraction shape constants and `WATER_CLEAN_IOR` now live once in `core::components::water` and reach `water.vert`, the composite shader and the CPU crest sampler by name (#6419, #6418). |
| no-fabrication | **Holds.** The new `FO4` entry in `needs_exterior_processing` cites a measurement (Nuka-World pair decisive, Goodneighbor consistent, not confirmed visually) and says so. The softening onset and reach are named engine choices (#6455). The god-ray `0.12` and the march's tuning literals stay under open #6526. |
| no-leak | **Holds.** The camera column's cloud transmittance rides `CompositeParams::sky_aperture_count.y`. The pass is gated to exterior cameras (`depth_params.x > 0.5`), so an interior pool does not pulse with the weather outside, which is the class #6245 fixed for caustics. |
| no-render-time-fallback | **Holds.** No `GameKind` or game token in any sky, cloud, ground-cover, water, terrain or composite GLSL. `grep -rn GameKind crates/physics/src` is empty. |

## Per-Category Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Terrain (LAND, splat, GNAM cover) | `build_cell_splat_layers`, `spawn_terrain_mesh`, `cover_density_inputs` | OK | OK | OK | OK |
| Terrain shading (height blend / parallax / softening) | `terrain_lod_soften_end`, `push_terrain_lod_soften` | OK | OK (named, #6455) | OK | OK |
| Sky (bake, clouds, SH) | `translate_sky`, `pack_sky_dome` | OK | OK | OK | OK |
| Weather / sun | `translate_weather`, `weather_system` | OK | OK | OK | OK |
| Climate / landscape inheritance | `resolve_exterior_climate`, `needs_exterior_processing` | OK | OK (FO4 measured) | OK | OK. **D1-01** (spec), **D1-02** (test doc) |
| Water | `resolve_water_material`, `water_material_from_mesh`, `WaterBedField::sample_baked` | OK | OK | OK | OK |
| Ground cover | `groundcover_translate.rs`, `resolve_authored_cover` | OK | OK | OK | OK |
| LOD (terrain) | `spawn_lod_block`, `synth_block_albedo_gain`, `coarse_refine_radius` | OK | OK | OK | OK. **D6-01** (crash fix unguarded) |
| LOD (objects / placement / trees) | `object_lod_scheme`, `placement_lod_cell_path` | OK | OK | OK | OK |

## Findings

Finding IDs are `EXT-D<dim>-2026-10-31-NN`.

### EXT-D6-2026-10-31-01: the fix for the 10-27 `holes[i]` out-of-bounds panic has no regression guard
- **Severity**: LOW (test-gap). The defect is fixed; nothing prevents its return.
- **Dimension**: 6 Distant LOD and trees
- **Tier Violated**: n/a
- **Game Affected**: every game with terrain LOD (Oblivion, FO3, FNV, Skyrim, FO4)
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:1116-1125` (`append_lod_skirts`, then `vertex_bound(.., |i| holes.get(i).copied().unwrap_or(false))`); `byroredux/src/cell_loader/lod_support.rs:1284` (`vertex_bound_ignores_skipped_vertices`)
- **Status**: NEW. This is a guard gap behind the fixed EXT-D6-2026-10-27-01. The fix landed in `5af77a128`, an audit-report commit, with no issue number.
- **Description**: The 10-27 crash came from calling the shared `vertex_bound` with a closure indexing `holes`, which has one entry per grid sample, after `append_lod_skirts` had pushed extra vertices. `spawn_lod_block` needs a `VulkanContext`, so no unit test reaches that call. The only test that pairs `vertex_bound` with a hole mask, `vertex_bound_ignores_skipped_vertices`, builds a mask exactly as long as its vertex list. That is the shape that cannot fail, and it would stay green if someone restored `|i| holes[i]`. The fix is a single inline `.get(i)` with no named helper to pin.
- **Evidence**:
  ```rust
  append_lod_skirts(&mut vertices, &mut indices, &skirt_edges);   // vertices.len() = n*n + 2*E
  ...
  let bound = vertex_bound(&vertices, |i| holes.get(i).copied().unwrap_or(false));
  ```
  ```rust
  let holes = [false, false, true];            // same length as `verts`
  let b = vertex_bound(&verts, |i| holes[i]);
  ```
- **Impact**: A later refactor of the bound call (it has already been rewritten twice: `block_bound`, then the shared `vertex_bound`) can reintroduce a main-thread panic on every skirted block. The panic would reach the same path as before: `stream_lod_blocks` on the main thread, with no `catch_unwind`.
- **Suggested Fix**: Extract `terrain_block_bound(vertices: &[Vertex], holes: &[bool]) -> WorldBound` with the `get(i).unwrap_or(false)` rule, and call it from `spawn_lod_block`. Add a test with `vertices.len() > holes.len()` (for example the output of `append_lod_skirts` on a 2x2 grid). Also assert that a hole vertex placed far away does not stretch the sphere while a skirt vertex does.

### EXT-D1-2026-10-31-01: `exal.md` still says "Oblivion and FO3/FNV only … not measured on the Skyrim+ children" after #6464 added FO4
- **Severity**: LOW (doc-rot)
- **Dimension**: 1 EXAL boundary
- **Tier Violated**: n/a
- **Game Affected**: FO4 (documentation)
- **Location**: `docs/engine/exal.md:148-154` (the "Landscape (#5505)" paragraph)
- **Status**: NEW. The sentence was current at `fa6be4a6b`; `977936719` (#6464, 2026-10-29) made it wrong.
- **Description**: The paragraph says the landscape inheritance runs "once after the load order is merged and for Oblivion and FO3/FNV only (`needs_exterior_processing`; …) — not measured on the Skyrim+ children". Since `977936719`, `needs_exterior_processing` is `Oblivion | Fallout3NV | Fallout4`. NukaWorldMarket and NukaWorldAmphitheater were measured decisive, and Goodneighbor is consistent. The rustdoc on `needs_exterior_processing`, `SharedParseCache::get_or_parse` and `EsmCellIndex::inherit_parent_landscapes` was updated in the same commit. The spec was not. The spec is also the doc of record for the measurement that justifies the gate. Skyrim and Starfield are the games "not measured".
- **Evidence**:
  ```
  order is merged and for Oblivion and FO3/FNV only (`needs_exterior_processing`;
  #6089 measured the FO3 "Use Land Data" children co-located on the Wasteland's
  grid, …) — not measured on the Skyrim+ children.
  ```
- **Impact**: A reader trusting the spec concludes FO4 children do not inherit LAND and may re-open #6464, or may not know that the FO4 evidence (Nuka-World pair; Goodneighbor not visually confirmed) exists.
- **Suggested Fix**: Say "Oblivion, FO3/FNV and FO4", cite #6464 with the measured medians (|dz| 69 / 168 against 448+ / 713+ for shifted controls) and the Goodneighbor caveat, and narrow "not measured" to Skyrim and Starfield.

### EXT-D1-2026-10-31-02: #6464 spliced a new test between `the_cache_and_the_processing_step_share_one_gate` and its doc comment
- **Severity**: LOW (doc-rot)
- **Dimension**: 1 EXAL boundary
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Location**: `byroredux/src/cell_loader/load_order.rs:1957-1969`
- **Status**: NEW. This is the same class as closed #6373 / #6454 and open-then-fixed #6410, and `977936719` is its cause.
- **Description**: The doc "The step's gate and the cache's `exterior_ready` are one predicate: every game the step processes is re-parsed for an exterior session, and no other is." used to head `the_cache_and_the_processing_step_share_one_gate`. The commit inserted `use_land_data_children_inherit_on_oblivion_fo3_fnv_and_fo4_only` directly under that doc, so the doc now describes the new gate-membership test. The sharing test (:1969) has no doc.
- **Evidence**:
  ```rust
  /// The step's gate and the cache's `exterior_ready` are one predicate: ...
  #[test]
  fn use_land_data_children_inherit_on_oblivion_fo3_fnv_and_fo4_only() {   // :1960
  ...
  #[test]
  fn the_cache_and_the_processing_step_share_one_gate() {                 // :1970, no doc
  ```
- **Impact**: Misleading test documentation on the exact predicate that #6407 and #6464 turned on. `source_scan::spliced_doc_blocks` trips only when the doc leads with a sibling's name, so it cannot see this.
- **Suggested Fix**: Give the new test its own doc (the measured FO4 children and the Skyrim "not measured" note already in its body) and put the "one predicate" sentence back above `the_cache_and_the_processing_step_share_one_gate`.

### EXT-D7-2026-10-31-01: `m-exteriors.sh --self-test` (the #6458 exemption decision) is run by no CI lane or contract checker
- **Severity**: LOW (test-gap)
- **Dimension**: 7 Acceptance gates and harness
- **Tier Violated**: n/a
- **Game Affected**: FO4
- **Location**: `docs/smoke-tests/m-exteriors.sh:48-95` (`fo4_known_seam_crack`, `--self-test`); `scripts/check-playable-smoke-contracts.sh:255` (runs only `m47-triggers.sh --self-test`); `.github/workflows/ci.yml`
- **Status**: NEW. This is a gap left by `37a7e9ff9` (#6458).
- **Description**: #6458 turned the FO4 seam exemption into a pure shell function and added a `--self-test` for it, so the decision can be proven without Vulkan or game data. `m47-triggers.sh` does the same and its self-test is run by `check-playable-smoke-contracts.sh:255`. No lane runs `m-exteriors.sh --self-test`, so the self-test only runs when a developer thinks to call it. The exemption is the only thing between a new 3-vertex FO4 crack and a green boundary gate. I ran the self-test by hand at HEAD and it passes.
- **Evidence**: `grep -rn "self-test" scripts .github docs/smoke-tests/m-exteriors.sh` lists `m47-triggers`, `check-issue-traceability`, `check-bench-harness-provenance`, `package-release` and `fsr_bench_report` as gated, and `m-exteriors.sh` only at its own definition.
- **Impact**: Edits to `fo4_known_seam_crack` or to the `first_dirty_pair=` parse can break the exemption (in either direction) with no CI signal until an FO4 boundary run on a machine with game data.
- **Suggested Fix**: Add `bash "$ROOT/docs/smoke-tests/m-exteriors.sh" --self-test >/dev/null || fail "m-exteriors.sh --self-test failed"` beside the m47 line in `check-playable-smoke-contracts.sh`.

## Dedup notes (not re-filed)

- **Existing: #6421** (`docs/smoke-tests/README.md` row for `w1-water-traversal.sh` still says Skyrim declares no route). Still present and unchanged.
- **Existing: #6048** (terrain collider LTEX Havok material), **#6106** (full-detail refusal never retried), **#3307** (VWD full-model culling). All still open, code unchanged.
- **Existing: #6526** (underwater god-ray march keeps loop bounds and ~15 tuning literals inline). #6415 added one more consumer of the same block (`cameraCloudSunT`), but it is a named multiplier, not a new bare literal.
- **Dup of AUDIT_REGRESSION_2026-10-31 REG-2026-10-31-04**: nine new `m43-*.sh` smoke scripts are in `docs/smoke-tests/` but in no contract loop. They are in Dim 7's path but are not exterior gates.
- **Dup of AUDIT_REGRESSION_2026-10-31 REG-2026-10-31-03**: `ROADMAP.md` is 808 lines against the 800 ceiling. Not exterior.
- **Dup of AUDIT_PERFORMANCE_2026-10-31 PERF-2026-10-31-D7-02**: #6464 adds FO4 to `needs_exterior_processing`, so an FO4 interior-start session re-parses the load order at the first exterior door. I confirmed the gate change and the cache re-parse are intentional (#6407's shared predicate), and the cost is performance-owned.
- **Dup of AUDIT_PERFORMANCE_2026-10-31 PERF-2026-10-31-D7-01**: the `NifImportRegistry` eviction deferral in `app_step.rs` / `streaming/mod.rs` (`304f3b587`). Its `geometry_batch_in_progress` split is behaviour-preserving for the LOD-water rebuild defer.
- **Dup of AUDIT_RENDERER_2026-10-31 REN-D12-01** and the `gpu_timers.rs` refactor (#6358): the `TimerPass` rename touched `groundcover/frame.rs`, `groundcover_models.rs` and `groundcover_bench.rs`. The model-timer source-scan test was updated to the table row and still asserts the same stages and active bit.
- **Not filed**: the `scattered_name_swap` matcher added by #6472 flags any `x`/`*_x` first argument paired with a `y`/`*_y` negated third argument. It reads as intentionally narrow, and its own doc says a false hit must be rewritten or the matcher narrowed.

## Verification notes for the fixes I traced

- **#6411** (`cover_density_inputs`): `ltex_form_id == None` lanes and an unauthored base (`normalize_base_ltex(base_ltex).is_none()`) keep the editor-ID affinity under a localised `AuthoredCover`. The pin `a_localised_cover_keeps_the_default_land_textures_classification` is live and covers an authored lane, a default lane and both bases.
- **#6415**: `pack_camera_cloud_sun_transmittance` clamps to `[0, 1]` and maps non-finite to 1.0. The shader reads it behind the exterior gate. The sky-cube parameters do not copy `sky_aperture_count`. `every_direct_sun_site_applies_the_cloud_shadow` now also scans `composite.frag` and the host plumbing. The `.spv` matches.
- **#6420**: `sample_baked` is used by the mesh bake, the CPU crest sampler (`authored_wave_height_over_bed`) and the river-current sampler. The fade is 0 on the border and 1 from one lattice step in, and depth is untouched. `the_physics_bed_field_agrees_with_the_baked_shore_direction` compares direction per vertex. Clipped shoreline polygons with vertices inside the strip now fade continuously, a deliberate consequence the commit states.
- **#6445**: `weather_system` copies the four tile scales into a local before taking the `CloudSimState` write guard. The `SkyParamsRes` guard is a statement-scoped temporary.
- **#6452**: `coarse_refine_radius(radius_unload) = radius_unload + 1` reaches both selectors (`select_authored_lod_quads` call and `LodBandSelection::refine_within`). `streaming_helpers.rs:197` still passes the raw `radius_unload` as `full_detail_radius`, and no other raw consumer remains. Both ladder pins assert the margin.
- **#6458**: `first_dirty_pair` is the lexicographic minimum, so it does not depend on resident-set iteration order. With exactly one dirty pair the equality check is exact. `m-exteriors.sh --self-test` passes (see EXT-D7-2026-10-31-01 for its missing gate).

## Known-Open Register (what this pass changed)

| Item | Owner doc / issue | This pass |
|---|---|---|
| Spliced `spawn_lod_block` docs / bare `allow` on `LOD_SKIRT_DROP` | #6410 | **closed**, verified (`LOD_SKIRT_DROP` carries its own doc; `the_ring_rebuilds_and_gates_on_the_hole_and_rim_pair` has its doc back) |
| Coarse-quad margin from the resident ring | #6452 | **closed**, verified |
| Terrain-LOD albedo gain legacy rows | #6453 | **closed**, verified |
| Distance softening constants and doc | #6455 | **closed**, verified |
| `exal.md` LOD-join paragraph (§5.4.2) | #6456 | **closed**, verified; its coarse-quad margin sentence matches the code |
| Underwater god-ray sun / IOR / shore literals / border direction | #6415 / #6418 / #6419 / #6420 | **closed**, verified |
| FO4 "Use Land Data" children inherit LAND | #6464 | **closed**; spec sentence stale (D1-01), test doc spliced (D1-02) |
| 10-27 CRITICAL `holes[i]` out-of-bounds | EXT-D6-2026-10-27-01 | **fixed** in `5af77a128`, unguarded (D6-01) |
| Terrain collider LTEX Havok material | #6048 | unchanged |
| Full-detail refusal never retried | #6106 | unchanged |
| VWD full-model culling | #3307 | unchanged |
| `w1` README row | #6421 | unchanged, still present |
| Blade-length sourcing | #6201 / #6413 | #6413 documented the clamp trim (σ ~9.5 % low, ~5.6 % at max); the draw remains an unsourced placeholder |
| FO4 `Goodneighbor` co-location | #6464 | not visually confirmed; the Nuka-World pair carries the justification |

Next step: `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-10-31.md` (labels `terrain-exterior`, plus `doc-rot` / `test-gap`).
