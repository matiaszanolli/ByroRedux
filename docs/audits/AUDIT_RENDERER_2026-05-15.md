# Renderer Audit — 2026-05-15

**Scope**: Full 20-dimension audit of the Vulkan renderer pipeline.
**Prior base**: `AUDIT_RENDERER_2026-05-14_DIM14.md`, `AUDIT_RENDERER_2026-05-14_DIM17.md` (partial prior coverage)
**Open issues checked**: 39 issues fetched from GitHub

---

## Executive Summary

**29 new findings** across 20 dimensions. No CRITICAL issues.

| Severity | Count |
|----------|-------|
| HIGH     | 2     |
| MEDIUM   | 4     |
| LOW      | 23    |

**Dimensions with zero new findings**: Sync (1), GPU Memory (2), Pipeline State (3)\*, Render Pass/G-Buffer (4)\*, Command Recording (5), Shader Correctness (6), Resource Lifecycle (7), Material Table/R1 (14)\*\*

\* has 1 low doc-only finding  
\*\* 4 carry-over findings from prior audit; 0 new

**Highest-priority fixes** (by blast radius):
1. **REN-D19-001 (HIGH)** — `bloomTex` binding 7 dangling when bloom is `None`: spec-UB, device loss on strict Vulkan drivers
2. **REN-D18-001 (HIGH)** — froxel images not cleared after layout transition: doc says "clears", code doesn't
3. **REN-D8-001 (MEDIUM)** — TLAS UPDATE with `primitive_count > source BUILD count`: violates `VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03708`
4. **REN-D18-002 (MEDIUM)** — interior cells accumulate volumetric extinction with no fill light: ~63% scene darkening
5. **REN-D10-003 (MEDIUM)** — SVGF denoised indirect bound with LINEAR sampler: adds unwanted spatial blur to indirect lighting
6. **REN-D16-001 (MEDIUM)** — Starfield `BSGeometry` tangents always `Vec::new()`: all SF meshes fall through to screen-space derivative path

---

## RT Pipeline Assessment

The core RT pipeline is in strong shape. All five `rayQueryInitializeEXT` sites carry correct flags (`gl_RayFlagsTerminateOnFirstHitEXT`), origins, directions, and TLAS binding. Prior audit findings (#789 glass loop, #820 Frisvad basis, #992 mesh-ID R32_UINT) are confirmed fixed.

**One correctness issue (MEDIUM)**: The TLAS BUILD→UPDATE transition violates the spec's primitive-count invariant (REN-D8-001). This could produce silent ray-miss errors in scenes where instance count grows between frames without a resize trigger.

**One quality issue**: The SVGF denoised output is sampled bilinearly (REN-D10-003), adding unintended spatial blur. A NEAREST sampler is the correct choice.

---

## Rasterization Assessment

All G-buffer formats, pipeline state, command recording, and resource lifecycle are correct. The mesh-ID R32_UINT invariants hold across all four consumer shaders. The Drop teardown order is fully correct with all 32 subsystems destroyed in the right sequence.

---

## Findings

### HIGH

---

### REN-D19-001: No fallback binding for `bloomTex` when bloom pipeline is absent

- **Severity**: HIGH
- **Dimension**: Bloom (M58)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2250`, `crates/renderer/shaders/composite.frag:434`
- **Status**: NEW
- **Description**: When `self.bloom` is `None` (creation failure or between destroy and recreate during resize), `composite.frag` binding 7 (`bloomTex`) is left dangling. The shader unconditionally executes `vec3 bloom = texture(bloomTex, fragUV).rgb; combined += bloom * BLOOM_INTENSITY;` with no specialization-constant gate. No code path in `composite.rs` or `draw.rs` binds a black dummy image to slot 7 when bloom is absent.
- **Evidence**:
  ```rust
  // draw.rs:2250 — bloom dispatch only runs when Some
  if let Some(ref mut bloom) = self.bloom { /* ... */ }
  // composite always runs regardless:
  if let Some(ref composite) = self.composite {
      composite.dispatch(&self.device, cmd, frame, img, bindless_set);
  }
  ```
- **Impact**: Dangling descriptor read — spec-UB, potential device loss on strict Vulkan drivers (e.g. MoltenVK, some AMD Linux drivers). On NVIDIA in debug mode, validation layer error every frame bloom is absent.
- **Suggested Fix**: In `composite.rs`, allocate a 1×1 black `B10G11R11_UFLOAT` image once at creation; bind it to slot 7 in `update_descriptors` when `bloom_image_view` is `None`. Alternatively, add a `BLOOM_ENABLED` specialization constant so the shader branch is compiled out.

---

### REN-D18-001: Froxel images not cleared after layout transition — undefined transmittance on first enable

- **Severity**: HIGH
- **Dimension**: Volumetrics (M55)
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:645-673` (`initialize_layouts`)
- **Status**: NEW
- **Description**: `initialize_layouts` transitions all froxel images UNDEFINED → GENERAL but issues no `vkCmdClearColorImage`. The doc-string says it "clears them" — it does not. If `VOLUMETRIC_OUTPUT_CONSUMED` is enabled before the first integration dispatch completes, composite reads driver-undefined `vol.a` values; since `combined = combined * vol.a + vol.rgb`, `vol.a ≈ 0` collapses the entire scene to black.
- **Evidence**:
  ```rust
  // volumetrics.rs:660-672 — barrier only, no clear
  device.cmd_pipeline_barrier(cmd, TOP_OF_PIPE, COMPUTE_SHADER, ...);
  // doc at initialize_layouts:620 says "clears them" — incorrect
  ```
- **Impact**: Scene-black frame on first volumetrics enable or after any resize that re-runs `initialize_layouts`. Also, the misleading doc comment is a maintenance hazard.
- **Suggested Fix**: After the barrier in `initialize_layouts`, call `cmd_clear_color_image` with `{0.0, 0.0, 0.0, 1.0}` (zero inscatter, full transmittance) on all froxel images. Fix the doc comment.

---

### MEDIUM

---

### REN-D8-001: TLAS UPDATE primitive_count exceeds source BUILD count — VUID-03708 violation

- **Severity**: MEDIUM
- **Dimension**: Acceleration Structures
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:265-266, 390-396, 723-729`
- **Status**: NEW
- **Description**: The TLAS sizing query and allocation use `padded_count = max(2×, 8192)`, but the BUILD range is submitted with `primitive_count = instance_count` (e.g. 100). On subsequent frames where `instance_count = 150 < max_instances = 8192`, UPDATE fires with `primitive_count = 150`, violating the spec requirement that UPDATE primitive counts match the source BUILD counts.
- **Evidence**:
  ```rust
  // Size query uses padded_count (correct):
  accel_loader.get_acceleration_structure_build_sizes(..., &[padded_count as u32], ...);
  // But BUILD and UPDATE both use instance_count (wrong for BUILD):
  let range = vk::AccelerationStructureBuildRangeInfoKHR::default()
      .primitive_count(instance_count);  // should be padded_count for BUILD
  ```
- **Impact**: `VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03708` violation. On NVIDIA release this is undefined behaviour — RT shadows/reflections/GI may silently miss geometry. Window: any frame where instance count grows without crossing the resize threshold (common in streaming cell loads).
- **Suggested Fix**: Store `built_primitive_count = padded_count` in `TlasState` and fall back to BUILD when `instance_count > built_primitive_count`. Alternatively, submit BUILD range with `padded_count` (matching the sizing query assumption) and treat unused tail slots as inactive.

---

### REN-D10-003: SVGF denoised indirect bound with LINEAR sampler — unwanted spatial blur

- **Severity**: MEDIUM
- **Dimension**: Denoiser & Composite
- **Location**: `crates/renderer/src/vulkan/composite.rs:626-629`
- **Status**: NEW
- **Description**: `indirectTex` (binding 1, the SVGF denoised output) is bound with `hdr_sampler` (LINEAR filter). The composite shader samples it with `texture(indirectTex, fragUV)`, spreading each denoised pixel into its neighbours and adding spatial blur not part of the Schied 2017 SVGF model. The caustic binding (5) correctly uses a dedicated NEAREST sampler.
- **Evidence**: `composite.rs:626`: `write_image_sampler(... indirectTex ..., hdr_sampler)` where `hdr_sampler` has `mag_filter = LINEAR, min_filter = LINEAR`.
- **Impact**: Subtle but measurable: indirect lighting appears softer than intended, losing sharpness at contact shadows and on geometric detail. Most visible in high-contrast interior lighting.
- **Suggested Fix**: Create a `nearest_sampler` alongside `caustic_sampler`. Bind `indirectTex` with it in both `new_inner` and `recreate_on_resize`.

---

### REN-D16-001: Starfield `BSGeometry` tangents always `Vec::new()` — all SF meshes use screen-space derivative fallback

- **Severity**: MEDIUM
- **Dimension**: Tangent-Space & Normal Maps
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:155`
- **Status**: NEW
- **Description**: `extract_bs_geometry` hard-codes `tangents: Vec::new()` with a comment acknowledging it as a placeholder since #783. No follow-up issue was ever filed. The `BSGeometryMeshData` packed vertex stream contains tangent data but nothing routes it into `ImportedMesh::tangents`. Every Starfield mesh falls through to the screen-space derivative Path 2 in `perturbNormal`, which lacks UV-mirror handedness (see REN-D16-002).
- **Evidence**: `bs_geometry.rs:155`: `tangents: Vec::new(), // TODO: extract tangents`
- **Impact**: All Starfield normal maps use lower-quality screen-space derivatives. Mirrored UV shells show inverted normals (REN-D16-002 compounds this).
- **Suggested Fix**: Decode the tangent channel from `BSGeometryMeshData` and pass through `bs_tangents_zup_to_yup`, mirroring the `BsTriShape` path at `bs_tri_shape.rs:136`. File a tracking issue; this is a known deferral.

---

### REN-D18-002: Interior cells accumulate volumetric extinction with no fill light — ~63% scene darkening

- **Severity**: MEDIUM
- **Dimension**: Volumetrics (M55)
- **Location**: `byroredux/src/render.rs` (camera UBO assembly) + `crates/renderer/shaders/volumetrics_inject.comp`
- **Status**: NEW
- **Description**: `extinction_coef = scattering_coef = 0.005/m` is sent regardless of interior/exterior. Interior cells correctly zero `sun_color` (no inscatter), but the integration still accumulates transmittance `T_cum = exp(-extinction × 128 slices × slice_depth)`. For a cell 256 wu tall with 128 slices, `T_cum ≈ 0.368`. The composite formula `combined = combined * vol.a + vol.rgb` multiplies every interior pixel by ~0.37 with zero inscatter to compensate.
- **Impact**: When volumetrics is enabled in an interior cell, the entire scene darkens by ~63% with no light to fill. Functionally unacceptable for any interior render.
- **Suggested Fix**: In `draw.rs` camera UBO assembly, zero both scattering and extinction coefficients when the active cell is interior (check `CellKind::Interior`). Alternatively gate on `radius < 0` (same flag used for the shadow bypass).

---

### LOW

---

### REN-D3-001 / REN-D17-001 (deduplicated): Stale "112-byte / 112B" strings in water.rs after WaterPush grew to 128B

- **Severity**: LOW
- **Dimension**: Pipeline State / Water (M38)
- **Location**: `crates/renderer/src/vulkan/water.rs:102, 203`
- **Status**: NEW
- **Description**: WaterPush grew from 112B to 128B (Fix #1069). Two strings still say 112: the doc comment at line 102 (`"the 112-byte push-constant block"`) and the `log::info!` at line 203 (`"112B push constants"`). The compile-time assert and pipeline range are correct (128B).
- **Suggested Fix**: Change both to `"128B"` / `"128-byte"`.

---

### REN-D4-NEW-04: Stale 'bit 15' comments in svgf_temporal.comp and caustic_splat.comp post-#992 R32_UINT migration

- **Severity**: LOW
- **Dimension**: Render Pass & G-Buffer
- **Location**: `crates/renderer/shaders/caustic_splat.comp:121`, `crates/renderer/shaders/svgf_temporal.comp:80`
- **Status**: NEW
- **Description**: After #992 migrated `MESH_ID_FORMAT` to R32_UINT (bit 31 → ALPHA_BLEND flag, bit 15 retired), two narrative comment lines still say "bit 15". Runtime code is correct (`& 0x7FFFFFFFu`, `0x80000000u` tests). A future maintainer reading the stale line could write a broken read-mask (`& 0x7FFFu`) targeting a slot that now carries legitimate instance IDs in dense scenes.
- **Suggested Fix**: Replace "bit 15" with "bit 31" in each stale narrative line. Leave explicitly historical "Pre-#992 ... bit 15" lines in `caustic_splat.comp:132-133` untouched.

---

### REN-D8-002: No test for skip→add round-trip in `last_blas_addresses`

- **Severity**: LOW
- **Dimension**: Acceleration Structures
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:527-568`
- **Status**: NEW
- **Description**: The draw-command-skipped-due-to-missing-BLAS path (frame N) → BLAS built (frame N+1) is guarded by `debug_assert_eq!` but never exercised by a test. No correctness bug confirmed, but the invariant is unproven.
- **Suggested Fix**: Add a test in `acceleration/tests.rs` that simulates the skip→add→validate sequence.

---

### REN-D8-003: `build_blas_batched` leaks GPU resources on mid-loop `create_acceleration_structure` failure

- **Severity**: LOW
- **Dimension**: Acceleration Structures
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs:580-596`
- **Status**: NEW
- **Description**: If `create_acceleration_structure` fails on iteration `i > 0`, the `bail!` early return leaks `prepared[0..i-1]` GpuBuffer + VkAccelerationStructureKHR handles. The cleanup at lines 714-726 only runs on `submit_one_time` failure, not on the `bail!` path.
- **Suggested Fix**: Collect successfully prepared entries into a `Vec`; on `bail!`, iterate and destroy them before returning.

---

### REN-D9-001: `CameraUBO.skyTint.w` comment says "reserved" — carries `sun_angular_radius`

- **Severity**: LOW
- **Dimension**: RT Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag:169`
- **Status**: NEW
- **Description**: UBO struct comment says `w = reserved`, but `triangle.frag:2444` reads `skyTint.w` as `sunAngularRadius` for directional shadow jitter. Zeroing `skyTint.w` (a reasonable "cleanup" from a maintainer reading the comment) would collapse all directional shadows to a hard point-source cone.
- **Suggested Fix**: Update comment at line 169: `w = sun_angular_radius (rad; SkyParams::sun_angular_radius)`.

---

### REN-D10-001: `composite.frag:35` comment says `depth_params.z = unused` — it gates volumetrics

- **Severity**: LOW
- **Dimension**: Denoiser & Composite
- **Location**: `crates/renderer/shaders/composite.frag:35`
- **Status**: NEW
- **Description**: GLSL comment says `z/w = unused`, but `depth_params.z` is the `VOLUMETRIC_OUTPUT_CONSUMED` gate used at lines 420 and 489. Future zero-init of `depth_params.z` would silently disable volumetric lighting.
- **Suggested Fix**: Update comment: `z = volumetric_consumed (bool as float)`.

---

### REN-D10-002: SVGF bit-31 mesh-ID contract not cross-referenced between triangle.frag and svgf_temporal.comp

- **Severity**: LOW
- **Dimension**: Denoiser & Composite
- **Location**: `crates/renderer/shaders/svgf_temporal.comp:93, 143`
- **Status**: NEW
- **Description**: The full mesh-ID encoding contract (R32_UINT, bit 31 = ALPHA_BLEND_NO_HISTORY, bits 0-30 = instance ID) is documented in `triangle.frag:1027` but `svgf_temporal.comp` applies the masks without a cross-reference comment. A change to the encoding in one shader would not obviously need to propagate to the other.
- **Suggested Fix**: Add a one-line comment citing `triangle.frag::computeMeshId` and `gbuffer.rs::MESH_ID_FORMAT` at the mask sites in `svgf_temporal.comp`.

---

### REN-D11-001: SSAO and cluster-cull reconstruct positions from jittered `inv_view_proj`

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:175-211`, `draw.rs:396-484`
- **Status**: NEW
- **Description**: `GpuCamera.inv_view_proj` is the inverse of the jittered projection matrix. SSAO and `cluster_cull.comp` use it to reconstruct world/view-space positions from depth, shifting the reconstruction origin by up to ±0.5 px per frame. A `proj_unjittered` / `inv_view_proj_unjittered` field does not exist.
- **Impact**: TAA accumulates the per-frame jitter shift into AO and cluster bounds, adding sub-pixel noise to ambient occlusion and potentially misassigning lights near cluster boundaries.
- **Suggested Fix**: Add `proj_unjittered: [[f32;4];4]` (and its inverse) to `GpuCamera`; route SSAO and cluster shaders to the un-jittered inverse. Requires `GpuCamera` size change + lockstep shader sync per `feedback_shader_struct_sync`.

---

### REN-D11-002: Halton period is 8 but Halton(3) natural period is 9 — asymmetric Y coverage

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `byroredux/src/render.rs:396`
- **Status**: NEW
- **Description**: `frame_counter % 8` cycles indices 1–8; the 9th Halton(3) sample (≈0.889) is never reached, leaving an asymmetric gap in Y sub-pixel coverage. Base-2 axis is unaffected.
- **Suggested Fix**: Change `% 8` to `% 16` for a cleaner joint LCM period, or document as intentional in a comment.

---

### REN-D12-001: Overflow warn log prints wrong "already pushed" count

- **Severity**: LOW
- **Dimension**: GPU Skinning
- **Location**: `byroredux/src/render.rs:356-368`
- **Status**: NEW
- **Description**: The once-per-session `MAX_TOTAL_BONES` overflow warning logs `MAX_BONES_PER_MESH × skin_offsets.len()` as the "already pushed" count. This equals `bone_palette.len()` only when every mesh uses exactly `MAX_BONES_PER_MESH` bones — variable-stride packing (M29.5) breaks this coincidence. Diagnostic only; no correctness impact.
- **Suggested Fix**: Log `bone_palette.len()` directly as the actually-pushed count.

---

### REN-D12-002: Double AS-WRITE barrier per skinned BLAS refit after #983 moved it to callee

- **Severity**: LOW
- **Dimension**: GPU Skinning
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:863`, `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:555`
- **Status**: NEW
- **Description**: After #983 moved the scratch-serialize barrier inside `refit_skinned_blas` for self-enforcement, the caller-side emission in `draw.rs:863` was kept as "idempotent." Result: every refit records two back-to-back `AS_WRITE → AS_WRITE` barriers with no work between them.
- **Impact**: Correct but wasteful; clutters RenderDoc captures.
- **Suggested Fix**: Remove the caller-side barrier at `draw.rs:863`; callee covers all hazard cases.

---

### REN-D13-001: `avgAlbedo` read from legacy GpuInstance — inflates per-instance size for caustic-only use

- **Severity**: LOW
- **Dimension**: Caustics
- **Location**: `crates/renderer/shaders/caustic_splat.comp`
- **Status**: NEW (acknowledged R1 deferral)
- **Description**: `caustic_splat.comp` reads `avgAlbedo` from the per-instance `GpuInstance` struct rather than from `materials[materialId]` (the R1 path). This is an acknowledged deferral in the source code, adding 16 bytes to every instance for a field consumed only by caustics.
- **Suggested Fix**: Migrate `avgAlbedo` to `GpuMaterial` and read via `materials[instance.materialId]`. Coordinate with R1 Phase 6 closeout.

---

### REN-D13-002: Caustic fixed-point clamp `4.0e7` unanchored to `CAUSTIC_FIXED_SCALE`

- **Severity**: LOW
- **Dimension**: Caustics
- **Location**: `crates/renderer/shaders/caustic_splat.comp`
- **Status**: NEW
- **Description**: The upper clamp for fixed-point packing is a magic constant unanchored to `CAUSTIC_FIXED_SCALE`. A scale change requires manual update.
- **Suggested Fix**: Derive the clamp from `CAUSTIC_FIXED_SCALE`: `clamp_max = float(0xFFFFFFFFu) / CAUSTIC_FIXED_SCALE`.

---

### REN-D13-003: Caustic `initialize_layouts` uses deprecated `TOP_OF_PIPE` source stage

- **Severity**: LOW
- **Dimension**: Caustics
- **Location**: `crates/renderer/src/vulkan/caustic.rs` (`initialize_layouts`)
- **Status**: NEW
- **Description**: Same pattern as open issue #949 for `gbuffer::initialize_layouts`. The `TOP_OF_PIPE` source stage on an UNDEFINED→GENERAL transition is technically correct but deprecated and generates validation noise on some drivers. Should use `NONE` (Vulkan 1.3) or `HOST` per the Khronos migration guide.
- **Suggested Fix**: Change source stage to `NONE` (mirrors the fix for #949).

---

### REN-D15-001: `wind_speed` not promoted when WTHR cross-fade completes

- **Severity**: LOW
- **Dimension**: Sky/Weather
- **Location**: `byroredux/src/systems/weather.rs:578-587`
- **Status**: NEW
- **Description**: The `transition_done` promotion block copies `sky_colors`, `fog`, and `tod_hours` from the target `WeatherDataRes` into the live one but omits `wind_speed`. After a storm↔calm transition completes, cloud scroll uses the source weather's wind speed indefinitely.
- **Suggested Fix**: Add `wd.wind_speed = tr.target.wind_speed;` to the promotion block.

---

### REN-D15-002: `skyrim_dalc_per_tod` not promoted on WTHR cross-fade completion

- **Severity**: LOW
- **Dimension**: Sky/Weather
- **Location**: `byroredux/src/systems/weather.rs:578-587`
- **Status**: NEW
- **Description**: Same block as REN-D15-001. The Skyrim DALC ambient cube is not promoted after cross-fade, leaving the 6-direction ambient cube frozen at source weather values while sky/fog switches to target.
- **Suggested Fix**: Add `wd.skyrim_dalc_per_tod = tr.target.skyrim_dalc_per_tod.clone();` alongside REN-D15-001's fix.

---

### REN-D15-003: `transition_done` ordering invariant undocumented — fragile under system split

- **Severity**: LOW
- **Dimension**: Sky/Weather
- **Location**: `byroredux/src/systems/weather.rs:578-587`
- **Status**: NEW
- **Description**: The correct behavior on the completion frame depends on `lerp(src, tgt, 1.0) = tgt` computing before promotion in the same system invocation. This invariant is not documented; splitting `weather_system` into timer-advance + blend-apply passes would silently break it.
- **Suggested Fix**: Add a comment explaining the same-invocation ordering requirement.

---

### REN-D16-002: Path-2 screen-space derivative bitangent ignores UV-mirror handedness

- **Severity**: LOW
- **Dimension**: Tangent-Space & Normal Maps
- **Location**: `crates/renderer/shaders/triangle.frag:731`
- **Status**: NEW
- **Description**: The Path-2 fallback in `perturbNormal` computes `B = cross(N, T)` with no sign factor. Path-1 uses `B = vertexTangent.w * cross(N, T)`. On UV-mirrored geometry, bitangent is always right-handed regardless of authoring, inverting tangent-space normals on mirrored shells.
- **Suggested Fix**: `float screenSign = sign(dUVdx.x * dUVdy.y - dUVdx.y * dUVdy.x); B = screenSign * cross(N, T);`

---

### REN-D18-003: Volumetric inject binding 2 (TLAS) never written at construction

- **Severity**: LOW
- **Dimension**: Volumetrics (M55)
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs`
- **Status**: NEW
- **Description**: Injection descriptor set binding 2 (TLAS) is written only after `write_tlas` is called; construction only writes bindings 0 and 1. Validation layers report "descriptor not updated" in debug. No GPU correctness hazard since dispatch is gated.
- **Suggested Fix**: Document the deferred-write pattern and add `debug_assert!(self.tlas_written)` at the top of `dispatch()`.

---

### REN-D18-004: `volumetrics_integrate.comp` uses `#version 450`, partner uses `#version 460`

- **Severity**: LOW
- **Dimension**: Volumetrics (M55)
- **Location**: `crates/renderer/shaders/volumetrics_integrate.comp:1`
- **Status**: NEW
- **Description**: Version mismatch between `volumetrics_inject.comp` (#version 460) and `volumetrics_integrate.comp` (#version 450). Functionally benign but creates maintenance confusion.
- **Suggested Fix**: Bump `volumetrics_integrate.comp` to `#version 460`.

---

### REN-D19-002: Comment at draw.rs:2241 says "un-TAA'd scene HDR" — bloom reads post-TAA output

- **Severity**: LOW
- **Dimension**: Bloom (M58)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2241`
- **Status**: NEW
- **Description**: Comment says bloom reads "un-TAA'd scene HDR" but TAA resolves into that buffer first; bloom reads post-TAA output.
- **Suggested Fix**: Update comment to "post-TAA resolved HDR".

---

### REN-D20-001: TAA YCoCg gamma=1.25 too tight for 4×-physical-sun cone penumbra noise

- **Severity**: LOW
- **Dimension**: M-LIGHT Soft Shadows
- **Location**: `crates/renderer/shaders/taa.comp:179`
- **Status**: NEW
- **Description**: With `sunAngularRadius = 0.020 rad` (4× physical), penumbra-edge shadow noise has higher per-frame variance than the YCoCg gamma=1.25 clamp tolerates. Valid history samples near penumbra edges get rejected under camera motion, causing residual flicker.
- **Suggested Fix**: Widen gamma to 1.5, or make it proportional to `sunAngularRadius` (e.g. `1.25 + 10.0 * sunAngularRadius`).

---

### REN-D20-002: No host-side guard on `sun_angular_radius` above tangent-plane approximation threshold

- **Severity**: LOW
- **Dimension**: M-LIGHT Soft Shadows
- **Location**: `byroredux/src/render.rs` (`SkyParamsRes::sun_angular_radius` assembly)
- **Status**: NEW
- **Description**: The shader comments at `triangle.frag:2418-2425` note the disk-to-sphere approximation is only valid for α < ~0.05 rad. No host-side `debug_assert` or clamp enforces this. A per-cell override above 0.1 rad would produce visibly biased penumbras silently.
- **Suggested Fix**: Add `debug_assert!(sun_angular_radius < 0.10, "sun cone approximation invalid above 0.10 rad")` at the `SkyParamsRes` construction site.

---

## Prioritized Fix Order

### Correctness + Safety (fix before next Vulkan submission)
1. **REN-D19-001** (HIGH) — Bloom fallback dummy binding: device-loss risk on strict drivers
2. **REN-D18-001** (HIGH) — Volumetric clear on init: first-enable scene-black + misleading doc
3. **REN-D8-001** (MEDIUM) — TLAS BUILD primitive_count: spec VUID violation, silent RT misses
4. **REN-D18-002** (MEDIUM) — Interior volumetric extinction: unacceptable interior darkening

### Quality
5. **REN-D10-003** (MEDIUM) — SVGF indirect NEAREST sampler: unintended spatial blur on indirect
6. **REN-D16-001** (MEDIUM) — Starfield BSGeometry tangents: all SF meshes degrade to screen-space derivatives
7. **REN-D11-001** (LOW) — SSAO/cluster jittered inv_view_proj: sub-pixel AO and cluster noise

### Documentation (cheap, high value)
8. REN-D3-001 / D17-001 — stale "112B" in water.rs
9. REN-D4-NEW-04 — stale "bit 15" in SVGF/caustic shaders
10. REN-D9-001 — stale `skyTint.w = reserved` comment
11. REN-D10-001 — stale `depth_params.z = unused` comment
12. REN-D15-001/002 — WTHR cross-fade missing wind_speed + DALC promotion (1-line each)
13. REN-D18-003/004 — volumetrics descriptor gap + version mismatch

### Cleanup (low risk, low urgency)
14. REN-D12-002 — duplicate AS-WRITE barrier in skinned refit loop
15. REN-D8-002 — missing test for BLAS skip→add round-trip
16. REN-D8-003 — mid-loop BLAS creation failure leaks handles
17. REN-D11-002 — Halton period 8 vs natural period 9
18. REN-D13-001/002/003 — caustic avgAlbedo R1 deferral, clamp constant, TOP_OF_PIPE
19. REN-D16-002 — Path-2 bitangent UV-mirror handedness (compounds REN-D16-001)
20. REN-D20-001/002 — TAA gamma tightness + sun_angular_radius guard
21. REN-D12-001 — wrong "already pushed" count in overflow warn
22. REN-D15-003 — transition_done ordering comment
23. REN-D19-002 — bloom "un-TAA'd" comment
24. REN-D10-002 — SVGF mesh-ID contract missing cross-reference

---

## Previously Confirmed Fixed (this audit)

- **Prior DIM4 REN-D4-NEW-01** (EARLY_FRAGMENT_TESTS in outgoing subpass dep) — confirmed fixed in `helpers.rs:162-166`. Recommend closing the associated issue.
- **Prior DIM15 findings** (fog breakpoint, cloud velocity duplication, sun-arc gate) — all confirmed fixed.
- **Prior DIM8 LOWs** (`instance_custom_index` 24-bit guard, `UPDATABLE_AS_FLAGS`, `evict_unused_blas` const-assert, `refit_skinned_blas` self-barrier) — all confirmed fixed.
