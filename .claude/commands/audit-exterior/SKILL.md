---
description: "Deep audit of the exterior layers EXAL / SKYAL / WATAL + ground cover — translation boundary, terrain + LOD, ground-cover pipeline, sky bake / clouds / SH, weather + sun, water translation"
argument-hint: "--focus <dimensions> --game <name> --depth shallow|deep"
---

# Exterior Audit — EXAL / SKYAL / WATAL / Ground Cover

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

Audits the outdoors: per-game ESM environment data -> one canonical form -> terrain, LOD, ground cover, sky, weather/sun, water. Cite the code-verified specs: `docs/engine/exal.md`, `exal-groundcover.md`, `exal-trees.md`, `skyal.md`, `watal.md`; VRAM in `docs/engine/memory-budget.md` § Sky and Ground Cover. Doctrine mirrors `/audit-nifal`: **single-boundary** (one `translate_*` site per category), **no-fabrication** (a new constant cites a source or is a documented engine choice), **no-leak**, **no-render-time-fallback** (no `GameKind`/version branch downstream of the boundary, Rust or GLSL). Orchestrator: each dimension is a Task agent (max 3 concurrent).

**Not owned here**: water shading/sync, sky consumption in `triangle.frag`/`raytrace.glsl`, GPU-struct lockstep, FSR -> `/audit-renderer` (Dim 8, 10, 3, 11); buoyancy -> `/audit-physics` Dim 5; byte-level WTHR/CLMT/WATR/LTEX/GRAS/LAND decode -> `/audit-esm`; `.spt` -> `/audit-speedtree`; streaming cost -> `/audit-performance`; NIF materials -> `/audit-nifal`.

**Severity**: `_audit-severity.md` applies; a wrong/divergent canonical value out of an EXAL `translate_*` is HIGH (all-game blast radius, no render-time fallback).

**Parameters**: `--focus <dims>` (default all 7); `--game <name>` restricts Dim 1, 2, 5, 6; `--depth shallow|deep` (deep = trace one worldspace ESM -> ECS -> GPU; default).
**Extra per-finding fields**: **Dimension** (the dimension name below); **Tier Violated** (single-boundary | no-fabrication | no-leak | no-render-time-fallback | n/a); **Game Affected**.

## Phase 1: Setup

1. `mkdir -p /tmp/audit/exterior`; `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
2. Read the newest `docs/audits/AUDIT_EXTERIOR_*.md`; scope to dimensions whose `Paths:` changed (`git log --since=<report date> --format='%h %cs %s' -- <Paths>`).
3. A `Guard:` counts only while live: `grep -B3 'fn <name>' <file>` shows a plain `#[test]` (not `#[ignore]`) still asserting its name. Aim at what guards cannot see.
4. Cargo tests only; no windowed engine launch (*feedback_no_parallel_engine_launch*).

## Phase 2: Dimensions

### Dimension 1: EXAL boundary discipline
**Paths**: `byroredux/src/env_translate.rs` (+ `env_translate/`), `byroredux/src/groundcover_translate.rs`, `byroredux/src/fog.rs`, `byroredux/src/scene/world_setup.rs` (orchestration only), `byroredux/src/cell_loader/exterior.rs`, `byroredux/src/app_step.rs` (per-tick XCCM override call)
**First step**: `grep -rn 'translate_sky\|translate_weather\|translate_exterior_cell_lighting\|procedural_fallback_' byroredux/src | grep -v -E 'env_translate.rs|test'` — callers are `scene/world_setup.rs`, `systems/weather.rs`, `cornell.rs`, plus `render/sky.rs`'s direct-`--cell` interior fallback (known-open #4902: a render-loop rebuild); a `SkyParamsRes {` / `WeatherDataRes {` literal outside `env_translate.rs`, `components.rs` and `#[cfg(test)]` is a second site (the `let &WeatherDataRes {` in `systems/weather.rs` is #4733's destructure).
- Sole producers: `translate_sky`, `translate_weather`, `translate_exterior_cell_lighting`, `exterior_image_spaces` (IMGS, #4416), `procedural_fallback_*`, `resolve_worldspace_climate` / `resolve_cell_climate`, `translate_lod_water`, `translate_terrain_lod_textures`; bulk `--grid`, streaming and XCCM overrides all install through `apply_environment`. No-climate is an explicit canonical default, not a render-loop branch. Known-open: `image_space` patched in after `translate_weather` (#4914) and not promoted by the cross-fade (#4901).
- `GameKind` only as table-shaped `match` returning data (`terrain_lod_layout`, `default_water_for_worldspace`, `object_lod_scheme`, `DefaultLandTexture::for_game`, `combined_lod_supported`). Flag `game ==` in logic and any game token in `sky*` / `clouds` / `groundcover_*` GLSL.
- Sentinels are not leaks (`skyrim_dalc_per_tod: None`, no default water, `sunlight_dimmer` 1.0 without HNAM); grep a parsed field's consumers before calling it "parked". Constants cite a source or are documented engine choices: `SUN_SOUTH_TILT` (no authored latitude, exal.md §9 Q1), cloud coverage 0.86/0.80/0.70/0.40/0.55 (skyal.md §2.3).
- `fog.rs` (exterior half): the near/far ramp becomes a `FogMedium` once via `fit_legacy_fog_extinction`; runtime never rebuilds a ramp. Particle fire/smoke volumes there -> `/audit-renderer` Dim 8.
- Guard: `material_translate.rs::every_exterior_spawner_inserts_a_boundary_material` walks `SPAWNER_ROOTS` recursively and, since #4856, counts boundary calls (`translate_texture_only_material*`, `translate_material`, `insert_object_lod_submesh_material`) against `, MeshHandle(` inserts per enclosing function. Blind to other insert idioms and to a translate hoisted into a caller; known-open #4917. `env.health` (`commands/env_health.rs`) gates non-finite/negative/non-unit inputs; known-open #4915, #4916.

### Dimension 2: Terrain, splatting, LTEX/GNAM
**Paths**: `byroredux/src/cell_loader/terrain.rs`, `terrain_seam.rs`, `crates/renderer/shaders/include/terrain_sample.glsl`
**First step**: `cargo test -p byroredux terrain`
- `spawn_terrain_mesh` + `build_cell_splat_layers` are the one LAND translate site (33x33 grid; FO76/Starfield ship no LAND). Splat: <=8 lanes, coverage-aware truncation, BTXT feather; optional normal/specular roles use handle 0 = "no contribution" (never the checker) and upload linear. Per-lane `cover_affinity` is keyed from the LTEX editor ID (`cover_affinity_key`, #4899), never the TXST diffuse path. Known-open (2026-09-29): base LTEX untranslated into cover inputs (#4903); feather stops at cell edges (#4905).
- `LTEX.GNAM` (an array since #4642, carried through `merge_from` since #4904) -> `authored_grass_for_splat_layers` (GPU lane order) -> `TerrainTile.authored_grass`, read by nothing yet (the scatter by design; the model tier uses climate, #4906).
- Guard: `gpu_instance_layout_tests.rs::gpu_terrain_tile_is_160_bytes`, `shader_contract_tests.rs::named_splat_offset_reads_keep_their_unorm_recovery`, `terrain_splat_tests.rs`. Unseen: >8 real layers, a DLC overriding one LAND tile.

### Dimension 3: Ground-cover pipeline
**Paths**: `crates/renderer/src/vulkan/groundcover.rs`, `groundcover_models.rs` (Phase C model tier), `groundcover_stats.rs`, `groundcover_bench.rs` (opt-in §11.1 measurement harness), `crates/renderer/shaders/groundcover_*`, `crates/renderer/shaders/include/groundcover_*.glsl`, `crates/renderer/src/shader_constants_data.rs` (`GROUNDCOVER_*`), `crates/core/src/ecs/components/groundcover.rs`, `byroredux/src/render/groundcover.rs`
**First step**: `cargo test -p byroredux-renderer groundcover && cargo test -p byroredux groundcover`
- Determinism: seeds are frame-invariant integer hashes (only the tier cross-fade uses the frame serial); compaction is in candidate order, never an atomic race. Guard: `groundcover_seed_hashes_are_integer_and_frame_invariant`.
- Sync (#4293): counter clear = zero fill, then ordered seed fills (`counter_seed_fills_are_sequenced_after_the_zero_fill`); source-shape only, so hazards need a `BYRO_VALIDATION=1` run.
- Budget: `GROUNDCOVER_MAX_CHUNKS` 256 x `GROUNDCOVER_MAX_BLADES_PER_CHUNK` 16,384 x 16 B = 64 MiB arena (64.6 MiB total, allocated on every RT device). The model tier adds 8.3 MiB (`groundcover_model_resident_bytes`). Guard: `memory_budget_ledgers_the_sky_and_ground_cover_owners` (both rows), `chunk_cap_covers_every_chunk_in_reach`.
- LOD tiers: blade / ribbon (projected-height crossover, not distance) / clump card / always-on terrain detail layer. Guard: `tiered_indirect_streams_preserve_fixed_blade_slabs`.
- Lifecycle (#4307): `recreate_draw_pipelines` runs in resize's `format_changed` arm. Guard: `context/resize.rs::a_surface_format_change_rebuilds_every_main_pass_pipeline`.
- FSR (#4297): opaque ribbons write reactive 0 / T&C 0 with previous-pose motion vectors; only the stochastic hand-off writes bounded (<=0.9) reactive (§12.14). Guard: `blade_motion_and_fsr_mask_contract_stay_material_driven` (shader-text only).
- Boundary: per-game data enters only via `groundcover_translate.rs` (`layer_affinity`: `nograss`/asphalt suppression beats any positive keyword; climate palette; `resolve_wind*`); `GroundCoverDimmer` (HNAM grass dimmer) rides `weather_system`. Not findings: GRAS is not translated into blades (§12.12 census); REGN RDGS/RDOT is out of scope (§10, #3301). A *new* bare literal in `groundcover_*` GLSL is a finding (existing ones: #4378, §12.12 debt).
- Authored-model tier (§12.12 Phase C, #4413): GRAS -> `resolve_authored_cover` (one caller, `scene/world_setup.rs`) -> `AuthoredCover` -> `groundcover_models.comp`, drawn as indexed-indirect shapes. Guard: `model_timer_encloses_all_phases_and_stats_copy` (#4866 is fixed by it but still open). Known-open (2026-09-29): placed by climate, not LTEX->GRAS (#4906); #4907, #4919-#4921, #4924.
- Known-open: Phase 4 RT proxy gated on need (§5 Stage 2).

### Dimension 4: Sky, weather, sun
**Paths**: `crates/renderer/src/vulkan/{sky_cube,sky_dome,cloud_noise}.rs` (+ `sky_cube/`), `crates/renderer/shaders/{sky_cube,sky_prefilter,sky_irradiance}.comp`, `crates/renderer/shaders/include/{sky,clouds,sky_sh,sky_cube_direction,medium_transport}.glsl`, `byroredux/src/systems/weather.rs`, `byroredux/src/render/sky.rs`
**First step**: `cargo test -p byroredux-renderer sky_ && cargo test -p byroredux weather`; GPU (prefilter mips + SH): `cargo test -p byroredux-renderer gpu_filter_preserves_constant_radiance --lib -- --ignored`
- Bake: every frame before the geometry pass, from the same `CompositeParams` as the background; ready flag `exterior_sky_tint.w` = pipeline presence. Guard: `every_sky_cube_consumer_gates_on_the_ready_flag`.
- Prefilter/SH: 8 mips, 256 GGX samples per texel; 9-coefficient SH (set 1 binding 21) replaces the ambient fallback and is deliberately unoccluded, each consumer owns its occlusion (`sky_visibility_pin`).
- Clouds: one `cloud_march` for background and bake (`composite_does_not_carry_its_own_copy_of_the_sky`); height from radial altitude (`the_cloud_density_profile_uses_radial_altitude`); adaptive bounded march (`the_cloud_march_steps_adaptively`); each WTHR layer gets the mip its own tile scale needs (#4230: `every_cloud_layer_samples_with_its_own_lod`); sun term = `compute_directional_upload` illuminance.
- Weather -> sun: `translate_weather` carries HNAM `sunlight_dimmer`, `weather_system` multiplies the sampled sun colour (steady state + cross-fade, #4481). Guards: `sunlight_dimmer_translates_from_the_hnam_block`, `consumer_multiplies_sunlight_and_grass_dimmers_without_a_transition`, `promotion_carries_the_dimmers_and_the_steady_state_uses_them`. Known-open: the froxel medium's sun ignores cloud cover + dimmer (#4909); the cross-fade does not blend the sun arc (#4926). Direct-sun `exp(-2.5*coverage)`: `render/directional_upload_tests.rs::exterior_cloud_coverage_attenuates_the_direct_key`. TOD / sun arc / cross-fade / DALC easing: `systems/weather.rs` tests.
- Clock + interior: `GameTimeRes` is re-installed only by `ensure_game_time` (insert-if-absent). Interior `SkyParams` keep the cell's own fields; the outdoor sky rides separate lanes only: `exterior_zenith_color` (-> `GpuCamera::exterior_sky_tint`, window-portal escape, #3323), `portal_outdoor_sky` (bake + background through openings), `portal_sun_*` (volumetric apertures, #4839), `interior_show_sky`. Never `zenith_color` (#2226): `render/sky.rs::stale_exterior_sky_params_res_does_not_leak_into_interior`. Known-open: #4908 (no stars/moon), #4925, #4927.
- Documented, do not re-file (skyal.md §2.3-§3, 2026-09-14/18): cloud-type taxonomy, noise frequencies, atmosphere LUT, positional cloud shadows are open; coverage constants and multi-scatter a=b=c=0.5 are engine choices.

### Dimension 5: Water translation (WATAL)
**Paths**: `byroredux/src/env_translate.rs` (water arm), `byroredux/src/cell_loader/water.rs`, `byroredux/src/material_translate.rs` (`attach_mesh_water`, `water_volume_from_phantom`), `crates/core/src/ecs/components/water.rs`, `byroredux/src/commands/water.rs`
**First step**: `cargo test -p byroredux water`
- Two composition sites: ESM WATR/XCWT -> `resolve_water_material`; NIF water shader -> `attach_mesh_water` -> `water_material_from_mesh`. Any other production `WaterMaterial { .. }` outside `#[cfg(test)]` is a third.
- Frames: WATR noise-layer angles go through `watr_angle_to_engine_xz` (#4727); layers the parser already filled from NAM0 are in the engine frame and skip it (#4900). Known-open (2026-09-29): +90° applied to every game on a Skyrim/FO4 census (#4910); #4911, #4929.
- SENTINEL vs AUTHORED per game is watal.md §4's table. Guard: `resolve_water_material_sentinels_are_game_invariant`. Byte offsets (Oblivion DATA shift, Skyrim DNAM tail) are decode -> `/audit-esm`; check *promotion*: each decoded field reaches `WaterMaterial` / `WaterFlow` or is open in watal.md §2. `WaterNormalEncoding` is set in the parser (FO3/FNV = `OffsetNoise`) and honoured on every normal layer in `water.frag` (source guard in `vulkan/water.rs`).
- Heights: Oblivion Z=0 when NAM2 is present, other games WRLD DNAM; XCLW tri-state (`an_absent_xclw_stays_absent`); LOD water NAM3/NAM4 is render-only (`lod_water_is_render_only_and_cannot_create_false_submersion`).
- Mesh water depth: authored `bhkSimpleShapePhantom` bounds beat the named `4 x radius` heuristic (`authored_phantom_volume_replaces_the_radius_heuristic`). Currents: WATR NAM0 velocity, REFR `XWCU` entry 0 overrides (`reference_current_overrides_the_watr_current`), CELL `XWCU` is not a velocity; flow bounded (`flow_speed_ladder_is_ordered_and_bounded`).
- Physics stays game-invariant (`grep -rn GameKind crates/physics/src` is empty). Open (watal.md): water-walking, freezing, Skyrim DNAM tail, FO76/Starfield scope.

### Dimension 6: Distant LOD and trees
**Paths**: `byroredux/src/cell_loader/{terrain_lod,terrain_lod_btr,object_lod,placement_lod,lod_bands,lod_support,lod_coverage}.rs`, `byroredux/src/streaming_helpers.rs`, `crates/bsa/examples/probe_lod_corpus.rs`
**First step**: `cargo test -p byroredux lod`; with game data, `cargo run -p byroredux-bsa --example probe_lod_corpus` (counts all three LOD families).
- One partitioning ring: `select_lod_quads` descends a 4/8/16/32 quadtree so a quad emits itself or recurses, never both. Objects use `coarsen_to_available` (#3502); terrain keeps descending (synthesis always covers). Guard: `partition_never_covers_a_cell_twice`, `missing_baked_asset_subdivides_instead_of_holing`, `coarsened_selection_is_still_a_partition`.
- Schemes are data tables: `terrain_lod_layout` (Oblivion FormID-keyed, FO3/FNV editor-ID quadtree, Skyrim+FO4 `.btr`, FO76/Starfield none); `object_lod_scheme` (`BakedBto` Skyrim+FO4+FO76 (#4488), `FalloutLegacyBlocks` FO3/FNV, Starfield none; Oblivion `DistantLOD\*.lod` -> `_far.nif` via `placement_lod_supported`). FO3/FNV ship zero `distantlod\*.lod`, but "FO3/FNV have no distant-object LOD" is a false premise (#3321).
- `.btr` is quad-local (`btr_local_to_world`), `.bto` world-absolute; a block is `.btr` or synthesized, never both (z-fight). `lod_coverage.rs`: overlap > 0 = double draw. Legacy (Oblivion/FO3/FNV) quad textures carry their image orientation from the boundary (`TranslatedTerrainLodTexture`, #4898); known-open: `.btr` samples WRAP where the NIFs author CLAMP (#4912).
- VWD: flag parsed (#1731) -> `stamp_visible_when_distant` -> `resident_vwd_refr_cells` (locks hoisted, #3142). Open (2026-09-29): full-model culling #3307. LOD entities carry `IsLodTerrain`, no BLAS, materials via the Dim 1 boundary.
- Trees: near-field geometry is design-only (`exal-trees.md` re-scoped 2026-09-07, #3808: `.spt` carries no geometry); distant trees come only through `.bto`. Known-open: Skyrim's `.btt` tree LOD + treelod atlases are unconsumed (#4913).

### Dimension 7: Acceptance gates and harness
**Paths**: `scripts/renderer-eval-groundcover.sh`, `docs/smoke-tests/{m-exteriors,w1-water-traversal,m34-day-night}.sh`
**First step**: read `docs/smoke-tests/README.md` (`--bench-hold` -> `byro-dbg` attach).
- Unit guards cannot see GPU output; a sky/cloud/ground-cover/water claim needs a captured frame (skyal.md §4: fixed camera, `--render-debug-mode composite_term` separates sky assembly from bloom/tonemap, capture twice for the noise floor).
- `m-exteriors.sh <game> static|boundary|soak|cycle|water` = cross-game readiness matrix (incl. WATR provenance, waterline delta); `w1-water-traversal.sh` = WATAL traversal; `m34-day-night.sh` = Skyrim sun response. All need Vulkan + game data.
- `renderer-eval-groundcover.sh` enforces the washout precheck (#4482, *groundcover_reference_capture_washout*): luminance sd under 0.039 (env override BYROREDUX_GC_WASHOUT_SD) fails a `gc-backlit-*` frame (the only ground-framing poses), WARNs frontlit ones; missing telemetry fails (#4509). The committed backlit references (sd ~0.01, 2026-09-20) are the veil: confirm a re-mint before trusting an A/B.
- "Chrome" surfaces: `tex.missing` first (*feedback_chrome_means_missing_textures*). The harnesses run `scripts/check-shader-artifacts.sh` before capture (#4738); a rebuilt `.spv` may still not relink (*feedback_spv_rebuild_staleness*).

## Phase 3: Merge and Report

1. Write `docs/audits/AUDIT_EXTERIOR_<TODAY>.md`: **Executive Summary** (findings by severity; verdict per tier invariant); **Per-Category Matrix** (terrain / sky / weather-sun / water / ground cover / LOD x the four invariants, boundary fn cited); **Findings**; **Known-Open Register** (dated items above, with what this pass changed).
2. Dedup per `_audit-common.md` § Deduplication (`/tmp/audit/issues.json`, prior `docs/audits/AUDIT_*`).
3. `rm -rf /tmp/audit/exterior`; tell the user the report is ready and suggest `/audit-publish docs/audits/AUDIT_EXTERIOR_<TODAY>.md` (labels: `terrain-exterior`, plus `water`, `shaders`, `doc-rot`; `game:*` if title-specific).
