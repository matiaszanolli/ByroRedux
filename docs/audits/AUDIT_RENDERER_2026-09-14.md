# Renderer Audit — 2026-09-14 (full sweep, all 23 dimensions)

**Scope**: `/audit-renderer`, all 23 dimensions, `--depth deep`, against HEAD `147d97c3`.
Dimensions ran as independent agents (three dimension groups re-ran after the
first attempt failed without output). Every NEW finding below was then
re-checked by the orchestrator against live source before inclusion.
Cross-checked against `docs/audits/AUDIT_RENDERER_2026-09-11.md` and the full
issue list (98 open at sweep time). Reference docs (`docs/engine/shader-pipeline.md`,
`docs/engine/memory-budget.md`) were used as ground truth; where code has moved
past them, that divergence is reported as doc-rot.

**Delta under audit**: 108 commits since 2026-09-11, ~7.3k inserted lines under
`crates/renderer` + `byroredux/src/render`. The bulk is new code no dimension
checklist names: the SKYAL sky cubemap bake (`crates/renderer/src/vulkan/sky_cube.rs`),
volumetric clouds (`crates/renderer/src/vulkan/cloud_noise.rs`, `crates/renderer/shaders/include/clouds.glsl`),
EXAL ground cover (`crates/renderer/src/vulkan/groundcover.rs` + four shaders),
the bloom bright-pass, and fixes #4177/#4179/#4181/#4187/#4191–#4197.

## Executive Summary

**28 findings** — **0 CRITICAL, 2 HIGH, 6 MEDIUM, 20 LOW**. 27 are NEW; one
(D11-02) extends open #4110.

The sweep is dominated by one pattern: **the newly landed ground-cover and sky
systems were integrated into most, but not all, of the renderer's standing
contracts.** 11 of 28 findings sit on ground cover or the sky cube:

- **Ground cover** receives no direct sun at all (D18-01, HIGH), is rasterized
  without the TAA/FSR projection jitter (D23-01), writes both FSR masks at 1.0
  against the documented mask contract (D23-02), writes undefined albedo in its
  debug-point view (D11-01), carries an unbarriered triple buffer fill that
  likely explains a standing validation warning (D4-01), and sits outside the
  GPU timers, reflection guards, surface-format recreate, VRAM ledger and
  submission-order doc (D20-01, D11-02, D11-03, D5-01, D12-01).
- **The sky cubemap** reached two of five exterior sky-miss sites; glass and
  water reflections still see the flat pre-SKYAL sky (D2-01), and its ready
  flag lives in a lane the layout doc still calls reserved (D3-01).

Outside that cluster: an **MSWP material swap** keeps the source BGSM's
non-texture material response (D6-01, HIGH, NIFAL floor), and **evicted
MorphSlots are never recreated**, permanently freezing facial morphs on an NPC
that misses the skin dispatch for three frames (D9-01).

The remainder is doc-rot left behind by fixes (#4046 → D8-01, #3901 → D6-02,
#4023 → D21-01, #4287 → D6-04, the bright-pass → D16-02) and comments that
overstate what code does (D16-01, D13-01, D14-01, D18-03).

**Pipeline areas affected**: EXAL ground cover (raster, FSR masks, lighting,
sync, telemetry), SKYAL sky/clouds, NIFAL material overlay, GPU skinning/morph,
bloom, plus reference-doc drift in `shader-pipeline.md` and `memory-budget.md`.

**Clean**: D1 (acceleration structures), D7 (material table), D10 (camera-relative
precision), D15 (water), D17 (Disney BSDF / soft shadows), D19 (tangent space),
D22 (light animation).

## RT Pipeline Assessment

**AS correctness and SSBO indexing are clean.** #4196 per-mesh BLAS admission
cannot spin or evict in-flight BLAS; #4177 and #4179 barriers are in place and
source-pinned (GPU-side effect still needs RenderDoc). Ground cover adds no TLAS
instances and no `GpuInstance` entries, so the `instance_custom_index` ↔ SSBO
contract is untouched; its scatter compute reads the TLAS after the #2931
barrier. The `integrity_snapshot()` → `rt.integrity` chain and
`restore_missing_static_blas_for_draws` ordering are intact.

**Ray-query safety holds** for every consumer, including the two new ones
(`groundcover_scatter.comp`, `groundcover_blade.frag`), both of which use
absolute-space origins against the absolute TLAS. The one ray-side defect is
coverage, not safety: three exterior sky-escape sites still bypass the baked
cube (D2-01). Interior gating is correct — no daylight leaks into GI or
reflection misses — and the irradiance/π conversion survived the cube change.

**Denoiser stability**: SVGF, TAA and composite reassembly are correct; the
findings there are documentation of #4046's changed signal (D8-01, D8-02) and an
untested alpha pass-through that composite's sky branch depends on (D13-01).
Open #3572 (TAA bypass of composite-stage radiance) still stands.

## GPU-Struct & Memory Assessment

**Layout pins are all correct**, re-derived from source: `GpuInstance` 160 B
across 5 named mirrors plus `GpuBoundaryInstance`, all carrying `surfaceId`;
`GpuCamera` 368 B; `GpuMaterial` 428 B, 107 scalar fields, all 107 hashed;
`DBG_BITS` 35 entries; capacity constants unchanged. The new `SkyCubeParams` is
pinned field-for-field; the ground-cover structs match by hand under their
documented `MirroredPendingGuard` classification. The only struct finding is a
repurposed lane still documented as free (D3-01).

**Lifecycle is clean** for all three new owners (sky cube, cloud noise, ground
cover): paired create/destroy with partial-init cleanup, correct teardown order
inside the allocator-owned block, resize handling. #4187/#4196/#4197 hold. The
gaps are observability: ~18.5 MiB of new resident VRAM is absent from
`memory-budget.md` (D5-01), and ground cover has no GPU timer bracket (D20-01).

## Findings

IDs use `REN-2026-09-14-D<N>-NN`. Grouped by severity, then dimension.
REN-2026-09-14-D11-04 (a duplicate of D2-02 + D3-01) was merged and is omitted.
Two findings were adjusted during orchestrator verification and say so inline:
D18-01 (raised MEDIUM → HIGH) and D11-02 (status → Existing #4110).


---

# HIGH

### REN-2026-09-14-D6-01: An MSWP material swap swaps the textures but keeps every non-texture material scalar from the *source* BGSM
- **Severity**: HIGH
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/cell_loader/refr.rs` (`RefrTextureOverlay::fill_from_bgsm`, `build_refr_texture_overlay`); `byroredux/src/cell_loader/spawn/mesh_instance.rs` (`resolve_mesh_paths`, `spawn_mesh_instance`); `byroredux/src/material_translate.rs` (`translate_material`, `bgsm_authored_scalars_are_never_reclassified_by_an_overlay_swap`)
- **Status**: NEW
- **Description**:
  - An XMSP swap substitutes the BGSM path at REFR level (`build_refr_texture_overlay`) and per shape (`resolve_mesh_paths`, #973).
  - Both then call `fill_from_bgsm`, which fills **texture paths only** from the swap target.
  - `spawn_mesh_instance` then calls `translate_material(&mesh.material, …)`. `mesh.material` was merged at import time (`byroredux/src/cell_loader/references/import.rs` → `merge_external_material`) with the mesh's **original** BGSM.
  - The swap target's alpha test/threshold, two-sided, emissive, smoothness and PBR overrides, translucency, glass flags, greyscale bits and UV transform never reach `Material`. The source's values ride onto the target's textures.
  - #4229's `recomputed_pbr` skips `bgsm_pbr_scalars_authored` sources on any overlay swap, and its test pins that. That is right for a TXST texture swap and wrong for an MSWP material swap.
- **Evidence**:
  - `fill_from_bgsm`'s `.bgsm` and `.bgem` arms contain only `Self::fill(&mut self.<texture_slot>, …)`.
  - There is no `merge_external_material` call in `byroredux/src/cell_loader/spawn/mesh_instance.rs`, `spawn.rs` or `refr.rs`.
  - `extra_material_flags` carries only the TXST model-space-normals bit.
- **Impact**: FO4 placements whose MSWP target authors different alpha/two-sided/emissive/glass/PBR data render the target's textures with the source's material response. That is a wrong `Material` out of the translate boundary, the NIFAL HIGH floor. Vanilla prevalence is **unmeasured**.
- **Related**: #973, #4229, #2708, #4287/#4288.
- **Suggested Fix**: When the effective `material_path` comes from an MSWP swap, run `merge_external_material` on a clone of `mesh.material` against the swapped path before `translate_material`, replacing texture-only `fill_from_bgsm` for that half. Scope the #4229 "never reclassify BGSM scalars" test to TXST provenance.

### REN-2026-09-14-D18-01: ground-cover blades multiply every directional light by `params.x`, which `collect_lights` uploads as 0.0 — grass gets no sun, no traced shadow, no canopy shadow and no translucency
- **Severity**: HIGH (raised from MEDIUM during orchestrator verification: it removes all direct sun, traced shadow and translucency from every exterior blade — a rendering-correctness defect, which the severity decision tree floors at HIGH)
- **Dimension**: Sky/Weather
- **Location**: `crates/renderer/shaders/groundcover_blade.frag` (`main`, the `incoming` term); `byroredux/src/render/lights.rs` (`collect_lights`)
- **Status**: NEW
- **Description**:
  - The blade pass lights grass only from directional lights (`color_type.w >= 1.5`) and scales each one's radiance by `lights[i].params.x`.
  - `params.x` is the falloff exponent in the `GpuLight` contract (`crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` table; `lighting.glsl` reads it as `falloffShape`).
  - `collect_lights` builds the only directional light in the SSBO, which is both the exterior WTHR sun and the interior XCLL key light. It sets `params: [0.0, 0.0, VisibilityMask::FULL.bits() as f32, …]` and comments that 0.0 is "the 'use default' sentinel so the shader's directional branch ignores it cleanly".
  - As a result `incoming` is exactly zero for the sun every frame. So `lit` and `transmitted` are always zero.
  - The blade's final colour reduces to `albedo * sheenAmbient`, plus composite's `indirect * albedo` from the ground GI left under it (commit `4d54c73a`).
- **Evidence**:
  - `groundcover_blade.frag`: `vec3 incoming = lights[i].color_type.rgb * lights[i].params.x * shadow * canopy;`, followed by `lit += incoming * (diffuse + sheenLobe); transmitted += incoming * (lobe * bladeTransmittance);`.
  - `lights.rs` `collect_lights`: `color_type: [dir_color…, 2.0]`, `params: [0.0, 0.0, VisibilityMask::FULL.bits() as f32, AttenuationModel::LegacySoftRange as u8 as f32]`.
  - No renderer-side pass rewrites `GpuLight.params`. A grep of `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs` and `crates/renderer/src/vulkan/scene_buffer/upload.rs` found no writer.
  - The canonical directional path does not use the factor. `shadowableLightRadiance` uses `atten = 1.0` and `lightColor * atten`.
  - History: `params.x` has been 0.0 for directionals since `fc338d90` (2026-05-25). The factor has been in the blade shader since Phase 2 (`637b6526`, `lit += … * (wrapped * lights[i].params.x)`) and was carried into #4057's rewrite (`b01ef926`). The contribution has therefore been zero since the shader first shipped.
  - No test pins the factor. A grep for `params.x` in ground-cover tests found none.
- **Impact**:
  - Every exterior ground-cover blade, in every game, is ambient/GI-lit only.
  - Lost with it:
    - the sun's diffuse and sheen lobes;
    - the per-blade traced world shadow (so rocks and trees cast no shadow on grass);
    - the §12.5 canopy transmittance;
    - the §12.2 backlit translucency that #4057 shipped.
  - All of these are computed (including the shadow ray trace on `sceneFlags.x`) and then multiplied by zero, which also wastes one shadow ray per lit blade fragment.
  - The 4d54c73a "sits in the scene's exposure" verification was done in this state, so the §12 lobe calibration has never been observed lit.
- **Related**: #4057 (closed; ground cover light response), `4d54c73a` (GI coupling), D6 (ground cover builds no Material).
- **Suggested Fix**:
  - Drop the `params.x` factor so directional radiance is `color_type.rgb`, matching `shadowableLightRadiance`, and add a shader-source pin against re-multiplying by the falloff lane.
  - Grass brightness will change substantially. Re-check the §12 calibration with a screenshot A/B, not by tuning constants.


---

# MEDIUM

### REN-2026-09-14-D2-01: Sky-cubemap adoption covers only two of five exterior sky-miss sites — glass face-on reflection, glass refraction miss and water reflection miss still return the flat zenith blend
- **Severity**: MEDIUM
- **Dimension**: Ray Queries (Sky/Weather)
- **Location**: `crates/renderer/shaders/triangle.frag` (`isExteriorGlass` `reflColor` fallback in the glass IOR block; `refrColor` miss arm in the glass refraction loop); `crates/renderer/shaders/water.frag` (`reflectionMiss`); pinned by `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` (the `reflectionMiss = mix(sceneFlags.yzw, skyTint.xyz, skyWeight);` assertion, and `path_environment_radiance_cites_live_sky_tint_evidence`'s `MISS_BLEND`)
- **Status**: NEW
- **Description**: `6db9eac2` replaced the direction-blind exterior miss with a `texture(skyCube, dir)` sample in `traceReflection` (`raytrace.glsl`) and `pathEnvironmentRadiance` (`lighting.glsl`). Its commit message and `docs/engine/skyal.md` describe that as the end of "every sky reflection … the same radiance toward the sun and away from it". Three other exterior sky-escape sites were not converted and still return the pre-SKYAL flat value:
  1. The glass face-on reflection, taken when `fresnelScalar <= 0.05`: `reflColor = isExteriorGlass ? (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5) : sceneFlags.yzw`. Above the 0.05 threshold the same fragment calls `traceReflection`, which now misses into the cube. The sky seen in exterior glass therefore changes discontinuously at a Fresnel threshold: cube-sampled at grazing angles, flat at face-on angles.
  2. The glass refraction-ray miss: `refrColor = isExteriorGlass ? (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5) : …`. Its own comment says it exists to "Match the `traceReflection` miss fallback above on the same gate". That equivalence stopped being true in `6db9eac2`, because the string survives in `raytrace.glsl` only as the no-bake branch.
  3. The water reflection miss: `reflectionMiss = mix(sceneFlags.yzw, skyTint.xyz, skyWeight)` in `water.frag`. Water is the most sky-dominated reflective surface in an exterior, and it now reflects a different, cloudless and azimuth-free sky than a mirror-like metal or glass edge a few metres away.

  `water.frag` already `#include`s `crates/renderer/shaders/include/bindings.glsl`, so it declares both `skyCube` and `CameraUBO.exteriorSkyTint`. The water pipeline layout uses the shared `scene_set_layout` at set 1 (`water.rs`), and binding 20 is `FRAGMENT`-visible. The cube is therefore reachable from all three sites with no layout change.

  `docs/engine/skyal.md`'s TODO list names prefiltered mips, irradiance projection and cloud type, but not these sites, so this isn't a known deferral.
- **Evidence**:
  ```glsl
  // raytrace.glsl — converted
  missCol = exteriorSkyTint.w > 0.5 ? texture(skyCube, direction).rgb
                                    : (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5);
  // triangle.frag — glass face-on fallback, NOT converted
  vec3 reflColor = isExteriorGlass ? (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5) : sceneFlags.yzw;
  if (fresnelScalar > 0.05) { vec4 reflRay = traceReflection(...); ... }
  // triangle.frag — refraction miss, comment claims parity with traceReflection
  refrColor = isExteriorGlass ? (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5) : sceneFlags.yzw;
  // water.frag — NOT converted
  reflectionMiss = mix(sceneFlags.yzw, skyTint.xyz, skyWeight);
  ```
  `rg -n 'skyCube' crates/renderer/shaders` returns only `bindings.glsl` (declaration), `raytrace.glsl` and `lighting.glsl`.
- **Impact**: Visual only; no NaN, index or descriptor hazard (all three sites are exterior-gated and read only UBO scalars). Exterior glass shows a sky seam at the Fresnel threshold. Exterior water reflects a flat zenith/ambient gradient while adjacent RT reflections show the directional sun and baked clouds. The whole-frame sky-consistency goal of SKYAL is only partly met. Two contract tests pin the stale flat strings, so a contributor who notices will hit test failures and may conclude the flat blend is intended.
- **Related**: `6db9eac2`, `c379898f`; #3323 / #2226 (exterior-sky lanes); #3620 (`path_environment_radiance_cites_live_sky_tint_evidence`); `every_sky_cube_consumer_gates_on_the_ready_flag` (hand-listed, with no discovery walk).
- **Suggested Fix**:
  - Route all three sites through the same gated selection `traceReflection` uses. That means `exteriorSkyTint.w > 0.5 ? texture(skyCube, dir) : <current fallback>`, keeping the directional `smoothstep` horizon blend on water.
  - Update the two pinning tests to accept the gated form.
  - Make `every_sky_cube_consumer_gates_on_the_ready_flag` discover every `texture(skyCube,` user instead of listing two files.

### REN-2026-09-14-D4-01: the ground-cover counter clear issues three overlapping `vkCmdFillBuffer`s with no barrier between them — the only site in the crate that can explain the "10 vkCmdFillBuffer WAW hazards" noted in a commit message
- **Severity**: MEDIUM. Rises to HIGH (the "validation errors in normal operation" floor) if a `BYRO_VALIDATION=1` run attributes those hazards to these calls.
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/groundcover.rs` (`GroundCoverPipeline::record_scatter`, `buffer_barrier`)
- **Status**: NEW
- **Description**:
  - `record_scatter` clears the shared (not per-FIF) `counter_buffer` in three steps: a whole-buffer zero fill, then two 4-byte `EXTREMA_MIN_SEED` fills into ranges the zero fill already wrote. Only after all three does it emit a `TRANSFER → COMPUTE` barrier.
  - Nothing separates the seed fills from the zero fill. Under the sync model this crate follows (#4177/#4179/#4181), their order is undefined, and sync validation reports this pattern as WRITE_AFTER_WRITE.
  - Strict-reading secondary: `counter_buffer`, `blade_buffer` and `indirect_buffer` are single allocations shared by both FIF slots. The next frame's pre-scatter barrier's source stage is TRANSFER only, so it does not cover the previous submission's VERTEX / DRAW_INDIRECT reads or its readback copy.
  - That secondary gap is serialized host-side by the both-slot fence wait, but #4177/#4179 hold that a host wait is not a device edge.
  - The comment in `record_interaction` ("consecutive frames can overlap on the queue") is contradicted by that same both-slot wait.
- **Evidence**:
  - Only three production `cmd_fill_buffer` sites exist:
    - `compute.rs`: one fill of the per-FIF telemetry buffer, followed immediately by a buffer barrier.
    - `restir.rs` `zero_fill`: one fill per distinct buffer in a one-time submit.
    - `groundcover.rs`: the three overlapping fills above.
  - The `6db9eac2` commit message, recorded against a `BYRO_VALIDATION=1` run on a Skyrim SE terrain capture: "the same 10 `vkCmdFillBuffer` WAW hazards, all pre-existing and none of them mine".
  - Two intra-command-buffer WAW pairs per scatter frame fits that count. I did not re-run validation (engine launch is out of scope).
- **Impact**:
  - Rendering is unaffected.
  - Under the strict reading, if the zero fill lands last, the `atomicMin` extrema start at 0, so `GroundCoverStats::d_ground_min` and `view_dist_min` read 0. This is the telemetry-only class of #4181, and EXAL tuning reads it directly.
  - The standing validation noise can also hide a real new hazard.
- **Related**: #4181, #4177, #4179, #4182 (open)
- **Suggested Fix**: Needs `BYRO_VALIDATION=1` verification on an exterior with ground cover first. If these fills are confirmed as the source, a TRANSFER→TRANSFER memory barrier between the zero fill and the seed fills should clear the hazards. Verify by re-running validation, not by `cargo test`.

### REN-2026-09-14-D9-01: an LRU-evicted `MorphSlot` is never recreated — any entity that misses the skin dispatch list for more than `MAX_FRAMES_IN_FLIGHT` frames permanently loses morph-target deformation, including on the raster path
- **Severity**: MEDIUM
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (`record_skinned_blas_refit` — the MorphSlot LRU bump inside the dispatch loop, and the `morph_evictees` sweep); `byroredux/src/cell_loader/spawn/mesh_instance.rs` (`spawn_mesh_instance` — the sole `ctx.morph_slots.insert`)
- **Status**: NEW
- **Description**: `MorphSlot`s are created exactly once, at spawn (`create_morph_slot_for_mesh` → `ctx.morph_slots.insert(entity, slot)` is the only insertion site; `morph_compute.rs`'s module doc confirms "created once at spawn … rather than lazily on first dispatch"). But their eviction was modelled on `SkinSlot`, which *is* lazily recreated: the sweep drops any MorphSlot whose `last_used_frame` trails `frame_counter` by `MAX_FRAMES_IN_FLIGHT + 1` frames. The only place `morph_slot.last_used_frame` is refreshed is inside the per-entity skin dispatch loop, after these gates: the draw must have `bone_offset != 0`, its mesh must be `rt_capable`, the entity must have a live `SkinSlot` (`self.skin_slots.get_mut(&entity_id) else continue` runs *before* the morph bump), and the whole block needs a live `skin_compute` + `accel_manager` + global vertex buffer + bone buffer. Any entity that fails one of those for 3+ frames has its MorphSlot destroyed, and nothing ever creates a new one. Once the entity reappears, `build_and_upload_instances` finds no slot, `morph_gpu_fields_for_draw(None)` writes `morphDeltaAddress = 0`, and `triangle.vert` skips blending for the rest of the entity's life.
- **Evidence**: Paths that keep a MorphSlot entity out of the bump:
  - `static_meshes.rs` skips entities whose `AnimatedVisibility` is false (`if !visible { continue; }`), so the entity has no `DrawCommand` at all.
  - A full `SkinSlotPool` makes `allocate` return `None`, the draw goes out with `bone_offset = 0`, and the loop's `if dc.bone_offset == 0 { continue; }` skips it.
  - A failed `create_slot` (the `failed_skin_slots` path) means no `SkinSlot`, so the loop `continue`s before the morph bump. Raster would still deform here: the `GpuInstance` morph lookup is gated only on `bone_offset != 0`, not on a SkinSlot.
  - A skinned mesh uploaded `rt_capable = false` is skipped by `if !mesh.rt_capable { continue; }`. Its MorphSlot is evicted about 3 frames after spawn, even though raster reads morph for every `bone_offset != 0` draw.
  
  `grep "morph_slots.insert"` finds only `mesh_instance.rs`. `pending_morph_unload_victims` only removes.
- **Impact**: Visual only: blink/lip-sync/expression morphs (NiGeomMorpherController, #3231) stop for good on the affected NPC, with no log line (eviction is `log::debug!`). Frequency at runtime is unmeasured. The skin-pool-full and create-slot-failure triggers need memory pressure, and the visibility trigger needs a skinned morph mesh hidden for 3+ frames. Not reproduced here (no engine launch). `morph_memory_usage()`'s active-slot count dropping while the entities are still spawned would confirm it.
- **Related**: #3231 (morph path), #3374 (drain placement), #643 (the SkinSlot LRU this copied), #2925 (epoch rebase, covers morph correctly).
- **Suggested Fix**: Either exempt MorphSlots from idle eviction (release them only through `pending_morph_unload_victims` on despawn), or move the LRU bump to a place every live morph entity reaches, e.g. `update_morph_weights` / `flush_pending_morph_weights`, which already visit every slot each frame. Add a test pinning that a slot absent from dispatch for N frames survives.

### REN-2026-09-14-D11-01: ground-cover debug-point pipeline enables writes on albedo (attachment 5) but its fragment shader has no location-5 output
- **Severity**: MEDIUM
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/groundcover.rs` (`GroundCoverPipeline::build_pipelines`)
- **Status**: NEW
- **Description**: `build_pipelines` builds the blade and debug-point pipelines in one loop with one shared blend-attachment array (write masks on 0, 5, 6, 7). `groundcover_blade.frag` writes {0, 5, 6, 7}; `groundcover_debug.frag` writes only {0, 6, 7}.
- **Evidence**: shared blend array in the `build_pipelines` loop; no `layout(location = 5)` output in `groundcover_debug.frag`.
- **Impact**: in the debug-points view the albedo attachment receives undefined values; composite's `indirect * albedo` speckles. Same class as #3977. Debug view only.
- **Related**: #3977.
- **Suggested Fix**: give the debug pipeline its own blend array with attachment 5 masked off (or add the output), and pin both ground-cover fragment shaders' output locations the way the water test does.

### REN-2026-09-14-D23-01: ground-cover blades are rasterized without the projection jitter every other scene pipeline applies — the largest thin-geometry population in an exterior frame is fed to FSR (and TAA) unjittered
- **Severity**: MEDIUM
- **Dimension**: FSR/Presentation (TAA)
- **Location**: `crates/renderer/shaders/groundcover_blade.vert` (`main`, both `gl_Position = pc.viewProj * …` sites); `byroredux/src/app_frame.rs` (the `GroundCoverFrame { view_proj: frame.view_proj, … }` construction); `crates/renderer/src/vulkan/groundcover.rs` (`BladePush.view_proj`)
- **Status**: NEW
- **Description**:
  - FSR requires the whole colour input to carry the per-frame jitter handed to the SDK (`jitter_offset`), and TAA's supersampling relies on the same offset.
  - The main and water pipelines add it in the vertex shader: `currClip.xy += jitter.xy * currClip.w` in `triangle.vert`, `clip.xy += jitter.xy * clip.w` in `water.vert`.
  - The blade pipeline instead projects with its own push-constant `viewProj`, set from `frame.view_proj`, the un-jittered relative matrix built in `assemble_camera`. It never reads `GpuCamera.jitter`: `groundcover_blade.vert` has no `jitter` reference other than per-blade colour jitter.
  - Blades therefore land at the same sub-pixel position every frame while their terrain, the depth they are tested against, and the colour FSR dejitters all move by the jitter.
  - The same push matrix also skips the DOF-effective view-projection. That only matters in TAA mode, since DOF is gated off under FSR.
- **Evidence**:
  - `grep -n jitter crates/renderer/shaders/groundcover_blade.vert` matches only the colour-jitter comments.
  - `triangle.vert` and `water.vert` each have the `jitter.xy * clip.w` line.
  - `app_frame.rs` passes `view_proj: frame.view_proj` on both the enabled and `--groundcover-off` branches.
  - `docs/engine/exal-groundcover.md` §6 says of blade widening that "thin high-contrast geometry is the canonical case TAA handles worst", a design that assumes the reconstruction supersamples the blades. That needs the jitter.
- **Impact**: Visual. In the default FSR Quality path, FSR shifts unjittered blade colour by minus the jitter each frame. Together with D23-02's reactive = 1.0, which suppresses history, the likely result is sub-pixel temporal wobble and un-antialiased blade edges, plus depth-edge flicker where jittered terrain meets unjittered blade bases. In TAA mode the blades get no supersampling. Magnitude is not measured (no engine launch). Needs an A/B capture with a jittered blade VP.
- **Related**: #2772 (TAA/FSR jitter sign convention), #2518 (single jitter predicate), D11-01..03 (other ground-cover pipeline gaps), D23-02.
- **Suggested Fix**: Apply the frame's jitter in `groundcover_blade.vert` the way `water.vert` does: add `jitter.xy * clip.w`, either from the camera UBO or from a jitter lane in the push block. Alternatively hand `prepare_groundcover` the uploaded, jittered DOF-effective VP. Validate visually; this cannot be confirmed by `cargo test`.

### REN-2026-09-14-D23-02: blades write reactive = 1.0 and transparency&composition = 1.0 over every covered pixel, contradicting the FSR integration plan's mask contract (0.9 clamp, opaque geometry masks off, "start material-driven rather than marking the entire frame") with no doc update
- **Severity**: MEDIUM
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/shaders/groundcover_blade.frag` (`main`, `outFsrReactive = 1.0; outFsrTransparency = 1.0;`); `crates/renderer/src/vulkan/groundcover.rs` (blade pipeline `blend_attachments[6]`/`[7]` `color_write_mask(R)`); `docs/engine/fsr3-upscaler-integration-plan.md` §1.4; `docs/engine/fsr3-troubleshooting.md`
- **Status**: NEW
- **Description**:
  - The blade pipeline is opaque and depth-writing, yet it enables R writes on attachments 6/7, and the fragment shader writes both FSR masks at full strength for every blade fragment.
  - The shader header calls this "the documented remedy". The project's authoritative FSR docs say otherwise:
    - The plan's §1.4 table says reactive is written as `min(alpha, 0.9)` by glass, particles, water and alpha-blended decals.
    - The same table says the T&C mask should "Start material-driven rather than marking the entire frame".
    - `fsr3-troubleshooting.md` says "opaque geometry masks its writes off entirely".
  - Neither doc mentions ground cover, and `docs/engine/exal-groundcover.md` has no reactive/transparency/FSR text.
  - An exterior meadow therefore marks most of the lower frame as fully reactive plus T&C. FSR then discards history over that area and reconstructs it from single render-resolution samples, which is the opposite of the §6 goal of letting reconstruction antialias thin blades.
- **Evidence**: The quoted doc lines above; `groundcover_blade.frag` writes `outFsrReactive = 1.0; outFsrTransparency = 1.0;` unconditionally; `groundcover.rs` sets `color_write_mask(vk::ColorComponentFlags::R)` for attachments 6 and 7. The rationale the shader gives (no motion vector for wind-animated geometry) is real, but the plan's own contract answers that case with the 0.9 clamp and material-driven marking, not 1.0 everywhere.
- **Impact**: Visual, and possibly deliberate. The masks are only consumed on the FSR path, which is the engine default. The expected cost is aliasing/shimmer on grass at Quality/Balanced/Performance, compounded by D23-01. It needs an A/B capture: masks at 1.0, at 0.9, and reactive-only. If 1.0 is the measured best choice, the finding reduces to doc drift in both FSR docs.
- **Related**: D23-01; #2749 (triangle.frag early-return mask writes, closed); #4203 (mask attachments unconditional); D11-01 (ground-cover debug pipeline write mask).
- **Suggested Fix**: Decide the policy from a capture: clamp to 0.9 per the plan, and/or drop T&C and keep a reduced reactive value keyed to wind sway. Then record the ground-cover row in `fsr3-upscaler-integration-plan.md` §1.4 and `fsr3-troubleshooting.md`, so the "opaque geometry masks off" statement is no longer contradicted by a live opaque writer.


---

# LOW

### REN-2026-09-14-D2-02: `shader-pipeline.md` descriptor table and submission order omit the new Set-1 binding 20 (`skyCube`) and the per-frame sky-cube bake
- **Severity**: LOW
- **Dimension**: Ray Queries (doc-rot)
- **Location**: `docs/engine/shader-pipeline.md` (Descriptor Sets table, Set 1 rows; Per-Frame Submission Order block); code source of truth `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (`build_scene_descriptor_bindings`) and `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (`record_bake` call)
- **Status**: NEW
- **Description**: `6db9eac2` added scene Set 1 binding 20 (`COMBINED_IMAGE_SAMPLER`, `samplerCube skyCube`, FRAGMENT, `PARTIALLY_BOUND`, read by `raytrace.glsl` and `lighting.glsl`), and a per-frame `SkyCubePipeline::record_bake` compute dispatch recorded before the main render pass. `shader-pipeline.md` was not touched after `1efc5251`: its Set-1 table stops at binding 19 and its submission-order block has no sky-cube step. The skill tells auditors to prefer this doc over source for descriptor facts, and `skyal.md` is currently the only place binding 20 is documented.
- **Evidence**:
  - `grep -n "skyCube\|sky_cube" docs/engine/shader-pipeline.md` returns no hits. The only `| 20 |` row in the doc is `volumetrics_inject.comp`'s private `BoundaryVertexBuffer`.
  - `git log -1 -- docs/engine/shader-pipeline.md` is `1efc5251`, which predates `b54b86b7` / `6db9eac2`.
  - `grep -n "binding(20)" crates/renderer/src/vulkan/scene_buffer/buffers.rs` → the SKYAL binding.
- **Impact**: Documentation only. Anyone using the table to answer "which bindings does Set 1 carry / what must be written when the scene set is rebuilt" will miss a binding whose absence from a write is undefined data by design (`PARTIALLY_BOUND`), not a validation error. This is the same rot class as #3577 / #2918 / #4019.
- **Related**: #4019 (`the_descriptor_table_does_not_credit_private_layout_passes_with_global_sets` checks only "Used by" cells, not completeness); REN-2026-09-14-D3-01 (same commit, GpuCamera row); the submission-order half overlaps Dim 4 scope.
- **Suggested Fix**:
  - Add a `| 1 | 20 | COMBINED_IMAGE_SAMPLER | SKYAL baked sky cubemap (per frame in flight, gated by exteriorSkyTint.w) | triangle (raytrace.glsl / lighting.glsl) |` row.
  - Add a sky-cube bake step before the main render pass in the submission order.
  - Consider a doc-completeness pin that walks `build_scene_descriptor_bindings`' binding numbers against the Set-1 rows.

### REN-2026-09-14-D3-01: `GpuCamera.exterior_sky_tint.w` became the live sky-cubemap ready flag, but `shader-pipeline.md` and the field's own rustdoc still document it as "reserved"
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (doc-rot)
- **Location**: `docs/engine/shader-pipeline.md` (GpuCamera table, `| 352 | 16 | exterior_sky_tint |` row); `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuCamera::exterior_sky_tint` doc comment); writer `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs`; readers `crates/renderer/shaders/include/raytrace.glsl` / `crates/renderer/shaders/include/lighting.glsl`
- **Status**: NEW
- **Description**: `6db9eac2` repurposed `exterior_sky_tint.w` from "reserved (0)" to the SKYAL ready flag. It is written as `if self.sky_cube.is_some() { 1.0 } else { 0.0 }` and gates every `skyCube` read, because binding 20 is `PARTIALLY_BOUND` and never written when the bake fails to initialise. The GLSL mirror comments were updated. The authoritative layout doc and the Rust field doc were not:
  - `shader-pipeline.md`'s row still reads "xyz = the live exterior's sky zenith colour (#3323); w reserved".
  - `gpu_types.rs` still says "xyz = the **exterior** TOD/weather zenith colour in linear RGB; w reserved (0)".
  - The `CameraUBO` comment in `bindings.glsl` still says the lane is "Read ONLY there" (the window-portal escape), which is now true only of `.xyz`.

  This is the exact class #3989 fixed for `render_debug.w` and `material_flags` bit 10: a live lane advertised as free. `shader_pipeline_doc_does_not_advertise_live_lanes_as_free` exists for this class but pins only those two rows.
- **Evidence**:
  ```text
  docs/engine/shader-pipeline.md:  | 352 | 16 | `exterior_sky_tint` | xyz = ... (#3323); w reserved. ...
  gpu_types.rs:                    /// xyz = the **exterior** TOD/weather zenith colour in linear RGB; w
                                   /// reserved (0).
  assemble_camera_and_lights.rs:   if self.sky_cube.is_some() { 1.0 } else { 0.0 },
  raytrace.glsl:                   missCol = exteriorSkyTint.w > 0.5 ? texture(skyCube, direction).rgb : ...
  ```
- **Impact**: Documentation only today. The hazard is the one #3989 names: a future author allocates the "reserved" `w` for new per-frame state, and every exterior RT miss then samples `skyCube` depending on that unrelated value. When the bake is absent that's an unwritten `PARTIALLY_BOUND` descriptor, which is undefined data rather than a validation error or crash, so `cargo test` would not catch it.
- **Related**: #3989 (same class, same doc, same guard test); #3323 (introduced the lane); REN-2026-09-14-D2-02 (same commit's binding-20 doc omission).
- **Suggested Fix**:
  - Update the `shader-pipeline.md` row and the `gpu_types.rs` rustdoc to "w = sky-cubemap ready flag (1.0 when `SkyCubePipeline` exists; gates every `skyCube` read) — Not a free slot".
  - Correct the `bindings.glsl` "Read ONLY there" sentence to scope it to `.xyz`.
  - Extend `shader_pipeline_doc_does_not_advertise_live_lanes_as_free` with an `exterior_sky_tint` row assertion.

### REN-2026-09-14-D5-01: `memory-budget.md` ledgers none of the three new GPU resource owners, and `sky_cube_bytes_per_frame` — documented as existing "so the memory budget can account for it" — has zero callers
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**:
  - `crates/renderer/src/vulkan/sky_cube.rs` (`sky_cube_bytes_per_frame`)
  - `crates/renderer/src/vulkan/groundcover.rs` (`GroundCoverPipeline::create_buffers`)
  - `crates/renderer/src/vulkan/cloud_noise.rs` (`CloudNoiseVolumes`)
  - `docs/engine/memory-budget.md` (VRAM Rough Budget table; "Not yet ledgered")
- **Status**: NEW
- **Description**:
  - `memory-budget.md` does not mention the SKYAL sky cube, the cloud noise volumes, or EXAL ground cover.
  - Its "Not yet ledgered" section lists only `StagingPool` retained capacity, and says: "A grep of this page for the owning subsystem name is the cheapest way to find a gap in it."
  - `sky_cube_bytes_per_frame`'s doc says it is "Exposed so the memory budget can account for it the way `SSAO_BYTES_PER_PIXEL` does". Its only reference is its own unit test.
  - The ground-cover blade buffer is the largest of these, a fixed 16 MiB device-local allocation that exists on every RT device whether or not the scene has ground cover.
- **Evidence** (sizes derived from code):
  - **Ground-cover blade buffer**: `GROUNDCOVER_MAX_CHUNKS` (256) × `GROUNDCOVER_MAX_BLADES_PER_CHUNK` (4096) × 16 B = 16,777,216 B.
  - **Other ground-cover buffers**:
    - interaction field: `INTERACTION_TEXEL_COUNT` (256²) × 2 × 4 B = 524,288 B
    - indirect buffer: 256 × 16 B = 4,096 B
    - per-FIF host-visible chunk / cell / species / table / state / disturber / readback buffers (small)
  - **Sky cube**: 6 × 128² × 8 B = 786,432 B per FIF × `MAX_FRAMES_IN_FLIGHT` = 1,572,864 B.
  - **Cloud noise**: 64³ + 32³ R8 = 288 KiB, per `cloud_noise.rs`'s module doc. The same doc notes `VolumetricsPipeline` "still uploads its own copy of the same texels".
  - `grep -rn sky_cube_bytes_per_frame` returns only the definition and `the_vram_figure_follows_the_face_size`.
  - `screen_scaled_reservation_bytes` (`crates/renderer/src/vulkan/acceleration/predicates.rs`) has no fixed-size term. That is consistent with its name, so the BLAS reservation is not wrong. The gap is the ledger.
- **Impact**: About 18.5 MiB of resident VRAM is invisible on the page cited as authoritative, and the page's own grep-for-subsystem recipe returns nothing for any of the three owners. No leak and no correctness risk.
- **Related**: #3566 (the mesh-side `StagingPool` ledger gap, fixed; same class); REN-2026-09-11-D5-01 / #4117 (dead telemetry accessor, same shape).
- **Suggested Fix**: Add ground cover, sky cube and cloud noise rows to the VRAM Rough Budget table, derived from the constants above. Then either cite `sky_cube_bytes_per_frame` from the doc or a test, or drop its "so the memory budget can account for it" claim.

### REN-2026-09-14-D6-02: #3901 lets a flipbook replace the `Normal` / `Height` texture index, but the draw's `normal_has_alpha` gate still describes the spawn-time normal map
- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/render/static_meshes.rs` (`apply_texture_flip_roles`, `collect_static_mesh_draws`)
- **Status**: NEW
- **Description**: `apply_texture_flip_roles` writes active `FlipTextureRole::Normal` / `Height` frames into `texture_indices`. `normal_has_alpha` is still read from the spawn-time `MaterialTextureHandles`. Two gates use it against the flipped index:
  - the #3562 `PARALLAX_ALPHA_HEIGHT_BIT` gate;
  - `normal_alpha_spec_binding_applies`, the gloss-slot rebind.
  A Normal flip whose frames differ in alpha presence would reintroduce the BC1/BC5 alpha misread #3562 fixed.
- **Evidence**: `apply_texture_flip_roles(flip, &mut texture_indices); let normal_map_index = texture_indices.normal; let normal_has_alpha = material_texture_handles.map(|handles| handles.normal_has_alpha)…`
- **Impact**: None on shipping content. Per the #3901 commit, the only vanilla non-base flipbook is a `GLOW_MAP` flip on Oblivion's `battle.nif`. Latent.
- **Related**: #3901, #3562, #4260, #1480.
- **Suggested Fix**: Carry per-frame alpha presence on `TextureFlipEntry`, resolved at clip-attach, for the Normal role; or restrict the Normal/Height arms until real content exists, and pin the choice with a test.

### REN-2026-09-14-D6-03: `every_exterior_spawner_inserts_a_boundary_material` skips `cell_loader/spawn/` and `cell_loader/references/`, so the guard the skill tells auditors to rely on misses the main spawner directory
- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs` (`every_exterior_spawner_inserts_a_boundary_material`)
- **Status**: NEW
- **Description**: The scan is a non-recursive `read_dir(cell_loader/)` that `continue`s on subdirectories. `byroredux/src/cell_loader/spawn/mesh_instance.rs`, `references/`, `byroredux/src/scene/nif_loader.rs` and `npc_spawn/` are never scanned. They route through the boundary today by inspection only. The Dimension 6 checklist says to trust this test instead of hand-verifying callers.
- **Evidence**: `continue; // subdirectories (spawn/, references/) and non-.rs files`; the sanity floor lists only six top-level files.
- **Impact**: Test coverage gap; no live divergence.
- **Related**: #2444, #3733, #4041.
- **Suggested Fix**: Recurse, and also scan `scene/` and `npc_spawn/`, skipping `*_tests.rs`. Add `byroredux/src/cell_loader/spawn/mesh_instance.rs` to the sanity list.

### REN-2026-09-14-D6-04: The #4287 fix pasted its six-line rationale comment twice in `fill_from_bgsm`'s BGEM arm
- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/cell_loader/refr.rs` (`RefrTextureOverlay::fill_from_bgsm`)
- **Status**: NEW
- **Description**: The `// #4287 / SF-2026-09-11-D9-02 — \`base_texture\` is BGEM's …` block appears twice back-to-back; the `a5a6407d` diff added it twice.
- **Evidence**: `rg -n "#4287 / SF-2026-09-11-D9-02" byroredux/src/cell_loader/refr.rs` → two hits, six lines apart.
- **Impact**: Hygiene only.
- **Related**: #4287.
- **Suggested Fix**: Delete one copy.

### REN-2026-09-14-D6-05: Ground cover is a drawn surface with no canonical `Material`, and neither the NIFAL spec nor the ground-cover spec records it as an exemption
- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `docs/engine/nifal.md` (§3), `docs/engine/exal-groundcover.md`, `byroredux/src/material_translate.rs` (`translate_texture_only_material` doc)
- **Status**: NEW
- **Description**: The #2444 doc states the invariant "every drawn surface's canonical material is produced at one boundary". EXAL ground cover draws blades with no `Material`, no `GpuMaterial` and no `MaterialTable`, shaded from `GroundCoverPalette` (`groundcover_translate.rs`). That is the correct shape: no `Imported*` tier, no renderer per-game branch. But unlike the Cornell and `crates/save` exemptions, it is not written down.
- **Evidence**: zero `Material` / `GpuMaterial` / `intern(` hits in the three ground-cover files; `exal-groundcover.md` mentions only `KHR_materials_sheen`; `nifal.md` has no ground-cover mention.
- **Impact**: Documentation only; the next audit must re-derive it.
- **Related**: #2444, #4054–#4058.
- **Suggested Fix**: Add a one-paragraph exemption to `nifal.md` §3, the `translate_texture_only_material` doc, and the Dimension 6 exemption list.

### REN-2026-09-14-D8-01: #4046 renamed only `next_svgf_temporal_alpha`'s parameter — the `params.w` contract is still documented as the bare camera-static flag in four places, one of which now states the opposite of the code
- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**:
  - `crates/renderer/src/vulkan/context/post_passes.rs` (`record_post_passes`, doc on `caustic_history_valid`)
  - `crates/renderer/src/vulkan/svgf.rs` (`SvgfTemporalParams::params` doc, `SvgfPipeline::upload_params`)
  - `crates/renderer/shaders/svgf_temporal.comp` (`SvgfTemporalParams` UBO comment, `hasHistory` branch comment)
- **Status**: NEW (residual of closed #4046; the behavioural fix holds)
- **Description**:
  - Since `5ee1c150`, `params.w` is `caustic_history_valid && !recovering`, i.e. camera parked AND light rig / caustic sources / rigid instances / skinned poses unchanged, with no recovery window open.
  - #4046 renamed the parameter of `next_svgf_temporal_alpha` to `scene_static`. It left every downstream description on the old camera-only meaning, in four places:
    1. `record_post_passes`'s parameter doc says: "The caustic accumulator's EMA is the only consumer down here; SVGF and TAA reject stale history per pixel and keep the camera-only flag, which they read at their own upload sites in `draw.rs`." That is now false on two counts:
       - SVGF consumes `caustic_history_valid`, not the camera-only flag.
       - Its upload site is `build_and_upload_instances.rs`, not `draw.rs`.
    2. `SvgfPipeline::upload_params` still names its parameter `camera_static`. Its inline comment says the flag is set "When the camera is static (view-proj unchanged frame-to-frame) … reverts to the floored EMA the moment the camera moves".
    3. `SvgfTemporalParams::params` doc: "w = camera_static flag".
    4. `svgf_temporal.comp`:
       - The UBO comment reads "w = camera_static".
       - The `hasHistory` comment still records the pre-#4046 trade-off: "dynamic *lighting* on a static surface converges slowly (~N frames) while parked — acceptable". #4046 removed exactly that behaviour.
       - It also says "the raw value still feeds the caustic-history gate". The value fed here *is* the caustic-history gate now.
- **Evidence**: `grep -n camera_static crates/renderer/src/vulkan/svgf.rs crates/renderer/shaders/svgf_temporal.comp` → the SVGF upload fn param and `params` docs. `build_and_upload_instances.rs` calls `next_svgf_temporal_alpha(self.svgf_recovery_frames, caustic_history_valid)`.
- **Impact**: No runtime effect. Anyone tuning the parked-camera GI convergence, or re-deriving `params.w`, is told it is camera-only, and would "re-fix" a light-rig lag that no longer exists or misattribute the extra history drops. #3995's own comment in the shader warns against re-deriving this flag.
- **Related**: #4046 (closed), #3995 (closed).
- **Suggested Fix**:
  - Rename `upload_params`'s parameter to `scene_static` (or `progressive_accumulation`).
  - Reword the four comments to "camera parked AND light rig/scene unchanged, with no recovery window open".
  - Correct `record_post_passes`'s doc to say SVGF consumes `caustic_history_valid` from `build_and_upload_instances.rs`.

### REN-2026-09-14-D8-02: `composite.frag`'s binding comments still describe a 32-step froxel ray-march, a pre-ACES bloom add and a tone-map that this shader no longer performs
- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/shaders/composite.frag` (comments on `volumetricFroxel` binding 6, `bloomTex` binding 7, and the geometry arm of `main`)
- **Status**: NEW
- **Description**: Three comment claims contradict the live shader and, in one case, the same file:
  1. The binding-6 comment says the froxel volume is "Sampled per-fragment with a 32-step ray-march for the in-scatter + transmittance modulation applied to `combined` before ACES."
     - The live consumer is `sampleVolumetricColumn`: a depth-weighted 2×2 bilateral column tap with one linear Z blend. No march.
     - No ACES runs in this shader; it lives in `presentation.frag`.
  2. The binding-7 comment says bloom mip 0 is "added to `combined` before ACES per Frostbite §8." The same file's M58 block, near the end of `main`, says `bloomTex` "is therefore unused by this shader now" (#2796). `bloom_apply.comp` performs the add.
  3. The geometry-arm comment reads "combine direct + (indirect × albedo) and tone map". No tone map follows.
- **Evidence**: `grep -n "ACES\|32-step\|tone map" crates/renderer/shaders/composite.frag`, compared against the body of `sampleVolumetricColumn` and the "`bloomTex` (binding 7) is therefore unused" comment in `main`.
- **Impact**: Documentation only. The header is the first thing a reader of the composite contract sees, and it re-teaches the composite-does-ACES misconception that open #4202 is tracking in `CLAUDE.md`, plus the pre-#2796 bloom placement.
- **Related**: #4202 (open, same misattribution in `CLAUDE.md`, different file), #3608 (closed, `renderer.md` bloom attribution), #2796.
- **Suggested Fix**: Reword the binding 6/7 comments: a depth-weighted column tap, output linear HDR with tone mapping in `presentation.frag`, and `bloomTex` declared-but-unused because `bloom_apply.comp` adds bloom. Drop "and tone map" from the geometry-arm comment.

### REN-2026-09-14-D11-02: new shaders sit outside the reflection guards
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/groundcover.rs`, `crates/renderer/src/vulkan/reflect.rs`
- **Status**: Existing: #4110 (same missing reflection-guard class, extended to ground cover; add to that issue rather than filing separately)
- **Description**: `groundcover.rs` builds three descriptor-set layouts with no `validate_set_layout` call. The blade shaders use scene set 1, which is reflection-checked only against the triangle and water shaders. Neither ground-cover fragment shader has an output-location pin. `every_committed_spv_is_spirv_1_0` omits `sky_cube.comp`, `groundcover_interaction.comp`, `bloom_apply.comp` and `presentation.frag`.
- **Impact**: a descriptor or output drift in these shaders is not caught by `cargo test`.
- **Related**: #4110; prior report `AUDIT_RENDERER_2026-09-11.md` D11-01 wrongly stated `groundcover.rs` calls `validate_set_layout`.
- **Suggested Fix**: add `validate_set_layout` for the three ground-cover layouts and set 1 against the blade shaders; add the missing `.spv` files to the SPIR-V version test.

### REN-2026-09-14-D11-03: surface-format change rebuilds triangle/water pipelines but not ground-cover pipelines
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` (`recreate_swapchain_core`)
- **Status**: NEW
- **Description**: on a surface-format change `recreate_swapchain_core` rebuilds the render pass and the triangle and water pipelines, but not the ground-cover pipelines.
- **Impact**: not a spec violation today because the new render pass is compatible; becomes VUID-vkCmdDraw-renderPass-02684 if a main-pass attachment ever depends on the swapchain format.
- **Suggested Fix**: rebuild ground-cover pipelines alongside the other main-pass pipelines, or assert render-pass compatibility at that site.

### REN-2026-09-14-D12-01: the "authoritative" per-frame submission order in `shader-pipeline.md` omits every pass added since the last sweep — the sky-cube bake, the ground-cover interaction/scatter compute with its fill/copy transfers, and the ground-cover draw inside the main pass
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (Command Buffer Recording)
- **Location**: `docs/engine/shader-pipeline.md` (Per-Frame Submission Order, Compute shader table); recording sites: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (`record_bake` call), `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (`record_scatter` call), `crates/renderer/src/vulkan/context/geometry_pass.rs` (`record_draw` call)
- **Status**: NEW
- **Description**:
  - `shader-pipeline.md` describes its step list as the strict submission order and numbers each barrier. Three things now recorded every frame are missing:
    - **Ground cover (before 5b)**: `record_interaction`'s dispatch and its two memory barriers, three `vkCmdFillBuffer` calls, a `TRANSFER → COMPUTE` barrier, the scatter dispatch, the #4181 publish barrier, and a `vkCmdCopyBuffer` readback. All sit between step 5 (cluster cull) and 5b.
    - **Sky-cube bake (before 5b)**: `record_bake`'s two image barriers and its compute dispatch, recorded in `build_and_upload_instances` ahead of the 5b HOST barrier.
    - **Ground-cover draw (inside step 6)**: blade/debug draw after water.
  - The compute-shader table likewise omits `sky_cube.comp`, `groundcover_scatter.comp` and `groundcover_interaction.comp`. No section documents the raster shaders `groundcover_blade.vert` / `groundcover_blade.frag` or `groundcover_debug.frag`.
  - `grep -in 'sky.cube|groundcover|ground.cover' docs/engine/shader-pipeline.md` returns only the `exterior_sky_tint` GpuCamera row, which D11-04 covers.
- **Evidence**:
  - The call sites listed under Location, confirmed by the phase-order trace in Checked above.
  - Commit `6db9eac2` ("baked every frame before the geometry pass") and `#4054`/`#4055` (ground cover) touched neither the doc's step list nor its shader tables.
- **Impact**: An auditor or contributor reasoning about barrier coverage from the doc's step list, which the skill tells auditors to cross-check against, cannot see two compute passes, a readback and a draw, including the sites of D4-01's hazard and #4181's fix. No runtime effect.
- **Related**: REN-2026-09-14-D11-04 (same doc: binding 20 and the `exterior_sky_tint.w` lane); REN-2026-09-14-D4-01; #4033 (closed — prior omission of barrier steps from this same list).
- **Suggested Fix**: Add numbered steps for the ground-cover interaction/scatter (with its fills, barriers and readback) and for the sky-cube bake between steps 5 and 5b, and a ground-cover draw line under step 6. Add the three compute shaders and the ground-cover raster shaders to the shader tables.

### REN-2026-09-14-D13-01: `taa.comp` and `coverage_alpha_factors` both still say HDR alpha has "no other consumer" / is "harmless today", but under `--upscaler taa` composite's sky arm reads TAA's forwarded alpha as transparent coverage
- **Severity**: LOW
- **Dimension**: TAA
- **Location**:
  - `crates/renderer/shaders/taa.comp` (`main`, comment above `vec4 curr = texelFetch(uCurrHdr, ...)`)
  - `crates/renderer/src/vulkan/pipeline.rs` (`coverage_alpha_factors` doc)
- **Status**: NEW
- **Description**:
  - `taa.comp` justifies forwarding `currA` at all three `imageStore` sites like this: "harmless today (composite forwards .a to the swapchain which ignores it) but a future composite branch that gates on alpha … would silently see a zeroed bit".
  - `coverage_alpha_factors`'s rustdoc makes the same claim: "The lane has no other consumer: `taa.comp` forwards HDR alpha untouched and `composite.frag` forwards it to the swapchain, which ignores it".
  - Both are false on both halves:
    1. Composite writes the offscreen `HDR_FORMAT` scene image, not the swapchain; presentation owns the swapchain (#3426).
    2. The "future composite branch that gates on alpha" already exists. `composite.frag`'s `is_sky` arm computes `coverage = clamp(direct4.a, 0, 1)` and weights `sky_radiance(...) * (1.0 - coverage)` (#2466).
  - With `--upscaler taa`, composite binding 0 (`hdrTex`) is TAA's output (`rebind_hdr_views(&taa_views, GENERAL)`). So TAA's alpha pass-through is exactly what that sky arm reads.
- **Evidence**:
  - `composite.frag`: `float coverage = clamp(direct4.a, 0.0, 1.0); … combined = sky_radiance(...) * (1.0 - coverage) + direct + skyIndirect * skyAlbedo;`
  - `crates/renderer/src/vulkan/context/init.rs` and `set_upscaler_mode`: `c.rebind_hdr_views(&device, &taa_views, vk::ImageLayout::GENERAL)`.
  - `grep -n currA crates/renderer/src/vulkan/taa.rs` finds nothing: no test pins the pass-through.
- **Impact**:
  - Documentation drift with a live trap. A "cleanup" trusting either comment could write `vec4(rgb, 1.0)` in `taa.comp`, since it is documented as harmless. That would make every clear-depth pixel under `--upscaler taa` report full coverage and replace the sky with black, and nothing would fail.
  - This includes the automatic FSR→TAA fallback (#2480).
- **Related**: #2466, #676 / DEN-6 (closed), #2799 (closed, similar stale "swapchain" attribution), #3572 (open).
- **Suggested Fix**:
  - Reword both comments: composite's sky arm consumes this lane as transparent coverage, so TAA's pass-through is load-bearing.
  - Add a source-scan pin in `taa.rs` that every `imageStore(uOutput, …)` forwards `currA`.

### REN-2026-09-14-D14-01: `CausticPipeline::dispatch` still documents a fixed 0.15 parked new-sample weight, and `advance_parked_visits` points at `context::draw` for a flag that lives in `build_and_upload_instances.rs`
- **Severity**: LOW
- **Dimension**: Caustics
- **Location**: `crates/renderer/src/vulkan/caustic.rs` (`CausticPipeline::dispatch` splat-dispatch comment; `advance_parked_visits` doc)
- **Status**: NEW
- **Description**:
  1. The comment above the splat `cmd_push_constants` in `dispatch` reads: "decay_factor drives the EMA new-sample weight (1 - decay_factor) in the shader: 0.15 of this frame while parked, full energy while moving". The live decay is not a fixed 0.85:
     - `advance_parked_visits` returns `(n / (n + 1.0)).min(CAUSTIC_DECAY_MAX)`, with `CAUSTIC_DECAY_MAX = 0.995`.
     - So the new-sample weight is `1/(n+1)`: 0.5 on the first parked visit, falling to a 0.005 floor.
     - A few lines earlier the same function explains why a constant decay was replaced ("a constant decay (e.g. 0.96) plateaus…").
  2. `advance_parked_visits`'s doc says "see `context::draw`'s `caustic_history_valid`". `caustic_history_valid` is computed in `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`; `grep -n caustic_history_valid crates/renderer/src/vulkan/context/draw.rs` has no definition.
- **Evidence**:
  - `grep -n "0\.15" crates/renderer/src/vulkan/caustic.rs` → only the splat comment.
  - `const CAUSTIC_DECAY_MAX: f32 = 0.995;` and `advance_parked_visits`'s body.
- **Impact**: Documentation only. Anyone tuning the parked caustic convergence (or the #2468 scene-dirty gate) from the comment would expect a ~6-frame EMA. The actual path is a ~200-visit running average, a very different stale-pool window.
- **Related**: #2468, #4009 (closed, earlier caustic/water `file:NN` doc-rot sweep), REN-2026-09-14-D8-01 (the same `caustic_history_valid` now also feeds SVGF).
- **Suggested Fix**: Replace "0.15 of this frame while parked" with "`1/(N+1)` of this frame while parked (floored at `1 - CAUSTIC_DECAY_MAX`)", and point the `advance_parked_visits` cross-reference at `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`.

### REN-2026-09-14-D16-01: `bloom_downsample.comp`'s "4-tap bilinear = 4×4 box" is geometrically a 2×2 box: taps offset ±0.5 *source* texels from a destination centre land exactly on source texel centres
- **Severity**: LOW
- **Dimension**: Bloom
- **Location**: `crates/renderer/shaders/bloom_downsample.comp` (`main`, header comment), `crates/renderer/src/vulkan/bloom.rs` (`BloomPipeline::upload_params`)
- **Status**: NEW
- **Description**:
  - The shader header states: "four taps placed at (±0.5, ±0.5) destination-pixel offsets cover an effective 4×4 source region with even weighting. Provably equivalent to a 4×4 box filter". That is the stated justification for choosing a box over Jimenez's 13-tap.
  - The code offsets by `src_pixel`, not destination pixels: `texture(src, uv + vec2(±0.5, ±0.5) * src_pixel)`, where `inv_resolutions.xy = 1 / src_extent` (`upload_params`).
  - Mip 0 has `src_extent = self.extent` (the render extent passed to `BloomPipeline::new`) and `dst = extent / 2`. Deeper levels halve again.
  - For an even source dimension, the destination texel `i` centre sits at source coordinate `(i + 0.5)·2 = 2i + 1`: the shared corner of source texels `2i` and `2i+1`.
  - A ±0.5 source-texel offset lands exactly on those two texel centres (`2i+0.5`, `2i+1.5`), so each bilinear tap degenerates to a single-texel point sample.
  - The four taps therefore average exactly the 2×2 footprint, with no overlap between neighbouring destination texels: the plain box-mip downsample, not a 4×4 filter. An odd source dimension (e.g. 1080-high chains at 135→67) only drifts slightly off that.
  - The 4×4 footprint the comment describes needs ±1 source-texel offsets. Those put each tap on a texel corner, so bilinear averages 2×2 per tap across a 4×4 region.
- **Evidence**:
  - `bloom_downsample.comp`: `vec2 src_pixel = params.inv_resolutions.xy;` … `texture(src, uv + vec2(-0.5, -0.5) * src_pixel)`.
  - `upload_params`: `1.0 / src_extent.width as f32` in lanes xy; mip-0 extent `(screen_extent.width / 2).max(1)` in the frame-state constructor.
- **Impact**:
  - The documented filter property does not hold, and it is the stated basis for "lands 80% of the visual win".
  - A non-overlapping 2×2 box chain is the downsample known to make small bright features pulse as they cross texel boundaries under sub-pixel motion. That now matters more, because since `62a09fd9` bloom is dominated by exactly those small above-threshold features (sun disc, emissive points).
  - Visual severity needs an in-engine A/B; it is not asserted here. No correctness or VRAM effect.
- **Related**: #2805 (closed, `BLOOM_INTENSITY` derivation), #1275 (upsample DC-gain note), `62a09fd9`.
- **Suggested Fix**: Either offset the taps by ±1.0 `src_pixel` to get the 4×4 footprint the comment promises (keeping the per-tap bright-pass and 0.25 weights), or correct the comment to say "2×2 box". Re-tune the knee numbers only if the footprint changes.

### REN-2026-09-14-D16-02: `62a09fd9` added the bright-pass directly below `BLOOM_INTENSITY`'s doc block but left it asserting there is no bright-pass, and three bloom comments still name `composite.frag` as the bloom consumer
- **Severity**: LOW
- **Dimension**: Bloom
- **Location**:
  - `crates/renderer/src/shader_constants_data.rs` (`BLOOM_INTENSITY` doc)
  - `crates/renderer/shaders/bloom_upsample.comp` (header)
  - `crates/renderer/shaders/composite.frag` (the "VOLUME_FAR and BLOOM_INTENSITY now come from the `#include`d header" comment)
- **Status**: NEW
- **Description**:
  1. `BLOOM_INTENSITY`'s doc still reads: "`bloom_downsample.comp`'s `DownsampleParams` carries no bright-pass threshold or Karis average, so this is a broadband lift on the local average, not a highlight-only glow."
     - Since `62a09fd9`, `DownsampleParams::bright_pass` is exactly such a threshold, and the next constant (`BLOOM_THRESHOLD`) documents it.
     - The same doc's "effective contribution … = 0.75× the local blurred average" is the pre-threshold figure; the commit itself measures ~1.01× on the sky after the knee.
  2. That doc says `BLOOM_INTENSITY` is "Consumed by `composite.frag` via the `#include`d `#define`". The only shader reference is `bloom_apply.comp` (`scene.rgb + bloom * BLOOM_INTENSITY`); `composite.frag` mentions it only in a comment.
  3. `bloom_upsample.comp` still says "Final mip 0 is what composite samples" and quotes the 5× × 0.15 "effective contribution to composite.frag's `combined`".
  4. The two adjacent docs now contradict each other on emissives:
     - `BLOOM_INTENSITY` says "See `feedback_color_space.md` for why we don't HDR-boost emissives globally".
     - `BLOOM_THRESHOLD` says "the real fix is the global HDR emissive boost that `feedback_color_space.md` and `BLOOM_INTENSITY`'s note above both already name".
- **Evidence**:
  - `grep -n "BLOOM_INTENSITY" crates/renderer/shaders/*.comp crates/renderer/shaders/*.frag` → `bloom_apply.comp` (code) and `composite.frag` (comment only).
  - The `BLOOM_INTENSITY` vs `BLOOM_THRESHOLD` doc blocks in `shader_constants_data.rs`.
- **Impact**: Documentation only, on the constant a tuner reaches for first. It tells them bloom is an un-thresholded broadband lift (the exact behaviour `62a09fd9` removed, 1.71× sky gain), names the wrong consumer shader, and gives two opposite readings of the emissive-boost policy.
- **Related**: #2805 (closed, contradictory `BLOOM_INTENSITY` derivations), #3608 (closed, `renderer.md` bloom attribution), REN-2026-09-14-D8-02 (`composite.frag` bloom binding comment).
- **Suggested Fix**:
  - Rewrite the `BLOOM_INTENSITY` doc: intensity is applied to the soft-knee-thresholded pyramid, consumed by `bloom_apply.comp`, with the 0.75× figure marked as the un-thresholded DC bound.
  - Update `bloom_upsample.comp`'s "composite samples" wording.
  - Settle the emissive-boost sentence in one place.

### REN-2026-09-14-D18-02: `cloud_march` bounds the ray against a spherical shell but derives `height_fraction` from flat `position.y`, compressing the cloud's vertical profile toward the horizon
- **Severity**: LOW
- **Dimension**: Sky/Weather
- **Location**: `crates/renderer/shaders/include/clouds.glsl` (`cloud_march`, `cloud_shell_distance`)
- **Status**: NEW
- **Description**:
  - `start` and `end` come from `cloud_shell_distance`, which works in radial altitude (r = R + h on a planet of radius `CLOUD_PLANET_RADIUS`). But each sample's height is `position = dir * t; height_fraction = (position.y - BOTTOM)/(TOP - BOTTOM)`, the flat-plane height of the viewer frame.
  - For a ray with elevation `dir.y`, the flat height at the shell crossing is below the true altitude, and the gap grows as `dir.y` falls.
  - `cloud_height_gradient` is 0 at `height_fraction` 0, so the start of low rays has zero density, and the upper taper is never reached.
  - The light march mixes the two conventions: `max(position.y, 0)` is passed as the radial `height` into `cloud_shell_distance`, and `lh` uses flat `light_pos.y`.
- **Evidence**: Re-derived by solving `|t·d + R·ŷ| = R + h` for the shell crossings.
  - `dir.y = 0.16`: crossings at t ≈ 9.3 km and 30.7 km, flat y ≈ 1490 m and 4919 m. Under 2% error, and `horizon_fade` is 1.
  - `dir.y = 0.05`: t ≈ 28.7 km and 87.9 km, flat y ≈ 1436 m and 4393 m, so `height_fraction` spans [0, 0.83] instead of [0, 1]. `horizon_fade` ≈ 0.15.
  - `dir.y = 0.02`: t ≈ 60.6 km and 155.3 km, flat y ≈ 1212 m and 3106 m, so `height_fraction` spans [0, 0.46]. The first ~8% of the ray sits below the flat base.
- **Impact**:
  - Clouds in the 0.015–0.16 elevation band are drawn with a truncated, bottom-heavy vertical profile.
  - This is inherited by the composite background and by every cube texel near the horizon, including RT reflection misses toward the horizon.
  - Most of it is hidden by `horizon_fade`, and it is negligible above ~9° elevation. Visual only, no NaN.
- **Related**: `564d0d2f`, `9ac8a929`, `5d5d6ddd`.
- **Suggested Fix**:
  - Use the sample's radial altitude, `length(position + vec3(0, CLOUD_PLANET_RADIUS, 0)) - CLOUD_PLANET_RADIUS`, for both `height_fraction` and the light-march `height`/`lh`, so the density profile matches the shell the march is bounded by.
  - Verify with the skyal.md §4 screenshot harness.

### REN-2026-09-14-D18-03: new cloud-shape constants in `clouds.glsl` carry no citation, and skyal.md's provenance list names only two of them while the constants banner claims every value is sourced
- **Severity**: LOW
- **Dimension**: Sky/Weather
- **Location**: `crates/renderer/shaders/include/clouds.glsl` (`cloud_height_gradient`, `cloud_density`, `cloud_march`); `docs/engine/skyal.md` (§2.3 "Still open"); `crates/renderer/src/shader_constants_data.rs` (SKYAL banner above `CLOUD_LAYER_BOTTOM`)
- **Status**: NEW
- **Description**:
  - The No-Guessing policy requires a source, or an explicit "uncited" record, for tuned values.
  - skyal.md "Still open" records two items as unjustified: the coverage mapping (0.86/0.80/0.70/0.40/0.55) and the noise frequencies (`0.00008`/`0.0009`).
  - The following literals are also new with SKYAL (`c379898f`/`564d0d2f`) but carry no citation in code or doc, and are not on that list:
    - `cloud_height_gradient` remap breakpoints `0.15` and `0.55`;
    - erosion strength `erosion * 0.45`;
    - erosion height blend `height_fraction * 5.0`;
    - detail-noise advection `wind_offset * 3.0`;
    - wind scale `* 0.00002` in `cloud_march`.
  - The `shader_constants_data.rs` banner says "every value below follow[s] Schneider & Vos … not tuned by eye". The constants below it are cited individually, but the MS falloffs cite Skybolt as a secondary reference with owner sign-off. The in-shader literals above sit outside that banner and are sourced nowhere.
  - Verified none of these came from the pre-volumetric 2D body: a grep of `62a09fd9:crates/renderer/shaders/composite.frag` returns 0 hits for each. By contrast, the tint `0.45`/`0.08`, alpha `0.78`/`0.96` and horizon fade `0.015/0.16` were in that file, so the "carried over verbatim" claims for those do hold.
- **Evidence**: `cloud_height_gradient`: `cloud_remap(height_fraction, 0.0, 0.15, 0.0, 1.0)` / `cloud_remap(height_fraction, 0.55, 1.0, 1.0, 0.0)`; `cloud_density`: `detail_uvw = vec3(position.xz * 0.0009 + wind_offset * 3.0, …)`, `mix(detail, 1.0 - detail, clamp(height_fraction * 5.0, 0.0, 1.0))`, `cloud_remap(shaped, erosion * 0.45, 1.0, 0.0, 1.0)`; `cloud_march`: `… * time * 0.00002`.
- **Impact**:
  - Doc/provenance only. These values shape cloud silhouettes and drift speed, and the "sourced, calibrated" framing of `564d0d2f` makes them look already justified to the next reader.
  - That invites a future retune to treat them as reference values, or to skip justifying them.
- **Related**: `564d0d2f`, `66e86ce1` (skyal.md), #4230.
- **Suggested Fix**:
  - Add these literals to skyal.md's "Still open" provenance list, or cite them if a source exists; the Schneider & Vos height-gradient discussion does not state breakpoints.
  - Narrow the constants banner to "cited per constant below".

### REN-2026-09-14-D20-01: the ground-cover interaction and scatter compute dispatches (and their counter clears) run outside every GPU timer bracket — the exact gap 74df367f closed for the sky-cube bake, re-opened one pass later in the frame
- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/groundcover.rs` (`GroundCover::record_scatter`, `record_interaction`); call site `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (`dispatch_skin_and_cluster`, the `gc.record_scatter(...)` block)
- **Status**: NEW
- **Description**: `record_scatter` records the §12.4 interaction dispatch (`record_interaction`), three `vkCmdFillBuffer` counter clears and one screen-independent scatter `cmd_dispatch`. The scatter traces a ray query per candidate against the TLAS (the placed-geometry cover test). This all runs every frame an exterior has ground cover. The call site sits after `cmd_cluster_cull_end` and before `record_groundcover_bench`. `cmd_main_render_start` is not reached until `geometry_pass.rs`. So this work lands in no bracket, and it is not even misattributed to a neighbour. `groundcover.rs` has no reference to timers at all (`grep -n "timers" crates/renderer/src/vulkan/groundcover.rs` returns zero hits).
- **Evidence**: 74df367f's own message states the rationale: "The bake … ran unmeasured … How the visible sky should adopt the cloud layer … is a cost decision, and it should be made from a number". It added bracket 18 for exactly that reason. The ground-cover scatter is the same shape: per-frame compute with a data-dependent ray-query cost, and density/LOD tuning (#4056 Phase 3, open) is a cost decision. #3676 closed this bug class for `skin_palette.comp` and two others. The blade draw itself is inside the `main_render` bracket (`record_draw` precedes `cmd_end_render_pass` in `geometry_pass.rs`), so only the compute half is unmeasured.
- **Impact**: Observability only. On exterior cells the scatter/interaction GPU time is invisible to `bench:` lines, the metrics map and the debug-UI grid, so a ray-query-heavy density regression shows up as unexplained frame time. No correctness risk.
- **Related**: #3676 (same class, closed), 74df367f (sky-cube bracket), #4052 (bench-only ground-cover bracket, which does not cover the production scatter), #4056.
- **Suggested Fix**: Add a ground-cover bracket around the `record_scatter` call: `QUERIES_PER_FRAME` 36→38, a new `BIT_`, plus `_ms`/`_active` through `GpuTimerSnapshot`, `SkinCoverageStats` and the three consumers. Extend the module-doc table so d28722fb's row-count test stays green. If a bracket is judged not worth it, add a sentence to the table noting the unmeasured scatter.

### REN-2026-09-14-D21-01: Seven shader-consumed `GpuMaterial` scalars still have no `mat.set` arm, so the soft and rim lighting lobes cannot be exercised from the harness
- **Severity**: LOW
- **Dimension**: Cornell Harness
- **Location**: `byroredux/src/commands/scene.rs` (`MatSetCommand::execute`, its `USAGE` string, the #4023 arm comment)
- **Status**: NEW (#4023's premise was incomplete, not a regression)
- **Description**: #4023 added the four `glass_*` arms under a comment calling them "the last four shader-consumed `GpuMaterial` scalars with no `mat.set` arm". That is false. Seven more scalars are assigned from `Material` into `DrawCommand` in `collect_static_mesh_draws`, carried into `GpuMaterial` by `to_gpu_material`, and read by shaders, yet have no arm and no `USAGE` entry:
  - `lighting_effect_1`
  - `lighting_effect_2`
  - `subsurface_rolloff`
  - `rimlight_power`
  - `backlight_power`
  - `fresnel_power`
  - `grayscale_to_palette_scale`

  The `MAT_FLAG_SOFT_LIGHTING` / `RIM_LIGHTING` / `BACK_LIGHTING` bits can be set through the `material_flags` arm. The scalars those lobes read cannot, and Cornell's constructors leave them at `Material::default()`:
  - **Soft lighting**: `lighting.glsl` takes its wrap width from `subsurfaceRolloff > 0 ? subsurfaceRolloff : lightingEffect1`. Both are 0.0, so the width is 0.
  - **Rim lighting**: the exponent comes from `rimlightPower > 0 ? rimlightPower : lightingEffect2`. The code comment calls both-zero the "nothing authored" state.
  - **Back lighting** falls back to strength 1.0, and `fresnel_power` stays at its default 5.0.

  So the harness can turn the soft and rim lobes on but cannot sweep them away from their degenerate defaults. That is the same shape #2514 (Disney scalars), #2823 (translucency suite) and #4023 (glass optics) each closed one group at a time.
- **Evidence**:
  - The arm list in `MatSetCommand::execute` has no `"lighting_effect_1"`, `"rimlight_power"`, etc., and falls through to `other => Err(format!("unknown field \`{other}\`"))`.
  - `static_meshes.rs`: `rimlight_power: mat.map(|m| m.rimlight_power).unwrap_or(0.0)` and the sibling lines.
  - `rg -l rimlightPower|lightingEffect1|subsurfaceRolloff|fresnelPower|grayscaleToPaletteScale crates/renderer/shaders` hits `crates/renderer/shaders/include/lighting.glsl` and `triangle.frag`.
  - `rg "rimlight|backlight|lighting_effect|fresnel_power|subsurface_rolloff|grayscale_to_palette" byroredux/src/cornell.rs` returns no hits.
- **Impact**: Harness coverage only. The Bethesda soft/rim/back lighting response (landed 2026-08-25, FLT_MAX-sentinel and rim-floor bugs #3452/#3448 in its history) cannot be A/B'd in the RT reference scene, so regressions there have to be found by static reading or on game content.
- **Related**: #4023, #2823, #2514, #3452, #3448, #3460.
- **Suggested Fix**: Add `set_scalar` arms plus `USAGE` entries for the seven fields, and correct the #4023 comment. To stop this recurring a fourth time, add a test asserting every `f32` lane written in `to_gpu_material` either has a `mat.set` arm or appears on an explicit allowlist.


## Prioritized Fix Order

1. **D18-01** (HIGH) — delete the `params.x` factor from the blade directional loop and pin it. One-line correctness fix with the largest visible effect in any exterior; re-screenshot grass afterwards because its tuned look was calibrated without sun.
2. **D6-01** (HIGH) — run the external-material merge against the MSWP target before `translate_material`; scope #4229's test to TXST provenance.
3. **D9-01** (MEDIUM) — move the MorphSlot LRU bump to `update_morph_weights` or exempt MorphSlots from idle eviction.
4. **D2-01** (MEDIUM) — route the three remaining sky-miss sites through the gated `skyCube` sample; make the consumer test discover call sites.
5. **D11-01** (MEDIUM) — per-pipeline blend array for the ground-cover debug view.
6. **D4-01** (MEDIUM) — confirm with `BYRO_VALIDATION=1`, then barrier the seed fills.
7. **D23-01 / D23-02** (MEDIUM) — jitter the blade VP and decide the mask policy from an A/B capture; record the result in both FSR docs.
8. **LOW test/guard gaps** — D11-02 (fold into #4110), D11-03, D20-01, D6-03, D13-01, D21-01.
9. **LOW doc-rot** — D2-02, D3-01, D5-01, D12-01 (`shader-pipeline.md` / `memory-budget.md` catch-up, best done in one pass), D6-04, D6-05, D8-01, D8-02, D14-01, D16-01, D16-02, D18-03.
10. **LOW latent** — D6-02, D18-02.

## Needs-RenderDoc / Capture Verification

- **D4-01** — needs a `BYRO_VALIDATION=1` run on an exterior with ground cover to attribute the "10 vkCmdFillBuffer WAW hazards" recorded in `6db9eac2` before any barrier change.
- **D23-01, D23-02, D16-01, D18-01** — visual; fix direction is clear from code, but the result should be confirmed by an in-engine A/B capture (no RenderDoc integration exists).
- **#4177 / #4179** (closed, verified present) — barrier placement is source-pinned; GPU-side effectiveness remains capture-only.

No finding proposes a speculative render-pass, pipeline, or barrier edit.

## Verified-Clean Dimensions

D1 (Acceleration Structures), D7 (Material Table), D10 (Camera-Relative
Precision), D15 (Water), D17 (Disney BSDF / Soft Shadows), D19 (Tangent-Space),
D22 (Light Animation). Dimensions with only LOW findings also re-verified their
full checklists: D8, D13, D14, D16, D20, D21.

## Pre-Existing Open Issues Confirmed Still Present (not re-filed)

- **#4111** — resize recomputes the BLAS budget from the pre-resize upscaler's SDK memory figure; `recreate_bloom_and_volumetrics` still runs before `recreate_taa_and_presentation`.
- **#4110** — now also covers ground cover (D11-02).
- **#3572**, **#4203**, **#4204**, **#4205**, **#4114**, **#4201**, **#4182**, **#4180**, **#4246** (plus `object_lod.rs` as a fourth caller without Phase-2 resolvers), **#4230**, **#3922**, **#4214**.

## Stale Skill Premises (for the next `/audit-renderer` sync)

- Dims 1/2/10/23 entry points omit the ground-cover shaders, `sky_cube.comp`, `crates/renderer/shaders/include/clouds.glsl`, and the older TLAS readers `volumetrics_inject.comp` / `caustic_splat.comp`.
- Dim 2: "`sceneFlags.x > 0.5` before every ray query" does not fit `groundcover_scatter.comp` (push-constant gate); the GI miss bullet predates the cube.
- Dim 3: `exterior_sky_tint.w` is live; `SkyCubeParams` and the ground-cover mirrors are unlisted contracts.
- Dim 4/5/12: no mention of the sky cube, cloud noise, or ground-cover barriers, teardown, or recording order.
- Dim 6: `every_exterior_spawner_inserts_a_boundary_material` does not scan subdirectories, so "rely on the test" overstates it (D6-03).
- Dim 9: skinned output usage omits `SHADER_DEVICE_ADDRESS`; dispatch math is `div_ceil(SKIN_WORKGROUP_SIZE)`.
- Dim 11: "the G-buffer pipeline writes all eight color attachments" is true only of `triangle.frag`.
- Dim 14: caustic source selection reads `instances[].flags`, not `materials[material_id]`.
- Dim 16: bloom has no disable path; "4-tap bilinear down" invites D16-01's misreading.
- Dim 18: still says "fog applied to direct only".
- Dim 22: attach sites are `byroredux/src/cell_loader/references/synth_child.rs`, `byroredux/src/cell_loader/references/attach.rs`, `byroredux/src/cell_loader/spawn/mesh_instance.rs`, not *references/mod.rs*.
- `_audit-common.md`: 32 GLSL entry-point sources, not 22.
- Prior report `AUDIT_RENDERER_2026-09-11.md` D11-01 wrongly said `groundcover.rs` calls `validate_set_layout`.
