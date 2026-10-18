**HEAD**: 6f44874ac · **Baseline**: AUDIT_RENDERER_2026-10-14.md (last full; 2026-10-17 was volumetrics-only and is not a baseline) · **Audited**: Dims 3, 4, 5 (exterior resources), Dim 8 water half, Dim 2 water.frag ray-query/coordinate slice, Dim 10 sky-consumption flag only; scope=exterior-area · **Unchanged since baseline (skimmed)**: the ground-cover, sky-bake and water pipeline code is nearly untouched since 2026-10-14 (only #5845 pool-builder swap, #5852 frame-serial removal and doc edits) — read in full anyway.

**AREA-SCOPED (exterior). This is NOT a full-codebase renderer audit.** Out of area, one line each: Dim 1 (AS), Dim 6 (material translation), Dim 7 (denoise/resolve), Dim 9 (skinning), Dim 11 (FSR/presentation), Dim 12 (debug/telemetry), volumetrics (covered by 2026-10-17).

# Renderer Audit — 2026-10-18 (exterior-area)

Scope: ground cover (`vulkan/groundcover.rs`, `groundcover/{construct,frame}.rs`, `groundcover_models.rs`, `groundcover_bench.rs`, `groundcover_*.comp/vert/frag`, `include/groundcover_*.glsl`, the 128 MiB arena and indirect/compute sync), sky bake (`sky_cube.rs`, `sky_cube/{filter,irradiance}.rs`, `cloud_noise.rs`, `sky.glsl`/`clouds.glsl` consumption), water (`water.rs`, `water_caustic.rs`, `water.vert/frag`), terrain sampling (`terrain_sample.glsl`, `GpuTerrainTile`), plus descriptor/lifecycle/teardown/resize for all of them.

## Executive Summary

| Severity | NEW | Findings |
|---|---|---|
| CRITICAL | 0 | none |
| HIGH | 0 | none |
| MEDIUM | 0 | none |
| LOW | 6 | REN-EXT-L-01 … REN-EXT-L-06 (L-06 is out of area, noticed in passing) |

The exterior renderer code is in good shape. Every barrier on the ground-cover scatter, interaction, model-tier, sky-bake, prefilter, SH, water-caustic and terrain-tile paths names a correct source and destination scope. Every struct mirror I diffed by hand agrees with the shader. Teardown and resize paths hold their orderings. The six findings are one state-machine latch (L-01), one draw-order rationale that the code contradicts (L-02, needs RenderDoc), two latent-hardening/test gaps (L-03, L-04), doc rot (L-05) and one out-of-area budget omission (L-06).

Baseline items in this area, re-checked at HEAD:
- REN-L-08 (blade fade bound): fixed, `const _` assertions at `GROUNDCOVER_BLADE_FADE_START` in `shader_constants_data.rs` (`dfab09c51`).
- REN-L-09 (ledger sentence): fixed (`8ee06850b`).
- REN-L-10 (`groundcover.rs` module doc): fixed (`e29256ca2`).
- REN-H-01 (non-atomic reservoir claim, out of area): fixed by #5765 (`a03cf9174`). `triangle.frag` now does `atomicMin(reservoirClaim[pixelIdx], …)` on binding 22 and `reservoirsCurr` stays `writeonly`.

**Method.** Static review only. The engine and any Vulkan device were not launched.

**Guards run** (all green, none newly `#[ignore]`d):
- `cargo test -p byroredux-renderer --lib -- groundcover sky water terrain shader_contract scene_buffer gpu_ cloud memory_budget reflect descriptors`: 453 passed, 1 ignored, 0 failed. This includes `name_diverging_glsl_rust_mirrors_stay_in_lockstep`, `every_shader_struct_is_classified`, `every_sky_cube_consumer_gates_on_the_ready_flag`, `gpu_terrain_tile_is_176_bytes`, `water_gpu_contract_layouts_are_stable`, `water_set_2_layout_matches_the_water_shaders`, `scene_layout_matches_water_shaders`, `passes_build_layouts_writes_and_teardown_through_the_shared_helpers`, `scatter_publish_barrier_covers_the_counter_readback_copy` and `counter_seed_fills_are_sequenced_after_the_zero_fill`.
- `cargo test -p byroredux --bin byroredux -- groundcover water sky chunk_cap env_translate fog`: 417 passed, 16 ignored (game-data gated).
- `scripts/check-shader-artifacts.sh`: clean (37 shaders plus the early variant byte-match glslang).
- Not run: the one ignored GPU test in the area, `gpu_filter_preserves_constant_radiance_and_broadens_a_lobe` (`sky_cube/filter/tests.rs`, needs a Vulkan device); anything needing RenderDoc or `BYRO_VALIDATION=1`; game-data tests.

## RT Pipeline Assessment (exterior slice)

- **Ground-cover scatter** reads the TLAS from `ray_query_tlas(frame)`. That is the handle the frame's own build published, withheld when the build failed (#4779). It sits after the unconditional AS_WRITE→AS_READ barrier whose dst includes COMPUTE.
- **Scatter publish barrier** (`record_scatter`): COMPUTE write → DRAW_INDIRECT | VERTEX | TRANSFER, with the TRANSFER half covering the counter readback copy (#4181). The zero fill and the seed fills are sequenced by a TRANSFER→TRANSFER barrier (#4293).
- **Interaction field** (`record_interaction`): the persistent two-half field is ordered COMPUTE→COMPUTE before the dispatch and COMPUTE→COMPUTE|VERTEX after. The cross-frame read of the previous half is covered by a barrier, not only by the all-slots fence wait.
- **Model tier**: the leading barrier orders its shared slab, counters and draws after every earlier frame's reads. The EMIT-phase barrier publishes to DRAW_INDIRECT | VERTEX | FRAGMENT. No compute pass reads the instance-SSBO tail, so FRAGMENT-only visibility is sufficient (caustic splat only indexes blended instances, and the tier never sets blend).
- **water.frag** rays: every exit goes through `traceWaterRay`, which is gated on `sceneFlags.x`, uses the alpha-skip walk bounded by `MAX_ALPHA_SKIP_LAYERS`, and cannot self-hit because water planes are excluded from the TLAS (`is_water` in `predicates.rs`). The sky miss goes through `exteriorSkyRadianceOr` behind `jitter.w`. The `sin()` phases in `water.vert` take absolute XZ, but the phase magnitude stays well inside f32 range under `RT_ABSOLUTE_PRECISION_CEILING`.

## GPU-Struct & Memory Assessment

- **Hand diffs, all agreeing**:
  - `ScatterPush` (48 B) ↔ `GcScatterPush`
  - `BladePush` (64 B) ↔ `GcBladePush`
  - `ModelPush` (64 B) ↔ `GcModelPush`
  - `GpuTerrainTile` (176 B) ↔ `bindings.glsl`
  - `GroundCoverCell`, `GroundCoverChunk`, `GroundCoverSpecies` and `GroundCoverBlade` ↔ `groundcover_scene.glsl`
  - `GpuWaterParams` (368 B, 23 slots) ↔ the water shaders
  - `SkyCubeParams` ↔ `SkyDome`
- **Ground-cover ledger** re-derived: arena 512 × 16,384 × 16 B = 134,217,728 B; indirect 512 × 16 B × 3 = 24,576 B; counters 539 × 4 = 2,156 B. This matches `memory-budget.md` and the `memory_budget_ledgers_the_sky_and_ground_cover_owners` test.
- **Teardown order** holds: `groundcover`, `groundcover_models`, `groundcover_bench` and `sky_cube` are destroyed in the allocator block. `cloud_noise` is destroyed after both of its consumers (`sky_cube`, `composite`). `shadow_filter` goes before `reservoir_buffers`. `water` frees its parameter SSBOs through its own allocator clone before `Arc::try_unwrap`.
- **Resize** holds:
  - `water` and the ground-cover draw pipelines are rebuilt only when the surface format changes. A failed ground-cover rebuild leaves null pipelines that `record_draw` skips (#4307).
  - `WaterCausticAccum` is recreated; on failure it is dropped and set 2 is rebound to the sink placeholder (#2142).
  - The sky cube, cloud noise and ground-cover arenas are not size-derived.

## Findings

### REN-EXT-L-01: ground-cover interaction field's "previous half is valid" flag is latched at `prepare`, not when the dispatch is recorded
- **Severity**: LOW
- **Dimension**: Sync/Barriers (state machine) · Memory/Lifecycle
- **Location**: `GroundCoverPipeline::prepare` → `advance_field_state` (`groundcover/frame.rs`); early-out in `GroundCoverPipeline::record_scatter` when `tlas` is `None`; field allocated in `groundcover/construct.rs` (`create_device_local_uninit(… STORAGE_BUFFER | TRANSFER_DST)`); `groundcover_interaction.comp`
- **Status**: NEW
- **Description**: `advance_field_state` flips `field_write_half` and sets `field_previous = Some(origin)` inside `prepare`, which runs before `draw_frame`. The dispatch that actually writes that half is recorded later in `record_scatter`. That function returns before `record_interaction` whenever `ray_query_tlas(frame)` is `None`, i.e. the frame's TLAS build failed or was skipped. The same early-out follows an upload failure inside `prepare`, which advances the state and then returns `false`. So the host's `previous.w = 1.0` ("the previous half holds a field") can be true for a half the GPU never wrote.
  - The field buffer is created uninitialised. `TRANSFER_DST` is requested but nothing ever fills it.
  - `groundcover_interaction.comp` does not sanitise what it reads: a NaN in `value` fails `dot(push, push) > dot(value, value)` and persists, then is copied forward by reprojection until the camera leaves the field extent.
- **Evidence**: `advance_field_state` call at `prepare`; `let Some(tlas) = tlas else { … return; }` before `self.record_interaction(device, cmd, frame)`; no `cmd_fill_buffer` on `field_buffer` anywhere.
- **Impact**: Mid-session a skipped frame only resurrects a stale trail for a frame. The bad case needs the very first live ground-cover frame to be skipped with a never-written half and non-zero uninitialised VRAM (fresh device-local allocations are usually zero-filled by drivers, but the spec does not promise it). Then blades near those texels would be displaced by garbage or NaN until the camera moves about one field extent. Not reproduced; severity reflects low likelihood.
- **Suggested Fix**: advance `field_previous` / `field_write_half` at record time (in `record_interaction`), or zero both halves once at creation with a fenced one-time fill (the buffer already has `TRANSFER_DST`). Neither changes any barrier or layout.

### REN-EXT-L-02: blade draw is ordered after water, but its stated reason ("a blade below a water plane must be composited under it") implies the opposite order
- **Severity**: LOW (visual, shoreline only; unverified)
- **Dimension**: Water · Pipeline/RenderPass
- **Location**: `record_geometry_pass` (`context/geometry_pass.rs`): the water block, then the "EXAL ground cover" `gc.record_draw` block; rationale comment dating from `637b65264`; `draw_groundcover_models` runs before water; `byroGcMoisture` / `GROUNDCOVER_MOISTURE_SUBMERGED_DEPTH` (24 units)
- **Status**: NEW
- **Description**: The water pipeline writes no depth, so a later opaque, depth-writing blade fragment that is nearer than the terrain passes its depth test and overwrites the water pixel. The density field allows roots up to 24 units below the plane (`byroGcMoisture` only returns 0 beyond that depth), so shoreline blades have a submerged portion. Those pixels get the blade's colour instead of being refracted, absorbed or blended by water. To composite "under" water the blades would have to be drawn first so the water's depth test and blend run over them. The model tier, which does carry underwater kelp, is already drawn before water. `shader-pipeline.md` documents the current order as intentional, so the comment and docs agree with each other and the behaviour disagrees with the rationale.
- **Impact**: shoreline grass tips and stems show through opaque water without water shading, at depths of at most 24 units. Likely subtle; it may even read as natural marsh grass. Not measured.
- **Needs RenderDoc**: capture a grass shoreline (FNV Lake Mead, Skyrim Riverwood) and compare blade pixels below the plane against a build that draws blades before water. Per the no-speculative-Vulkan rule this audit stops at the observation.

### REN-EXT-L-03: no static bound on the 32-bit vertex span the blade tiers address
- **Severity**: LOW (latent)
- **Dimension**: GPU-Struct
- **Location**: `groundcover_scatter.comp` (`firstVertex = sliceBase * vertsPerPoint`, `vertexCount = accepted * vertsPerPoint`); `groundcover_blade.vert` (`uint(gl_VertexIndex) / vertsPerPoint`); constants in `shader_constants_data.rs`
- **Status**: NEW
- **Description**: Tier 0 spans `GROUNDCOVER_MAX_CHUNKS × GROUNDCOVER_MAX_BLADES_PER_CHUNK × (15 × 4)` = 503,316,480 vertex indices, about 11.7 % of the u32 range. Nothing asserts that, and the constants have been retuned twice (candidate budget ×4 on 2026-09-16, chunk cap ×2 on 2026-10-14). A further ×8.5 would wrap `firstVertex` and `gl_VertexIndex` silently. The sibling model-tier span has exactly this kind of assertion (`GROUNDCOVER_MODEL_POINTS_PER_CHUNK << GROUNDCOVER_MODEL_RECORD_BITS <= 1 << 32`).
- **Suggested Fix**: add a `const _` beside `GROUNDCOVER_MAX_CHUNKS` asserting `MAX_CHUNKS × MAX_BLADES_PER_CHUNK × max(verts per point over tiers) <= u32::MAX`.

### REN-EXT-L-04: ground-cover push-constant blocks have no field-for-field lockstep guard; two use hand-rolled byte casts
- **Severity**: LOW (test gap)
- **Dimension**: GPU-Struct
- **Location**: `ScatterPush` / `BladePush` in `groundcover.rs` with `scatter_push_bytes` / `blade_push_bytes` in `groundcover/frame.rs`; `ModelPush` in `groundcover_models.rs`
- **Status**: NEW
- **Description**: The descriptor-set side of these pipelines is reflection-validated and the storage records run through `assert_glsl_matches_rust`. The push blocks are not. `push_block_fits_the_guaranteed_minimum` pins only `BladePush` = 64 B, `ScatterPush` ≤ 128 B and the byte-52 offset of the previous-clock lane. `ModelPush` sizes through `NoUninit`/`byte_view` (#5122); the other two push structs use `slice::from_raw_parts`. I diffed all three by hand against their GLSL blocks and they agree today, so this is drift risk only.
- **Suggested Fix**: add the three blocks to the mirror table, or reflect push-constant member offsets in `reflect.rs`; route the two byte views through `byte_view` with a `NoUninit` impl.

### REN-EXT-L-05: doc rot after the grass-LOD rewrite (`af8323881`)
- **Severity**: LOW (doc rot)
- **Dimension**: Debug/Telemetry (docs)
- **Location**:
  - `groundcover/frame.rs`, comment in `record_draw`: "Stream 0 is the three-segment tier and stream 1 the one-segment tier" (there are three streams, 3/2/1 segments).
  - `groundcover/frame.rs`, comment in `record_scatter`: "the blade tier is `segments * 6`" (the live formula is `3 * (2N - 1)`).
  - `groundcover.rs`, `BladePush` doc: lane `w` (the tier index) is not described.
  - `docs/engine/exal-groundcover.md` status header: still says "Tier 2 palette-atlas clump cards and their area fade IMPLEMENTED (2026-09-15)", while the same file marks that tier *(removed)* further down.
- **Status**: NEW
- **Suggested Fix**: edit the comments and the header sentence.

### REN-EXT-L-06 (OUT OF AREA, noticed while checking the BLAS reservation): the new shadow-filter images and the reservoir tail regions are not billed in `render_extent_bytes_per_pixel_x1024` and not ledgered in `memory-budget.md`
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `SHADOW_FILTER_BYTES_PER_PIXEL` (`shadow_filter.rs`, defined but referenced nowhere else); `render_extent_bytes_per_pixel_x1024` and its independent twin in `acceleration/tests/predicates_tests.rs`; `ReservoirBuffers` (`restir.rs`: `CLAIM_STRIDE` 4 B and `UNSHADOWED_STRIDE` 8 B tails); the "ReSTIR Reservoirs" section of `memory-budget.md`
- **Status**: NEW (out of the exterior area; not deduped against sibling suites)
- **Description**: The reservation counts `RESERVOIR_STRIDE × MAX_FRAMES_IN_FLIGHT` (64 B/px). Since #5765 the reservoir buffer is 32 + 4 + 8 = 44 B/px per slot, and `ShadowFilterPipeline` holds 24 B/px per slot (three RGBA16F images). The doc formula still reads `RESERVOIR_STRIDE + 4`, and the page has no shadow-filter row. The independent-list guard repeats the omission, so it passes.
- **Impact**: `blas_budget_for_heap` reserves about 72 B/px too little: around 150 MB at 1080p render extent and around 600 MB at 4K native. The BLAS budget is one third of the unreserved remainder, so the budget is over by a third of that (about 50 MB at 1080p). Eviction starts slightly late on small-VRAM cards.
- **Suggested Fix**: add both terms to `render_extent_bytes_per_pixel_x1024` and to the guard's list, and ledger them in `memory-budget.md`.

## Prioritized Fix Order

1. **Correctness / robustness**: REN-EXT-L-01 (move the latch to record time or zero the field once). Nothing else here changes behaviour.
2. **Verify then decide**: REN-EXT-L-02 needs a RenderDoc or visual shoreline check before any order change.
3. **Hardening / tests**: REN-EXT-L-03 (const assert), REN-EXT-L-04 (push-block lockstep).
4. **Budget (out of area)**: REN-EXT-L-06.
5. **Docs**: REN-EXT-L-05.

## Needs-RenderDoc

1. **Blade vs water order (L-02)**: grass shoreline; blade pixels below the plane against a blades-first build.
2. **Interaction field after a skipped first frame (L-01)**: force `tlas = None` on the first ground-cover frame (or log the halves) and inspect the field for non-finite texels.
3. **Shadow-filter correction under ground cover and water (observation only, not a finding)**: blades and water never claim a reservoir, so at those pixels the terrain beneath still owns the slot. Composite then adds the terrain's `U × (filtered − raw)` correction onto the blade or water colour (`shadow_filter.comp` stores a correction wherever the claimed history length is non-zero). The magnitude is bounded by the terrain's U inside penumbra bands (reach about 6 px). It may be invisible or mildly beneficial, since blades anchor their luminance to the terrain. Check on a sunlit shoreline.
4. **Not run**: `gpu_filter_preserves_constant_radiance_and_broadens_a_lobe` (needs a Vulkan device).

## Stale skill premises (for the next `/audit-renderer` sync)

- **Dim 3**: the `GpuInstance` mirror list omits `skin_vertices.comp`, which also declares `struct GpuInstance` (the lockstep test covers it). Add the ground-cover push blocks (`ScatterPush`, `BladePush`, `ModelPush`) as an unguarded class (L-04).
- **Dim 4**: the frame-shape line should note that blade draw is after water (L-02) and that `shadow_filter` is dispatched inside the SVGF bracket.
- **Dim 5**: `shadow_filter` is a new allocator owner (destroyed before `reservoir_buffers`); `GroundCoverPipeline` now builds its pool through `DescriptorPoolBuilder::from_layout_bindings` (#5845).
- **Dim 8**: the "Guard" list should name the `draw_write_masks_match_each_fragment_shaders_outputs` bin/lib pair for blades.
- **Exterior-owned numbers**: the ground-cover arena is 512 chunks, 128 MiB. The frame serial is gone from the blade push word (#5852; the word is now just the tier index).

## Process notes

**Dedup.** Open-issue snapshot `/tmp/audit/issues.json` plus `gh` searches for ground-cover interaction field, push-constant lockstep, shadow-filter reservation, submerged blades and `firstVertex`: no matches for any of L-01…L-06. Sibling exterior audit (`AUDIT_EXTERIOR_2026-10-18.md`) items, cited and not re-reported:
- EXT-D3-01 MEDIUM: authored-model tier squares density (`groundcover_models.comp`).
- EXT-D1-01 MEDIUM: deferred texture uploads left queued after Show-Sky / forced weather.
- EXT-D3-02 LOW: `GROUNDCOVER_BLADE_FADE_START` doc.
- EXT-D1-02 LOW and EXT-D6-01 LOW.

I read `groundcover_models.comp` only for its push block and `GpuInstance` mirror, not its density logic.

**Dropped candidates.**
- Missing `multiDrawIndirect` gate on the blade draw: gated at `GroundCoverPipeline::new` by `indirect_draws_supported()` (#4888).
- Sky-bake pre-barrier uses `src FRAGMENT_SHADER` with `UNDEFINED`: write-after-read against earlier compute readers is covered by the all-slots fence wait, and no compute stage samples the cube.
- SH buffer published only to FRAGMENT: all readers (`triangle.frag`, `groundcover_blade.frag`, `water.frag`) are fragment shaders.
- Terrain-tile upload barrier dst is FRAGMENT only: `terrainTiles[]` is read only by `triangle.frag` and `groundcover_blade.frag`.
- `CloudNoiseVolumes::new` destroys on any error including `MaybeInFlight`: init-only path where the error aborts boot.
- `water_commands` growth destroys the old parameter SSBO immediately: per-slot buffer, slot fence waited.
- A stale `vertexOffset` between `prepare_groundcover` and the scatter: the scatter reads the handle resolved at `prepare`, and `build_geometry_ssbo` is not reachable between them from this audit's reading. Unverified for geometry rebuilds mid-`draw_frame`; owned by the full renderer audit.
- Unverified, out of area: the rebuild of the geometry SSBO relative to `record_scatter` ordering.
