**HEAD**: `4a3e89d56` · **Baseline**: `AUDIT_RENDERER_2026-10-14.md` (HEAD `adbd468e2`, the last full renderer audit; 2026-10-17, 2026-10-18 and 2026-10-19 were area-scoped and are not baselines) · **Audited**: Dims 2, 3, 4, 5, 6, 7 traced in depth over the delta `adbd468e2..4a3e89d56` (456 commits, 53 under `crates/renderer`); Dims 1, 10, 12 delta-read · **Unchanged since baseline (skimmed)**: Dim 9 (skinning: only #5593 and #5946 refactors), Dim 11 (FSR/exposure/presentation: only #5809, #5824, #5849 hygiene), Dim 8 (volumetrics: only the fixes for the 2026-10-17 findings, which have their own guards)

# Renderer Audit — 2026-10-21 (full, delta)

## Executive Summary

| Severity | NEW | Findings |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 1 | REN-H-01 |
| MEDIUM | 2 | REN-M-01, REN-M-02 |
| LOW | 2 | REN-L-01, REN-L-02 |

The window closed every 2026-10-14 finding: REN-H-01 by the `atomicMin` claim (`a03cf9174`, #5765), REN-M-01 by the reprojection test (`cb0cf5798`, #5766), and the LOW items by `24eb7dba8`, `2e7d5bead`, `1a34efabf`, `6cbfe4d72`, `5d2679372`, `67476a5d7`, `1a2c48008`, `dfab09c51`/`76e4414ec` and `e29256ca2`. I re-read each fix, and none is a regression.

New code in the window:
- a new post pass, the direct-shadow spatial filter. It landed inside a commit titled "docs: update debug CLI commands…" (`ee0ba7069`), so no earlier audit traced it end to end.
- the per-pixel ReSTIR mover taint, which landed inside a Papyrus audit commit (`3934f46d7`).
- the EyeEnvmap shading branch (`b4d073454`).
- the SKYAL cloud-shadow map (`c231f5b5f`).
- terrain height lanes (`5017a64c2`, `ea45af067`).

Three defects found:
1. **The EyeEnvmap reflection mixes render-origin-relative and absolute positions** (REN-H-01).
2. **The shadow filter adds the reservoir owner's correction to pixels whose visible surface never claimed the slot** (REN-M-01).
3. **The mover taint misses the pixels a mover un-occludes on its first moving frame, and after any teleport.** Those pixels drop into the 1024-frame / 0.002-floor tail (REN-M-02).

**Method.** Static review only, solo, read-only. No engine, no Vulkan device, no game-data launch (suite rule). For each dimension: `git log adbd468e2..HEAD` over its paths, the delta read, then the owning guards.

**Guards run** (repo `target/`; all pass; nothing named is `#[ignore]`d except where noted):
- `scripts/check-shader-artifacts.sh`: 38 shaders plus the early variant match glslang byte for byte.
- `cargo test -p byroredux-renderer --lib`: 1458 passed, 1 ignored (GPU-only). Includes `every_shader_struct_is_classified`, the size and offset pins, `acceleration`, `shader_contract`, `restir`, `light_history`, `gpu_timers`, `bytes_per_pixel_matches_frame_images`, `a_failed_layout_submit_leaks_instead_of_destroying`, `every_direct_sun_site_applies_the_cloud_shadow`, the `view_proj_is_static` family and `no_file_states_a_stale_gpu_material_size`.
- `cargo test -p byroredux --bin byroredux` (filters: `static_blas_recovery`, `bone_palette_overflow`, `skin_dispatch_ran_rollback`, `renderer_config_defaults_to_fsr_quality`, `bench_gpu_key`, `mat_set`, `eye_pack`, `directional_source_contract`, `every_exterior_spawner`, `shadow_spotlight_bit`, `fallout3nv_zero_projection`): 35 passed.
- `cargo test -p byroredux-core --features inspect` (`resolve_pbr_is_idempotent`, `glass_behavior_preserves…`): 2 passed.
- `cargo test -p byroredux-nif --test translation_completeness -- --ignored cross_game_translation_completeness`: passed (2.7 s).
- `cargo test -p byroredux-fsr3-sys`: 8 passed.
- Not run (need an RT device): `cornell_parked_penumbra_converges_at_one_shadow_ray`, `gpu_filter_preserves_constant_radiance_and_broadens_a_lobe`.

## RT Pipeline Assessment

- **AS (Dim 1).** Only `9762d9fa2` (#6035) touches the BLAS budget. It now bills the reservoir slot at 44 B/px (reservoir + claim + U) and the shadow filter at 24 B/px per frame in flight; `per_slot_bytes_per_pixel_covers_every_region` pins it. The cloud-shadow map and the terrain height maps are fixed-size, not screen-scaled, so leaving them out of `screen_scaled_reservation_bytes` is correct. The cloud-shadow map is ledgered in the SKYAL sky-bake row of `memory-budget.md`.
- **SSBO indexing.** Every new reader indexes `instances[]` by the TLAS custom index (`traceShadowTransmittanceDetailed` → `committedInstance` → `instances[selectedRayHitInstance].flags`). `INSTANCE_FLAG_MOVED_THIS_FRAME` is assembled above the raster-only cut, so off-screen occluders carry it.
- **Reservoir ownership (#5765).** The claim is a real `atomicMin` on binding 22, which `begin_frame` resets to `u32::MAX` after zero-filling the reservoir region. Region offsets are 256-aligned (`CLAIM_OFFSET_ALIGN`), and the fill sizes are multiples of 4. The remaining store-after-claim race is documented in the shader as costing one history reset; that is true for history, but the shadow filter consumes the slot in the same frame (REN-M-01).
- **Parked camera (#5766).** `view_proj_is_static` reprojects jitter-free matrices in f64 at probe depths 32/256/2048. The smallest per-frame creep it can miss at the 32 BU probe is about 0.009 BU/frame, which is negligible. No finding.
- **Temporal gates.** The plane-distance gate (`17e0b7464`) and the 3×3 silhouette search (`6110b6078`) read the same surface, plane and normal gates through generated constants. No finding.
- **Denoiser.**
  - SVGF's floored history-age cap (`1092b5b9e`) is sound: the cap is `1/floor`, which is 5 at the 0.2 floor.
  - Dropping scene-wide motion from the SVGF gate (`3f8194117`) is backed by prior-palette skinned motion vectors (`triangle.vert`, set 1 binding 12) and the normal-cone rejection, so moving actors do not smear.
- **Coordinate spaces.** The cloud-shadow consumers pass absolute positions and an absolute `cameraPos`, which is consistent. The EyeEnvmap branch does not (REN-H-01).

## GPU-Struct & Memory Assessment

- **Struct pins.** `GpuInstance` 160, `GpuCamera` 368, `GpuLight` 64, `GpuTerrainTile` 208 (height lanes @168/@172, `height_blend_band` @204) and `GpuMaterial` 432 all pass. `shader-pipeline.md` documents the terrain-tile lanes. The instance-flag table is stale (REN-L-02).
- **`Reservoir` copies.** `Reservoir` now has a second GLSL copy in `shadow_filter.comp` that no lockstep test covers (REN-L-01).
- **New owners.**
  - The shadow filter is created with push-then-fill, destroyed before `reservoir_buffers`, and leaks its images on a may-be-in-flight layout submit (#6063).
  - The resize path recreates its images before composite rewrites set 0 binding 11. The fallback view is `albedo_views` in `SHADER_READ_ONLY_OPTIMAL`.
  - `CloudShadow` is destroyed inside `SkyCubePipeline` teardown.
  - Height maps are released through `GpuTerrainTile::texture_indices`.
- **Sync of new passes.**
  - Shadow filter: FRAGMENT→COMPUTE on the reservoir buffer, COMPUTE→COMPUTE ping-pong, COMPUTE→FRAGMENT on the correction image.
  - Cloud shadow: UNDEFINED→GENERAL every frame (the bake is unconditional), COMPUTE→FRAGMENT (every consumer is a fragment shader: `lighting.glsl`, `groundcover_blade.frag`, `water.frag`), and COMPUTE→HOST on the 16 B readback.
  - The `begin_frame` TRANSFER→FRAGMENT edge chains through the FRAGMENT→COMPUTE barrier to the filter's read of zero-filled slots.
  - Nothing found. None of this was sync-validated live (see Needs-RenderDoc).

## Findings

### REN-H-01: The EyeEnvmap branch mixes render-origin-relative eye centres with the absolute fragment position, so the "iris-centred" reflection collapses to a direction near `renderOrigin`
- **Severity**: HIGH (the skill's Dim 2 floor: a path mixing the two coordinate conventions = HIGH). Visual blast radius is limited to eyes authored as shader type 16.
- **Dimension**: Camera-Relative Precision / NIFAL Material
- **Location**: `crates/renderer/shaders/triangle.frag`, the `if (isEyeEnvmap)` block (`leftWS`, `rightWS`, `centerWS`, `eyeR`), from `b4d073454` (#5961)
- **Status**: NEW. It was introduced by the fix for closed #5961; it is not a regression of an earlier fix.
- **Description**:
  - `leftWS = (instances[fragInstanceIndex].model * vec4(mat.eyeLeftCenterX, …, 1.0)).xyz`.
  - Rigid `GpuInstance.model` is render-origin-relative: `rebase_model_matrix` subtracts `render_origin` from the translation in `build_and_upload_instances.rs`.
  - `fragWorldPos` is absolute: `fragWorldPos = fragWorldPosRel + renderOrigin.xyz` at the top of `main()`.
  - The block then takes `distance(fragWorldPos, leftWS)` and `reflect(normalize(fragWorldPos - centerWS), N)`, which mixes the two spaces.
  - `snap_render_origin` floors every axis to 4096, so the origin is essentially never zero. Even a camera at (100, 50, −1) gets origin (0, 0, −4096).
  - The difference is therefore dominated by the ≥4096-unit origin offset. The "view direction from the eye centre" is nearly the same vector for every iris fragment, and the left/right pick is arbitrary.
  - That is the flat, non-coherent reflection the branch exists to avoid.
- **Evidence**:
  - `ray_hit.glsl` does the same reconstruction correctly: `w0 = (hi.model * vec4(v0, 1.0)).xyz + renderOrigin.xyz;`, pinned by the `shader_contract_tests.rs` case near line 745.
  - The eye block has no `renderOrigin` term, and no test mentions `centerWS` or the eye centres' space (`eye_pack_tests` checks only the CPU packing).
- **Impact**: every EyeEnvmap eye (Skyrim / FO4 NPC eyes with an authored cubemap and `eyeCubemapScale > 0`) shows a reflection whose sample direction depends on the camera's 4096-cell rather than the eye geometry. It also jumps when the render origin snaps across a cell boundary. The documented skinned-eye limit (centres track the actor root) is separate and does not cover this.
- **Related**: #5961, #1495/#1496 (coordinate split), #2747 (the `ray_hit.glsl` rebase).
- **Suggested Fix**: add `+ renderOrigin.xyz` to `leftWS` and `rightWS`. Alternatively, do the whole computation in relative space (`fragWorldPosRel`). Add a source-shape pin beside the `ray_hit.glsl` one.

### REN-M-01: The shadow filter adds a correction wherever the reservoir slot was claimed, including pixels whose visible surface never claims — unlit screens, water, ground-cover blades, glass and decals receive the hidden surface's `U × (filtered − raw)`
- **Severity**: MEDIUM (a wrong resolve term on visible pixels; visual only)
- **Dimension**: Denoiser/Composite (shadow filter)
- **Location**:
  - `crates/renderer/shaders/shadow_filter.comp`: `guideAt` takes validity from `histLenAndDepth.x != 0`; `storeResult` writes the correction.
  - `crates/renderer/shaders/composite.frag`: `direct = max(direct + texture(shadowCorrectionTex, fragUV).rgb, 0)` under `caustic_flags.y`.
  - `crates/renderer/shaders/triangle.frag`: the claim block under `if (stableTemporalSurface)`; the `MATERIAL_KIND_NO_LIGHTING` early return.
- **Status**: NEW. The 2026-10-18 report recorded the blade/water instance as an observation only (its Needs-RenderDoc item 3); this finding generalises it and adds the opaque cases and the torn-slot case.
- **Description**:
  - A slot is valid if any `stableTemporalSurface` fragment (`!isAlphaBlend && !isGlass`) that reached the ReSTIR finalize won `atomicMin` this frame.
  - The filter then writes `U · strength · (filtered − raw)` for that pixel, and composite adds it to the visible pixel's direct light.
  - The visible surface is often not the claimant:
    - **Opaque `MATERIAL_KIND_NO_LIGHTING` (102)** surfaces (terminal screens, signs) return at the `outColor = vec4(emit, texColor.a)` early exit and never claim. They run in the late-test pipeline (kind > `MATERIAL_KIND_MAX_LIGHTING_SHADER`), and draw order is front-to-back only coarsely within a group (`draw_sort_key`). A lit surface behind them that was drawn earlier, or an alpha-tested cutout behind them in the late-test pipeline, owns the slot.
    - **Water and ground-cover blades** never claim, so the terrain or lake floor beneath owns the slot (the 2026-10-18 observation).
    - **Glass and blended decals** never claim. The opaque surface behind owns the slot, and its full correction is added to the composited colour although that surface reaches the pixel only through the blend's (1 − α).
  - **Torn slot.** The claim and the two stores are not atomic as a unit. The shader comment accepts that a farther fragment's late store can land after a nearer winner's, which costs only a history reset next frame. But `reservoirsCurr[pixelIdx] = rc` and `directUnshadowed[pixelIdx] = …` are separate stores, so the filter can pair one fragment's ratio and guides with another fragment's U in the same frame, with no gate.
- **Evidence**:
  - `composite.frag`'s comment says the term "replace[s] the shaded U * rawRatio by U * filteredRatio", which holds only if the slot owner is the visible surface.
  - Nothing in `shadow_filter.comp` compares the slot's surface against the visible pixel. The G-buffer mesh ID (opaque = stable surface ID, bit 31 = blended) is available but not bound to the pass.
  - Guards are thin (`bytes_per_pixel_matches_frame_images`, `a_failed_layout_submit_leaks_instead_of_destroying`), and no test covers ownership.
- **Impact**:
  - Light or dark smudges inside about 6 px of the owner's penumbra edges appear on surfaces the owner does not shade.
  - The worst cases are emissive screens in front of lit walls, and water over a shadowed shore floor.
  - Magnitude scales with the hidden owner's U and with `strength`, which is 1 while its history is short, i.e. whenever the camera moves.
  - Not measured (no engine launch).
- **Related**: #5765 (claim), #6035 (billing), REN-EXT 2026-10-18 Needs-RenderDoc item 3.
- **Suggested Fix**: bind the mesh-ID attachment to the filter. Write a zero correction when the visible mesh ID has bit 31 set or does not equal the slot's surface tag (`lightAndSurface` & `RESERVOIR_SURFACE_MASK`, the same identity the claim writes), and use the same test as the centre-pixel validity check. That also rejects torn slots whose reservoir came from a different surface. No barrier or layout change beyond one more sampled input (needs a `BYRO_VALIDATION=1` run when added).

### REN-M-02: The ReSTIR mover taint misses pixels a mover un-occludes on its first moving frame, or after a one-frame teleport, and keys on the first ray hit rather than the occluder; those pixels keep the 1024-frame / 0.002-floor tail and the old shadow ghosts for seconds
- **Severity**: MEDIUM (ghosting; the skill's Dim 7 floor)
- **Dimension**: Ray Queries / Denoiser (ReSTIR direct-light EMA)
- **Location**:
  - `crates/renderer/shaders/triangle.frag`: the `moverHit` / `prevTainted` / `deep` block before `Lo += restirUnshadowedSum * accum`, and the `if (sr == 0)` capture of `selectedRayHitInstance`.
  - `crates/renderer/shaders/include/shadow_transport.glsl`: `traceShadowTransmittanceDetailed`, where `committedInstance` is the first hit.
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`: `instance_moved`, `restir_rig_static`. From `3934f46d7`.
- **Status**: NEW
- **Description**:
  - Since `3934f46d7`, movers no longer clear `restir_rig_static` scene-wide. A parked camera with a mover anywhere sits in mode 3 (rig static), or mode 2 when nothing moved, and both run the deep tail.
  - A pixel leaves the deep tail only if this frame's selected shadow ray hits an instance flagged `INSTANCE_FLAG_MOVED_THIS_FRAME`, or if it was already tainted. Three holes:
    1. **First moving frame.** `instance_moved` is `previous_source != m`. On the first frame an occluder moves, the pixels it uncovers already miss it and were never tainted, because it was static the frame before. They blend toward lit at α = max(1/histLen, 0.002), about a 346-frame half-life, so a sliver of the old shadow edge lingers. The same applies to every pixel uncovered by a teleport (one frame flagged, then static). An instance that *vanishes* is handled by the set-change gate; one that *jumps* is not.
    2. **First hit, not the occluder.** `committedInstance` is set on the first committed hit, including alpha-tested layers that `rayHitHasCoverage` then skips (foliage, fences) and effect cards. An NPC standing behind a fence relative to the light reports the fence, which is static, so its shadow is never tainted.
    3. **Sample 0 only.** Only `sr == 0` records the hit. With `directShadowSamples > 1`, penumbra pixels whose other cone taps hit the mover stay untainted.
- **Evidence**: the code paths above. The skill states that no test pins the taint, and none does.
- **Impact**:
  - With the camera parked, an NPC that starts walking, a door that starts swinging, or a scripted reposition leaves ghost shadow strips that take seconds to clear. That is the ghosting #4942 removed with the scene-wide gate.
  - Hole 2 makes it systematic for actors seen through foliage or fences.
  - Not measured; the `restir_history` view (orange = tainted) is the on-device check.
- **Related**: #5369, #4942, `3f8194117` (SVGF made the same scene-wide → per-pixel move), REN-M-01.
- **Suggested Fix**:
  - Taint on the *previous* frame's occluder as well. For example, store the hit's stable surface ID or instance in the reservoir, and taint when last frame's recorded occluder carries the moved flag this frame. Or keep `INSTANCE_FLAG_MOVED_THIS_FRAME` set for `RESTIR_MOVER_SETTLE_FRAMES` after motion stops and for one frame before (the previous-frame transform is already uploaded).
  - Record the instance that actually returned zero visibility (the covered or opaque hit), not the first committed candidate. Consider OR-ing the flag across all `sr` samples.
  - Pin with a source-shape test, since the GPU behaviour needs RenderDoc.

### REN-L-01: `Reservoir` now has two GLSL copies, `include/bindings.glsl` and `shadow_filter.comp`, with no lockstep test between them
- **Severity**: LOW (latent; a future edit to one copy silently misreads every field in the other)
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/shaders/shadow_filter.comp` `struct Reservoir` (its own comment: "Not yet pinned by a GLSL-copies lockstep test"); `shader_contract_tests.rs` classification `("Reservoir", MirroredPendingGuard(…))`; `restir.rs` `reservoir_stride_matches_shader` (asserts `RESERVOIR_STRIDE == 8 * 4` against a literal, reads no GLSL)
- **Status**: NEW
- **Description**: `every_shader_struct_is_classified` matches by name, so the second copy passes. The stride test reads neither copy. A field reorder in `bindings.glsl`, for example moving the packed normal out of `pad0`, would leave the filter decoding ratio and guides from the wrong lanes.
- **Suggested Fix**: have `shadow_filter.comp` include the shared definition (as `struct GpuMaterial` lives only in `bindings.glsl`), or add both copies to a GLSL-vs-GLSL `assert_glsl_matches_rust`-style comparison.

### REN-L-02: `shader-pipeline.md`'s `GpuInstance.flags` table omits bit 10 (`INSTANCE_FLAG_MOVED_THIS_FRAME`) and misdescribes bit 9
- **Severity**: LOW (doc rot)
- **Dimension**: GPU-Struct Layout (docs)
- **Location**: `docs/engine/shader-pipeline.md`, the flag table under the `GpuInstance` layout
- **Status**: NEW
- **Description**:
  - Bit 10 was added by `3934f46d7` and has no row.
  - Bit 9's row says "Exclude this draw from the RT LOD path". The only shader reader is the `triangle.frag` debug-view `lodBlock` colour, and `scene_buffer/constants.rs` calls it "Raster-only diagnostic state".
- **Suggested Fix**: add the bit 10 row (set for a rigid matrix change or a dirty pose, read on the selected shadow ray's hit) and correct bit 9.

## Prioritized Fix Order

1. **Correctness:**
   - REN-H-01 (one-line rebase plus a pin).
   - REN-M-01 (gate the filter on visible-surface ownership).
   - REN-M-02 (previous-occluder taint and the occluding-hit instance).
2. **Robustness:** REN-L-01 (single `Reservoir` definition).
3. **Docs:** REN-L-02.

## Needs-RenderDoc

1. **REN-M-01.** On an FO4 or Skyrim interior with an emissive terminal in front of a lit wall, and on a sunlit shoreline, compare `r.shadowfilter on` against `off` while the camera moves. Run once under `BYRO_VALIDATION=1` to cover the new pass's barriers (not validated by this audit).
2. **REN-M-02.** With the camera parked, `render.debug restir_history`: watch an NPC start walking (look for the untainted sliver at the shadow's trailing edge) and an NPC behind a fence or foliage (no orange taint).
3. **REN-H-01.** Close-up of a Skyrim NPC's EyeEnvmap eye, before and after crossing a 4096 render-origin snap.
4. **Cloud-shadow sync.** The UNDEFINED→GENERAL transition each frame and the HOST readback are reasoned sound but unvalidated.
5. **Carried:** the FP32 FSR permutation is untested; no test pins the egui `drop_without_applying_deltas` discard sites; TLAS UPDATE after a shifted `instance_custom_index` is validation-only.

## Dimension notes (no further findings)

- **Dim 1:** guards green. `step_static_blas_restore` ordering is unchanged. The 2026-10-19 streaming-slice findings are tracked (#6106, #6111) and not re-reported.
- **Dim 4:** `r.dbg` (`458a84e2f`) signals the temporal discontinuity on a raw-output boundary crossing. `skip_clear_latches_are_promoted_after_submit_not_at_record_time` (#5956) and the ground-cover field advance (#6031) are in place. The shadow filter and its composite flag read one predicate (`shadow_filter_active`) in the same `draw_frame`.
- **Dim 5:** see the GPU-Struct & Memory Assessment. `acquire_matching_geometry` now tests `refcount(handle).is_none()` (#6112).
- **Dim 6:** besides REN-H-01, the MLP `refraction_scale` path and the eye packing tests pass.
- **Dim 7:** besides REN-M-01, the SVGF changes are sound (RT Pipeline Assessment).
- **Dim 10:** the cloud transmittance is applied once per directional arm (`shadowableLightRadiance`, `giLightSample`) on absolute positions, and the volumetric and caustic sun take the camera-column value. The sky-dome colour decode (`ae557b935`) sits at the boundary.
- **Dim 12:** the shadow-filter bracket uses Q46/47. `QUERIES_PER_FRAME` is 58 (24 named brackets plus 5 geometry phases). `NESTED_GPU_BRACKETS` lists `shadow_filter` → `svgf`, and `BENCH_GPU_KEYS` and `metrics_sample_system` carry it.

## Dropped candidates

- A NaN U on an invalid slot multiplied by strength 0: U is written only by claim winners and clamped at 65504. Not reachable from finite shading.
- The ReSTIR temporal plane gate on forward camera motion (the previous distance is measured from the previous eye): pre-existing design; the comment documents giving up history.
- Creep below the `view_proj_is_static` threshold: at most about 0.009 BU/frame at the 32 BU probe. Negligible.
- Terrain height-lane patch leak: `update_terrain_tile` runs in the same call as the vacancy check, and `texture_indices` releases the lanes.

## Stale skill premises (for the next `/audit-renderer` sync)

- **Dim 3:** "today only `Reservoir`, whose Rust side is a byte-stride constant": `Reservoir` now also has a second GLSL copy in `shadow_filter.comp` (REN-L-01).
- **Dim 2 / Dim 7:** the mover-taint description should say that the taint is the first committed hit of shadow sample 0 only (REN-M-02). The shadow-filter bullet should name the ownership assumption (REN-M-01).
- **Dim 6:** add the EyeEnvmap centre-space rule (relative `model` + `renderOrigin`) next to the documented skinned-eye limit.
- **Dim 1 / Dim 5:** the shadow filter and EyeEnvmap work landed in commits whose titles name other subsystems (`ee0ba7069` "docs: …", `3934f46d7` "Audit findings for Papyrus…"). Delta scoping should key on paths, not commit titles.
