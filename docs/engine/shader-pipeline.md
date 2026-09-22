# Shader Pipeline

The ByroRedux renderer is a **deferred + ray-traced** Vulkan pipeline built on
Vulkan 1.3 with ray-query extensions. Every frame visits a fixed set of passes in
strict submission order. This document is the reference for the per-pass
responsibilities, GPU data layouts, and descriptor bindings. For the high-level
renderer architecture (BLAS/TLAS, sync, swapchain, teardown ordering) see
[renderer.md](renderer.md).

> **Contract checkpoint (2026-08-15).** Reconciled with the RT
> lighting/material recovery: 1023-light SSBO, 512-light cluster lists,
> ReSTIR-selected visibility, generated material flags, scale-aware secondary
> origins, and raw correctness output (including direct/indirect term
> isolation) through composite/upscale/presentation. Zero lights no longer
> synthesize a renderer-owned fallback sun.

---

## Shader Files

### Raster

| File | Role |
|------|------|
| `triangle.vert` | Main geometry vertex shader — model transform, skinned-vertex read, motion-vector output, tangent-space setup, terrain splat-weight passthrough |
| `triangle.frag` | Main PBR fragment shader — Disney BSDF, RT ray-query shadows / reflections / bounded material-aware path-traced GI, glass RT refraction, terrain splatting, terrain blend |
| `water.vert` | Water quad vertex — flat local-space mesh (no per-frame BLAS rebuild) |
| `water.frag` | Water surface — RT reflection/refraction, Fresnel mix, caustic accumulator `imageAtomicAdd`, shoreline foam RT ray |
| `ui.vert` | UI quad passthrough — position already in NDC [-1, 1] |
| `ui.frag` | UI bindless texture sampling — no shading, straight texel output |
| `composite.vert` | Fullscreen triangle via `gl_VertexIndex` — no vertex buffer; reused unmodified as `presentation.frag`'s vertex stage |
| `composite.frag` | HDR compose — direct + SVGF-denoised indirect + dual caustic accumulator (glass/water), bloom add, volumetric froxel sample. Emits linear HDR to an intermediate image (no tone-map, no swapchain write — see `presentation.frag`) |
| `presentation.frag` | FSR 3.1 presentation pass — samples the upscaled (or native-blit-fallback) scene, applies `tonemap(graded * exposureTex)` (the exposure meter's per-FIF texel; ACES\|AgX display-transform switch) and underwater extinction, writes the swapchain (`PRESENT_SRC_KHR`) |

### Compute

| File | Role |
|------|------|
| `skin_palette.comp` | Build per-slot bone-matrix palette from world transforms + bind inverses |
| `skin_vertices.comp` | Deform skinned vertex **positions only** (`SKIN_OUTPUT_STRIDE_FLOATS` = 3 since #2170 — the skinned normal/tangent writes were dropped with their unread consumers); output drives per-entity BLAS refit |
| `cluster_cull.comp` | Build per-froxel light lists (clustered shading) |
| `ssao.comp` | Screen-space ambient occlusion texture generation |
| `svgf_temporal.comp` | Temporal denoiser — motion-vector reprojection + color/moments accumulation for indirect lighting |
| `svgf_atrous.comp` | Spatial denoiser — edge-stopping à-trous wavelet filter, `ATROUS_ITERATIONS` = 3 ping-pong passes after the temporal dispatch; final slot feeds composite (Dugout ablation capped the footprint at 14 render pixels) |
| `taa.comp` | TAA resolve — Halton(2,3) jitter, YCoCg variance-clamp, history reproject |
| `bloom_downsample.comp` | Gaussian + downsample pyramid (bright content) |
| `bloom_upsample.comp` | Upsample + blur stages of bloom pyramid |
| `exposure_meter.comp` | Stage-1 auto-exposure meter — EV100 average of the post-bloom scene, per-FIF-slot exponential adaptation, writes the 1×1 exposure texel FSR and presentation sample (`exposure_meter.rs`; fixed mode writes the authored constant) |
| `caustic_splat.comp` | Per-refractive-surface scatter of refracted-light contributions into caustic accumulator |
| `volumetrics_inject.comp` | Inject sun-light into froxel grid (HG-phase scattered radiance) |
| `volumetrics_integrate.comp` | Integrate transmittance over froxel grid |

All SPIR-V binaries are pre-compiled and embedded via `include_bytes!` in
[`crates/renderer/src/vulkan/pipeline.rs`](../../crates/renderer/src/vulkan/pipeline.rs)
and `compute.rs`. **All GLSL edits require a recompile** (see
[`crates/renderer/shaders/`](../../crates/renderer/shaders/) for the build script).

Every committed `.spv` targets **SPIR-V 1.0** — the version `glslangValidator -V`
emits by default with no `--target-env` flag (CLAUDE.md's documented recompile
command). Don't add `--target-env` to bump an individual shader to a newer
SPIR-V version; a mismatched version stamp across the shader set breaks the
"the documented command reproduces every binary" invariant (#1929 / REN-D11-01
— `triangle.vert.spv` had drifted to 1.5). `triangle.frag.spv`'s ray queries
running under a 1.0 stamp is tolerated by the current driver; bumping *that*
shader's version to formally match its capabilities is a separate, deferred
question needing RenderDoc/driver verification, not a "just recompile" fix.

---

## Per-Frame Submission Order

All passes record into a single command buffer and are submitted to one
graphics+compute queue. Pass ordering is `draw_frame` in
[`vulkan/context/draw.rs`](../../crates/renderer/src/vulkan/context/draw.rs),
but since the #3282 split most phases — and, of the barriers below, 4b and 5b —
are recorded in its sibling files (`sync_and_acquire_frame.rs`,
`dispatch_skin_and_cluster.rs`, `build_and_upload_instances.rs`,
`geometry_pass.rs`, `post_passes.rs`). Each row names the function that records
it where that is not `draw_frame` itself.

```
1a flush_pending_morph  ─  host write of the morph-weight buffer through its
   _weights                 persistent mapping. Runs immediately BEFORE step 1,
   [host, no cmds]          in `sync_and_acquire_frame` — after the fence wait
                           that covers BOTH slots, which is what makes the
                           write safe (`sync.rs`'s #870 block lists it as item
                           5 on that wait's dependency list). Its visibility to
                           the shaders comes from step 5b's bulk barrier, not
                           from a barrier of its own.
1  collect_image_health  ─  CPU readback (#2740 / REN-D4-04): harvest this
   [host, no cmds]          frame-in-flight slot's image-health counters
                           from its PRIOR use (MAX_FRAMES_IN_FLIGHT == 2
                           frames ago), then reset them for reuse. Runs
                           once, right after the per-slot fence wait, before
                           any command below is recorded. The counters are
                           WRITTEN continuously by step 20 (presentation.frag's
                           isnan/isinf check) during that prior frame — there
                           is no separate GPU "write" step of its own.
2  skin_palette.comp    ─┐ compute
3  skin_vertices.comp   ─┘ skinned BLAS input ready
4  AccelerationManager   ─  BLAS rebuild / refit + TLAS build
4b [Barrier]            ─  ACCELERATION_STRUCTURE_BUILD_KHR /
                           ACCELERATION_STRUCTURE_WRITE_KHR → FRAGMENT_SHADER |
                           COMPUTE_SHADER / ACCELERATION_STRUCTURE_READ_KHR.
                           The frame's ONLY AS build → shader-read edge, and
                           `/audit-severity`'s HIGH-floor barrier. Publishes
                           step 4 in full: the TLAS build AND every
                           skinned-BLAS refit (`record_skinned_blas_refit`,
                           which runs just above it). Emitted on both arms —
                           a failed TLAS build deliberately keeps the previous
                           AS alive (#2673), so volumetrics still ray-queries
                           from COMPUTE against skinned BLAS whose refit writes
                           would otherwise never be made visible (#415 / #2931).
5  cluster_cull.comp     ─  per-froxel light lists (cluster grid +
                           light-index list); consumed by both the
                           triangle.frag fragment shader AND
                           volumetrics_inject (same per-frame buffers,
                           #977eb95a)
5a sky_cube.comp        ─  SKYAL sky bake (`SkyCubePipeline::record_bake`, in
   sky_prefilter.comp       `build_and_upload_instances`, only when the
   sky_irradiance.comp      pipeline exists): bakes this slot's cubemap from
                           the same parameters composite draws its background
                           with, then GENERAL → SHADER_READ_ONLY_OPTIMAL,
                           then the GGX prefilter mips and the SH irradiance
                           projection. Owns its own layout transitions.
                           Recorded before step 6 because the main pass
                           samples it (Set 1 bindings 20/21); its
                           `exteriorSkyTint.w` ready flag is what makes the
                           no-bake case safe to skip.
5b [Barrier]            ─  HOST / HOST_WRITE → VERTEX_SHADER |
                           FRAGMENT_SHADER | COMPUTE_SHADER | DRAW_INDIRECT /
                           SHADER_READ | SHADER_WRITE | UNIFORM_READ |
                           INDIRECT_COMMAND_READ, at the tail of
                           `build_and_upload_instances`. The single publication
                           point for every host write this frame: the instance
                           SSBO plus the composite, SVGF, TAA and water param
                           UBOs, which are uploaded ABOVE it specifically so
                           they fold onto this one dependency (#909, #961,
                           #1397). That fold is why the later passes — step 16
                           composite most visibly — carry no HOST barrier of
                           their own despite consuming host-written UBOs.
                           Defense-in-depth (#4182), not a spec requirement:
                           host writes flushed before `queue_submit` are
                           already visible (Vulkan 1.3 §7.9 host-write
                           ordering), and every write here is a mapped write
                           made earlier in `draw_frame`. The barrier guards a
                           future non-coherent memory type or a
                           post-recording host write.
6  [Main render pass]   ─  raster (BEGIN → END):
     triangle.vert / .frag  geometry + RT ray-queries
     water.vert / .frag     water + caustic imageAtomicAdd
6b [Barrier]            ─  FRAGMENT_SHADER / SHADER_WRITE → HOST / HOST_READ,
                           publishing the bounded selected-ray probe record
                           `triangle.frag` wrote during step 6. The matching
                           CPU read happens two frames later, at this slot's
                           next fence wait, and invalidates first. Masks are
                           source-pinned by `selected_ray_probe_is_bounded_
                           and_captures_the_detailed_shadow_query`
                           (`scene_buffer/shader_contract_tests.rs`).
7  copy_depth_to_history ─  [TRANSFER] **conditional on `has_effect_soft_
   (conditional, #3667)      material`** — most frames of most cells skip
                           this entirely (#4032). When it runs: snapshot
                           this frame's opaque depth into the sampleable
                           depth-history image, for next frame's soft-
                           particle fade. Two depth-image layout
                           transitions (READ_ONLY → TRANSFER_SRC → READ_ONLY
                           restored after the copy); history image mirrors
                           SHADER_READ_ONLY → TRANSFER_DST → SHADER_READ_ONLY.
7b depth_capture_record_  ─  [TRANSFER] (#3308) recorded immediately after
   copy                      step 7's slot, unconditionally. The depth
                           image's DEPTH_STENCIL_READ_ONLY_OPTIMAL
                           precondition comes from the render pass's own
                           depth-attachment `final_layout`, not from step 7
                           (#4032, correcting the prior claim here) — step 7,
                           when it runs, only *restores* that layout after
                           its own transfer. Two more depth-image layout
                           transitions around a `cmd_copy_image_to_buffer`
                           (READ_ONLY → TRANSFER_SRC → READ_ONLY), independent
                           of step 7's own copy. Not a no-op: it moves the
                           depth image twice more before every later depth
                           consumer (SSAO, SVGF, composite, FSR).
8  [Barrier]               SHADER_READ_ONLY_OPTIMAL on all G-buffer attachments
9  [Barrier]               caustic accum atomic-add → SHADER_READ
10 svgf_temporal.comp   ─  temporal denoiser (indirect lighting)
11 svgf_atrous.comp ×3  ─  à-trous spatial denoiser (ATROUS_ITERATIONS),
   [COMPUTE→COMPUTE]        ping-pong slots gated each iteration by a
                           COMPUTE→COMPUTE barrier; final (odd count → slot 0)
                           is what composite samples via indirect_view(frame)
12 caustic_splat.comp   ─  caustic scatter
13 volumetrics_inject   ─┐ froxel grid (output consumed by composite,
                           VOLUMETRIC_OUTPUT_CONSUMED = true); reads
                           cluster_cull's cluster grid + light-index list
                           from step 5
14 volumetrics_integrate ─┘
15 ssao.comp             ─  SSAO texture
16 [Composite render pass]─ raster:
     composite.vert / .frag  HDR combine → intermediate HDR image
                           (`R16G16B16A16_SFLOAT`, `SHADER_READ_ONLY_OPTIMAL`;
                           no tone-map, does NOT write the swapchain; does
                           NOT add bloom — see step 17, #2796)
17 bloom_downsample ×N   ─┐ bloom pyramid, runs AFTER composite
   bloom_upsample   ×N    │ (`record_bloom_pass`, ordered after
   bloom_apply.comp      ─┘ `record_composite_pass` and before
                           `record_taa_pass`/`record_upscale_pass`);
                           `bloom_apply.comp` reads composite's HDR output
                           back as a storage image and adds `up_mips[0]` in
                           place (`BLOOM_INTENSITY = 0.15`)
17b exposure_meter.comp  ─  auto-exposure meter (Stage 1,
                           `record_exposure_meter_pass`): averages the
                           post-bloom scene (EV100 = log2(L·8)), adapts
                           the per-FIF 1×1 exposure texel toward the target
                           (alpha over the slot's full N-frame update
                           interval, #4590), writes the texel FSR and
                           presentation read. Fixed mode (the default)
                           writes the authored constant; raw debug views
                           skip the dispatch (#4591).
18 taa.comp              ─  TAA resolve — runs AFTER composite + bloom
                           (#3572): it resolves the SAME fully-composited,
                           post-bloom scene image the upscale consumes, so
                           sky / denoised indirect / volumetrics / caustics /
                           bloom are inside the filter on both jitter phases
                           (the pre-#3572 raw-HDR tap resolved direct lighting
                           only). Composite reads the raw HDR attachment
                           directly; TAA's output feeds the
                           upscale/presentation tap below.
19 frame_upscaler.record  ─  FSR 3.1 SDK dispatch (Quality preset default) or
                           native-blit fallback (`--upscaler taa`) — render-
                           resolution HDR → output-resolution HDR. Raw
                           correctness debug views force the native path.
20 [Presentation pass]    ─  raster: composite.vert / presentation.frag —
                           `tonemap(graded * exposureTex)` (the meter's
                           exposure; ACES|AgX via `tonemap.rs`), underwater
                           extinction,
                           writes the swapchain (`PRESENT_SRC_KHR`); then,
                           in the same subpass, `PresentationPipeline::
                           record_overlay` draws the Scaleform/Ruffle UI
                           quad (`ui.vert` / `ui.frag`) on top (#3426 — moved
                           here from the tail of the main render pass, so
                           the overlay is composited after tone-map/upscale
                           and never temporally reconstructed). Also
                           where step 1's image-health counters get WRITTEN:
                           an isnan/isinf check on the pre-tonemap linear HDR
                           value, atomicAdd'd into the `ImageHealth` SSBO this
                           same frame-in-flight slot's step 1 will harvest,
                           two frames from now. Raw debug views bypass the
                           look transforms but not this check.
21 [Egui render pass]    ─  egui overlay (blended on swapchain)
22 [Screenshot copy]     ─  transfer blit → staging buffer (if requested)
23 Queue submit
24 Present
```

Steps 19–20 are the FSR 3.1 tail added 2026-07-22→24 (`crates/fsr3-sys`,
`vulkan/frame_upscaler.rs`, `vulkan/presentation.rs`, `vulkan/exposure.rs`,
plus Stage 1's `vulkan/exposure_meter.rs` + `exposure_meter.comp`, the
per-frame meter between bloom and TAA);
the split moves ACES tone-mapping out of `composite.frag` (which now emits
un-tonemapped linear HDR) and into `presentation.frag`, which runs at output
resolution after the upscale so tone-mapping sees full-resolution detail.

---

## G-Buffer Layout

Eight colour attachments + depth, all double-buffered (one set per
`MAX_FRAMES_IN_FLIGHT` = 2). Written by the main render pass
(`triangle.frag` + `water.frag`), read by SVGF, TAA, SSAO, composite, and
(the two FSR mask attachments) `frame_upscaler`'s FSR 3.1 SDK dispatch.

| Attachment | `VkFormat` | Contents | Layout during pass |
|---|---|---|---|
| HDR colour | `R16G16B16A16_SFLOAT` | Direct lighting (pre-denoised); alpha feeds SRC_ALPHA blend + water | `COLOR_ATTACHMENT_OPTIMAL` |
| Normal | `R16G16_SNORM` | Octahedral-encoded world normal | `COLOR_ATTACHMENT_OPTIMAL` |
| Motion | `R16G16_SFLOAT` | Screen-space motion vector (current → previous NDC) | `COLOR_ATTACHMENT_OPTIMAL` |
| Mesh ID | `R32_UINT` | Bits 0–30: **opaque** = stable `GpuInstance.surface_id`; **alpha-blended** = sorted instance index + 1. Bit 31: `ALPHA_BLEND_NO_HISTORY` (skip SVGF accumulation) | `COLOR_ATTACHMENT_OPTIMAL` |
| Raw indirect | `B10G11R11_UFLOAT_PACK32` | Albedo-demodulated indirect light (SVGF input) | `COLOR_ATTACHMENT_OPTIMAL` |
| Albedo | `B10G11R11_UFLOAT_PACK32` | Surface colour (diffuse × vertex colour) | `COLOR_ATTACHMENT_OPTIMAL` |
| Reactive | `R8_UNORM` | FSR 3.1 reactive mask (transparent coverage) | `COLOR_ATTACHMENT_OPTIMAL` |
| Transparency | `R8_UNORM` | FSR 3.1 transparency & composition mask | `COLOR_ATTACHMENT_OPTIMAL` |
| Depth | `D32_SFLOAT` | Standard depth (0.0 = near, 1.0 = far), `LESS_OR_EQUAL`, clear = 1.0 | `DEPTH_STENCIL_ATTACHMENT_OPTIMAL` |

After `vkCmdEndRenderPass` all eight **colour** attachments transition to
`SHADER_READ_ONLY_OPTIMAL`; **depth** transitions to
`DEPTH_STENCIL_READ_ONLY_OPTIMAL`. That distinction is load-bearing, not
pedantry: `copy_depth_to_history` names the depth layout as its precondition
three paragraphs below, and `depth_capture_record_copy` documents the same one
as its own contract (#3628). This sentence claimed a single layout for every
attachment until #4005, contradicting both — the code was right.

> **Why Mesh ID carries two representations.** `fragInstanceIndex` follows the
> per-frame *sorted* draw order, so an actor changing depth bucket used to make
> static architecture look like a different surface and reset TAA + SVGF across
> the room. Opaque draws therefore write the ECS-derived `surface_id`, which is
> stable across frames. Alpha-blended draws bypass both histories anyway (bit
> 31), so they keep the sorted index in the low bits — `caustic_splat.comp`
> consumes it to index the current-frame instance SSBO. `0` remains the
> clear/background value. See `triangle.frag`'s `stableSurfaceId` block.

> **`depth_history_image` isn't one of the nine attachments above.** It's a
> separate, single (not per-FIF-double-buffered) `D32_SFLOAT` image, same
> extent as Depth, `SAMPLED | TRANSFER_DST`. It isn't written by the render
> pass at all — step 7 (`copy_depth_to_history`) populates it via an
> explicit `vkCmdCopyImage` right after the render pass ends, so next
> frame's effect-shader soft-particle fade can sample the *previous*
> frame's opaque depth while this frame's own Depth attachment is bound
> and unsampleable. See `copy_depth_to_history`'s doc comment
> (`vulkan/context/post_passes.rs`).

---

## GPU Data Types

### `GpuCamera` — 368 bytes, uniform buffer (Set 1, Binding 1)

[`gpu_types.rs`](../../crates/renderer/src/vulkan/scene_buffer/gpu_types.rs)

| Offset | Size | Field | Contents |
|---|---|---|---|
| 0 | 64 | `view_proj` | `mat4` — combined view-projection |
| 64 | 64 | `prev_view_proj` | `mat4` — previous frame's view-projection (motion vectors) |
| 128 | 64 | `inv_view_proj` | `mat4` — precomputed `inverse(viewProj)` |
| 192 | 16 | `position` | xyz = world position; w = `frame_counter & 0xFF_FFFF` |
| 208 | 16 | `flags` | x = RT enabled (1.0); yzw = ambient RGB |
| 224 | 16 | `screen` | x = width; y = height; z = fog_near; w = fog_far |
| 240 | 16 | `fog` | xyz = fog colour; w = fog enabled (1.0) |
| 256 | 16 | `jitter` | xy = TAA Halton jitter (NDC); z = debug flags (bitcast f32); w = is_exterior |
| 272 | 16 | `sky_tint` | xyz = TOD/weather zenith colour; w = sun angular radius (rad) |
| 288 | 16 | `sun_direction` | xyz = direction **to** sun (unit); w = sun intensity |
| 304 | 16 | `dof_params` | x = aperture half-radius; y = focus distance; z = `light_atten_knee` (ambient-cull falloff knee); w = `camera_static` flag (1.0 = parked, gates GI reprojection) |
| 320 | 16 | `render_origin` | xyz = camera-relative render origin (#markarth-precision); **w = FSR one-frame history-reset flag** (1.0 = reset pending), read by `triangle.frag`'s FSR-temporal debug view (#2164). Not a free slot — same trap as `VolumetricsParams.render_origin.w` (#1928) |
| 336 | 16 | `render_debug` | x = `RenderDebugMode` shader discriminant; y = optional `f32::to_bits` RT-LOD scale; z = diagnostic LOD-counter enable; **w = packed weather surface** (low 16 bits rain wetness, high 16 bits snow coverage), written by `pack_weather_surface` and decoded by `triangle.frag`. Not a free slot — same trap as the `render_origin.w` row above (#3989); the `DBG_*` mask is already 32/32 bits, so the next debug-flag expansion is exactly the change that must not take this lane |
| 352 | 16 | `exterior_sky_tint` | xyz = the live exterior's sky zenith colour (#3323), read only by `triangle.frag`'s window-portal escape branch so a ray leaving an interior cell sees the real outdoor sky instead of a frozen noon-blue default; falls back to `SkyParams::default().zenith_color` when no exterior has loaded this session. **w = sky-cubemap ready flag** (1.0 when `SkyCubePipeline` exists, else 0.0), written in `assemble_camera_and_lights.rs` and gating every `skyCube` read (`bindings.glsl`, `lighting.glsl`, `triangle.frag`): set 1 / binding 20 is `PARTIALLY_BOUND` and never written when the bake fails to initialise, so reading it without this gate samples undefined data. Not a free slot — same trap as the `render_origin.w` / `render_debug.w` rows above (#4299) |

### `GpuWaterParams` — 368 bytes, SSBO (Set 2, Binding 1)

One std430 record per active water draw. The buffer starts at a small initial
capacity and grows geometrically; the retired 186-entry / ~64 KiB uniform-buffer
cap and its 64-byte headroom no longer exist. Rust, `water.vert`, and
`water.frag` are field-order checked by
`gpu_water_params_rust_and_glsl_copies_stay_in_lockstep`, which also pins the
unsized SSBO declaration in both shader stages.

| Offset | Size | Field | Contents |
|---|---|---|---|
| 0 | 16 | `timing` | time, `WaterKind`, foam strength, IOR |
| 16 | 16 | `flow` | xyz direction, w speed |
| 32 | 16 | `shallow` | shallow RGB, fog-near |
| 48 | 16 | `deep` | deep RGB, fog-far |
| 64 | 16 | `scroll` | layer A/B scroll vectors |
| 80 | 16 | `scroll_c` | layer C scroll, underwater fog near/far |
| 96 | 16 | `tune` | layer A/B scale, shoreline width, wave amplitude |
| 112 | 16 | `misc` | Fresnel F0, wave frequency, normal-map index bits, sun power |
| 128 | 16 | `tint_reflect` | reflection RGB and reflectivity |
| 144 | 16 | `noise_indices` | three noise indices and opacity bits (`uvec4`) |
| 160 | 16 | `detail` | layer C scale and three amplitude scales |
| 176 | 16 | `noise_falloff` | noise distance, blend gate, roughness, specular radius |
| 192 | 16 | `normal_falloff` | three normal falloffs and packed rain controls |
| 208 | 16 | `displacement` | displacement size/falloff/dampener and ripple size |
| 224 | 16 | `depth` | reflection/refraction/normal/specular depth weights |
| 240 | 16 | `effects` | refraction, local specular, reflection, sun-specular controls |
| 256 | 16 | `absorption` | Starfield extinction RGB and rain response |
| 272 | 16 | `concentration` | Starfield pigment/oceanness concentrations |
| 288 | 16 | `ripple` | world-XZ center, intensity, radius |
| 304 | 16 | `underwater` | underwater RGB and fog amount |
| 320 | 16 | `alpha` | shallow/deep alpha and distance thresholds |
| 336 | 16 | `uv_offset` | mesh UV offset, flow-map index bits, tile scale |
| 352 | 16 | `optical` | x = Creation-era depth amount; yzw reserved |

### `GpuInstance` — 160 bytes, SSBO (Set 1, Binding 4)

One entry per draw call (up to `MAX_INSTANCES` = 262 144).

| Offset | Size | Field | Contents |
|---|---|---|---|
| 0 | 64 | `model` | `mat4` — model-to-world |
| 64 | 4 | `texture_index` | Bindless albedo/diffuse texture index |
| 68 | 4 | `bone_offset` | Base slot in bone palette (0 for rigid) |
| 72 | 4 | `vertex_offset` | Offset into global vertex SSBO (in vertices) |
| 76 | 4 | `index_offset` | Offset into global index SSBO (in indices) |
| 80 | 4 | `vertex_count` | Vertex count (bounds checking) |
| 84 | 4 | `flags` | Bit-packed flags + terrain tile slot (bits 16–31) — see below |
| 88 | 4 | `material_id` | Index into per-frame `MaterialBuffer` SSBO |
| 92 | 4 | `ior` | Per-draw optical IOR — repurposed padding slot; consumed only by `caustic_splat.comp` |
| 96 | 4 | `avg_albedo_r` | Pre-computed average albedo R |
| 100 | 4 | `avg_albedo_g` | Pre-computed average albedo G |
| 104 | 4 | `avg_albedo_b` | Pre-computed average albedo B |
| 108 | 4 | `surface_id` | Stable ECS-derived surface identity — written to the Mesh ID attachment by opaque draws so TAA/SVGF history survives draw-order changes |
| 112 | 8 | `skinned_vertex_address` | GPU address (`uint64_t`) of this entity's skinned-vertex output buffer, `0` for rigid instances — #2219, dereferenced via `GL_EXT_buffer_reference` for deformed-pose RT hit-normal reconstruction |
| 120 | 8 | `_reserved` | Padding — no live data |
| 128 | 8 | `morph_delta_address` | GPU address (`uint64_t`) of this entity's current morph-target delta buffer, `0` when the entity has no morph data (#3231) |
| 136 | 8 | `morph_weight_address` | GPU address (`uint64_t`) of this entity's current morph weights, `0` under the same condition as `morph_delta_address` (#3231) |
| 144 | 4 | `morph_target_count` | Number of morph targets the two addresses above carry, `0` when neither is live (#3231) |
| 148 | 12 | `_reserved2a`/`_reserved2b`/`_reserved2c` | Padding to a 16-byte-aligned std430 stride — three separate scalar `u32`s, deliberately not a `uvec3` (std430 vec3-alignment footgun, #3231) |

**Instance flags** (`flags` field, offset 84):

| Bits | Constant | Meaning |
|---|---|---|
| 0 | `INSTANCE_FLAG_NON_UNIFORM_SCALE` | Requires inverse-transpose for normal transform |
| 1 | `INSTANCE_FLAG_ALPHA_BLEND` | `NiAlphaProperty` blend enabled |
| 2 | `INSTANCE_FLAG_CAUSTIC_SOURCE` | Refractive surface — caustic scatter enabled |
| 3 | `INSTANCE_FLAG_TERRAIN_SPLAT` | Terrain splatting pass active |
| 4–5 | render layer | 2-bit packed layer index: `(flags >> 4) & 0x3` |
| 6 | `INSTANCE_FLAG_PRESKINNED` | Reserved: pre-skinned vertex offset |
| 7 | `INSTANCE_FLAG_FLAT_SHADING` | Flat shading via screen-space derivative normal |
| 8 | `INSTANCE_FLAG_DIFFUSE_ALPHA` | BC1 diffuse texture carries alpha (guards `NiAlphaProperty`-less alpha test) |
| 16–31 | terrain tile index | `(flags >> 16) & 0xFFFF` (when bit 3 set) |

### `GpuMaterial` — 432 bytes, SSBO (Set 1, Binding 13)

Indexed by `GpuInstance.material_id`. Deduplicated per frame: identical
material params share one entry. Up to `MAX_MATERIALS` = 16 384 entries.

Selected fields (full layout in
[`vulkan/material.rs`](../../crates/renderer/src/vulkan/material.rs)):

| Offset | Field | Contents |
|---|---|---|
| 0 | `roughness` | Perceptual roughness [0, 1] |
| 4 | `metalness` | Metallicity [0, 1] |
| 8 | `emissive_mult` | Self-illumination multiplier |
| 12 | `material_flags` | Bit flags — see below |
| 16–27 | `emissive_rgb` | Self-illumination colour (3 × f32) |
| 28–43 | `specular` | Strength + tint RGB |
| 44 | `alpha_threshold` | Alpha test cutoff |
| 48–79 | texture indices | normal, dark, glow, detail, gloss, parallax, env, env_mask (8 × u32; the diffuse index lives on `GpuInstance`, #3909) |
| 80 | `alpha_test_func` | 0=ALWAYS … 7=NEVER |
| 84 | `material_kind` | Classification — see below |
| 88 | `material_alpha` | Authored material alpha (`NiAlphaProperty`-independent) |
| 92–96 | parallax POM | height scale, max sample passes |
| 100–115 | UV transform | offset U/V + scale U/V |
| 116–136 | diffuse/ambient | legacy diffuse RGB + ambient RGB |
| 140–167 | tinting | skin tint ARGB, hair tint RGB (Skyrim+) |
| 168–227 | multi-layer / eye / sparkle | envmap strength, eye cubemap centers + scale, refraction scale, sparkle RGB |
| 228 | `sparkle_intensity` | Sparkle/glitter effect strength |
| 232–251 | BSEffect falloff | start/stop angle, start/stop opacity, soft depth |
| 252 | `greyscale_lut_index` | Bindless index of the BSEffectShaderProperty palette LUT (0 = none) |
| 256–272 | BGSM translucency | subsurface RGB, transmissive scale, turbulence |
| 276 | `ior` | Refractive index (default 1.5) |
| 280 | `subsurface` | Disney diffuse subsurface strength |
| 284 | `sheen` | Disney sheen strength |
| 288 | `sheen_tint` | 0 = white sheen, 1 = albedo-tinted sheen |
| 292 | `anisotropic` | Anisotropic GGX strength [0, 1] |
| 296 | `tint_map_index` | Supplemental role — bindless index (0 = none) |
| 300 | `inner_layer_map_index` | Supplemental role |
| 304 | `specular_map_index` | Supplemental role |
| 308 | `lighting_map_index` | Supplemental role — imported/uploaded but deliberately **unsampled** pending coordinate semantics |
| 312 | `flow_map_index` | Supplemental role — deliberately **unsampled** |
| 316 | `wrinkle_map_index` | Supplemental role — deliberately **unsampled** pending actor-control semantics |
| 320 | `reflectance_map_index` | Supplemental role |
| 324 | `emittance_gradient_map_index` | Supplemental role |
| 328–340 | `decal_map_0..3_index` | Four decal role indices (4 × u32) |
| 344–356 | animated shader sinks | Animated shader colour RGB + scalar; captured pending named-controller dispatch |
| 360–388 | BGEM glass optics | Fresnel tint RGB, refraction deviation, blur scale/factor, scratch-roughness and dirt-overlay indices |
| 392–416 | authored lighting response | lighting-effect pair, subsurface rolloff, rim/back powers, Fresnel power, greyscale-to-palette scale |
| 420–424 | lighting texture roles | soft/rim lighting mask and back-lighting map indices |
| 428 | `detail_neutral` | #4422 producer-declared detail-combine neutral → total **432** |

The twelve entries at 296–340 are the original source-agnostic supplemental
texture roles introduced with `MaterialTextureSet<T>`. Three of them
(`lighting_map`, `flow_map`,
`wrinkle_map`) are populated and hashed but not yet sampled by any shader —
that is intentional, not drift. The glass and Bethesda lighting suites are
fully sampled by `triangle.frag`; every appended field is included in
`GpuMaterial`'s byte-hash dedup (#4201 — one hash of the built struct, the
retired draw-command field walk included) so material-table dedup cannot
alias distinct authored responses.

**`material_flags`** (offset 12):

| Bit | Constant | Meaning |
|---|---|---|
| 0 | `MAT_FLAG_VERTEX_COLOR_EMISSIVE` | Vertex colour drives emissive instead of albedo |
| 1 | `MAT_FLAG_EFFECT_SOFT` | BSEffectShaderProperty soft (depth-feathered) particles |
| 2 | `MAT_FLAG_EFFECT_PALETTE_COLOR` | Sample `greyscale_lut_index` for colour (1D LUT) |
| 3 | `MAT_FLAG_EFFECT_PALETTE_ALPHA` | Sample `greyscale_lut_index` for alpha |
| 4 | `MAT_FLAG_EFFECT_LIT` | BSEffectShaderProperty responds to scene lights |
| 5 | `MAT_FLAG_PBR_BSDF` | Disney diffuse + sheen enabled (else Lambert) |
| 6 | `MAT_FLAG_TRANSLUCENCY` | BGSM v≥8 translucency suite |
| 7 | `MAT_FLAG_MODEL_SPACE_NORMALS` | Normal map is model-space, not tangent-space |
| 8 | `MAT_FLAG_TRANSLUCENCY_THICK_OBJECT` | Translucency: thick-object attenuation profile |
| 9 | `MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO` | Translucency: mix subsurface colour with albedo |
| 10 | `BGSM_AUTHORED` | Host-side provenance only — **deliberately not mirrored to GLSL** (`build.rs` and `shader_constants_data.rs` both carry an explicit "intentionally NOT emitted" note). Not a free bit: a new shader-visible `MAT_FLAG_*` allocated here would collide with the bit `cell_loader.rs` already sets (#3989) |
| 11 | `MAT_FLAG_THIN_GLASS` | Non-occluding glass — zero-ray Fresnel/framebuffer-transmission path, no RT (#883f57cd) |
| 12 | `MAT_FLAG_MSN_HAS_AUTHORED_Z` | Model-space normal map carries authored Z instead of requiring reconstruction |
| 13 | `MAT_FLAG_SOFT_LIGHTING` | Enable masked wrapped diffuse response |
| 14 | `MAT_FLAG_RIM_LIGHTING` | Enable masked view-dependent rim response |
| 15 | `MAT_FLAG_BACK_LIGHTING` | Enable the authored back-facing lighting lobe |

Bits 16–23 (`MAT_FLAG_EFFECT_LI_SHIFT`) additionally pack an 8-bit
lighting-influence value for `MAT_FLAG_EFFECT_LIT` materials, read as
`(materialFlags >> 16) & 0xFF) / 255.0`.

**`material_kind`** (offset 88):

| Value | Constant | Meaning |
|---|---|---|
| 0–19 | — | Skyrim+ `BSLightingShaderProperty.shader_type` (forwarded verbatim) |
| 100 | `MATERIAL_KIND_GLASS` | Alpha-blend + metalness < 0.3 → RT reflection/refraction path |
| 101 | `MATERIAL_KIND_EFFECT_SHADER` | BSEffectShaderProperty — emissive additive, no scene lights |
| 102 | `MATERIAL_KIND_NO_LIGHTING` | BSShaderNoLightingProperty — fullbright, no lights/GI |
| 103 | `MATERIAL_KIND_FIRE_REFRACTION` | Fire-proxy heat haze. `shadow_transport.glsl` folds it into `effectCard` so fire proxies cast no shadow (#2224); `triangle.frag` reinterprets `mat.ior` as a 0–1 distortion scalar rather than a refractive index (#2232) |

### `GpuLight` — 64 bytes, SSBO (Set 1, Binding 0)

Prefixed by a 16-byte header (`u32 count` + 3 × `u32` padding). Up to
`MAX_LIGHTS` = 1023 entries per frame. Index 1023 (`0x3ff`) remains the packed
ReSTIR invalid-selection sentinel and is never occupied by a real light.

| Offset | Field | Contents |
|---|---|---|
| 0–11 | `position.xyz` | World position |
| 12 | `radius` | Light radius (Bethesda units) |
| 16–27 | `color.rgb` | Linear colour [0, 1] |
| 28 | `type` | 0 = point, 1 = spot, 2 = directional |
| 32–43 | `direction.xyz` | Unit direction (spot/directional) |
| 44 | `spot_angle_cos` | Spot outer cone angle (cosine) |
| 48 | `falloff_exponent` | LIGH DATA falloff exponent (0 = 1.0) |
| 52 | `shadow_segment_radius` | Finite luminous-source radius used by shadow segments |
| 56 | `visibility_mask` | Exact f32 encoding of `VisibilityMask` bits; decoded to the ray-query cull mask by `decodeVisibilityMask` |
| 60 | `attenuation_model` | `ATTENUATION_MODEL_*` discriminant encoded as f32 |

---

## Scene Buffer Capacity Constants

[`constants.rs`](../../crates/renderer/src/vulkan/scene_buffer/constants.rs)

| Constant | Value | Notes |
|---|---|---|
| `MAX_LIGHTS` | 1023 | Per-frame point/spot/directional lights; packed index 1023 remains invalid |
| `MAX_LIGHTS_PER_CLUSTER` | 512 | Candidate indices retained by each 16×9×24 cluster; overflow/high-water/drop telemetry is fence-lagged |
| `MAX_INSTANCES` | 262 144 | One indirect draw command per instance worst-case |
| `MAX_MATERIALS` | 16 384 | 432 B each; deduplicated per frame |
| `MAX_TOTAL_BONES` | 196 608 | `floor(196 608 / 144)` = 1 365 palette slots, minus reserved slot 0 → **1 364 allocatable** skinned meshes (M29.6). Not an exact product: 1 365 × 144 = 196 560 leaves a 48-bone unused tail |
| `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME` | 1 366 | First-sight bind-inverse upload cap |
| `MAX_TERRAIN_TILES` | 1 024 | 32 B each |
| `IDENTITY_BONE_SLOT` | 0 | Slot 0 is always the identity matrix |

---

## Descriptor Sets

Global sets shared across most pipelines; per-pass sets are private to their
pipeline. Defined in
[`vulkan/descriptors.rs`](../../crates/renderer/src/vulkan/descriptors.rs) and
`scene_buffer/descriptors.rs`.

| Set | Binding | Type | Resource | Used by |
|---|---|---|---|---|
| 0 | 0 | `COMBINED_IMAGE_SAMPLER` (bindless array) | All scene textures | triangle, water, ui, composite |
| 0 | 1 | `STORAGE_IMAGE` (bindless) | Per-pass read/write images | bloom, svgf, taa |
| 1 | 0 | `STORAGE_BUFFER` | Light buffer (`u32 count` + `GpuLight[]`) | triangle, cluster_cull |
| 1 | 1 | `UNIFORM_BUFFER` | `GpuCamera` (368 B) | triangle, water, cluster_cull |
| 1 | 2 | `ACCELERATION_STRUCTURE` | TLAS | triangle, water |
| 1 | 3 | `STORAGE_BUFFER` | Bone palette (current frame) | triangle |
| 1 | 4 | `STORAGE_BUFFER` | `GpuInstance[]` | triangle, ui, water |
| 1 | 5 | `STORAGE_BUFFER` | Cluster grid (`ClusterEntry[]`) | triangle |
| 1 | 6 | `STORAGE_BUFFER` | Cluster light index list | triangle |
| 1 | 7 | `COMBINED_IMAGE_SAMPLER` | SSAO texture | triangle |
| 1 | 8 | `STORAGE_BUFFER` | Global vertex SSBO (RT UV fetch) | triangle, water (via `ray_hit.glsl::resolveRayHitUV`) |
| 1 | 9 | `STORAGE_BUFFER` | Global index SSBO (RT UV fetch) | triangle, water (via `ray_hit.glsl::resolveRayHitUV`) |
| 1 | 10 | `STORAGE_BUFFER` | Terrain tile buffer | triangle |
| 1 | 11 | `STORAGE_BUFFER` | `GpuRayBudget` — 17 × `u32` (68 B): `rayBudgetCount`, `glassRayLimit`, `directShadowSamples`, `maxPathSegments`, `maxShadedHits`, `volumetricLightCap`, `qualityTier`, `reserved`, plus nine RT-LOD telemetry words (`lodFragments`, `lodBin0..3`, `reflectionTraced`, `reflectionLodCulled`, `giTraced`, `giLodCulled`). Only word 0 is the CPU-zeroed atomic counter; sizing a range/flush/barrier from `u32` is 64 B short | triangle |
| 1 | 12 | `STORAGE_BUFFER` | Bone palette (previous frame) | triangle |
| 1 | 13 | `STORAGE_BUFFER` | Material table (`GpuMaterial[]`) | triangle, water (`materials[inst.materialId]` in the secondary-ray hit path) |
| 1 | 14 | `UNIFORM_BUFFER` | DALC cube (6-axis ambient) | triangle |
| 1 | 15 | `COMBINED_IMAGE_SAMPLER` | Depth history texture (previous frame, D32) | triangle (soft-particle feather) |
| 1 | 16 | `STORAGE_BUFFER` | ReSTIR reservoir buffer (current frame) | triangle (Session-49 ReSTIR) |
| 1 | 17 | `STORAGE_BUFFER` | ReSTIR reservoir buffer (previous frame) | triangle (Session-49 ReSTIR) |
| 1 | 18 | `STORAGE_BUFFER` | Previous-frame rigid instance model matrices (rigid motion vectors). Entries align **index-for-index** with binding 4's current-frame `GpuInstance[]` after sorting/batching, so `gl_InstanceIndex` addresses both without depending on last frame's draw order | triangle (vertex stage) |
| 1 | 19 | `STORAGE_BUFFER` (`coherent`) | `SelectedRayProbeBuffer` — the debug ray-probe readback for the currently-selected pixel: control (generation, state, pixel xy), ids (light index, mask, hit instance, flags), origin+tMin, direction+tMax, hit distance + averaged visibility | triangle (debug ray-probe path) |
| 1 | 20 | `COMBINED_IMAGE_SAMPLER` | SKYAL baked sky cubemap (`samplerCube skyCube`, GGX-prefiltered mip chain), one per frame in flight, written by `write_sky_cube`. `PARTIALLY_BOUND` and never written when `SkyCubePipeline` fails to initialise, so every read goes through `exteriorSkyRadianceOr`, gated on `exteriorSkyTint.w` | triangle (incl. `raytrace.glsl` / `lighting.glsl`), water |
| 1 | 21 | `STORAGE_BUFFER` | `SkyDiffuseBuffer` — the baked sky's 9 spherical-harmonic E/π coefficients (`sky_irradiance.comp`). Same optional ownership and `exteriorSkyTint.w` gate as binding 20, read through `exteriorSkyDiffuseOr` | triangle, groundcover_blade |
| 2 | 0 | `STORAGE_IMAGE` (`R32_UINT`) | Water caustic accumulator | water.frag (atomic add) |
| 2 | 1 | `STORAGE_BUFFER` (std430, growable) | Unsized `GpuWaterParams[]` table, 368 B per active water draw | water.vert, water.frag |

`caustic_splat.comp` likewise uses its own private `set = 0` layout — one
descriptor set layout, bindings 0-10 (`depthTex`, `normalTex`, `meshIdTex`, its
own `LightBuffer`, its own `CameraUBO`, its own `InstanceBuffer`, TLAS,
`causticAccum`, `CausticParams`, and its own `GlobalVertices`/`GlobalIndices`).
It binds neither the bindless Set 0 nor the scene Set 1, which is *why* it
declares its own `GpuInstance` mirror and its own vertex/index SSBOs rather
than reusing Set 1's. Those private mirrors are outside the Set-1 lockstep
guards — assuming otherwise is the class of mistake that produced #3829.

#4019 — until then the table above credited `caustic`/`caustic_splat` with
Set-0 bindings 0-1 and Set-1 bindings 0, 1, 2 and 4, and `volumetrics` with
Set-0 binding 0 and Set-1 bindings 1 and 2. Ground truth is
`caustic.rs`/`volumetrics.rs`, both of which build their
`VkPipelineLayout` with `set_layouts(std::slice::from_ref(&…))` — a single
set — and both shaders declare `set = 0` exclusively. The `volumetrics` cells
were additionally contradicted by the very next paragraph:

Volumetrics uses its own private `set = 0` layout, split across two shaders
that do NOT share one binding scheme — neither binds any Set-1 resource
above.

`volumetrics_inject.comp` (24 bindings — widened twice past the original 12:
#2228/#2231's fog-volume work, then the combustion-transport bindings below.
Regenerated 2026-09-11 from the live `layout(...)` declarations after #3829
found the table two generations stale; verify against the source before
relying on this table for a new binding):

| Binding | Type | Resource |
|---|---|---|
| 0 | `STORAGE_IMAGE` (`rgba16f`, write-only) | Froxel grid (injection output) |
| 1 | `UNIFORM_BUFFER` | `VolumetricsParams` |
| 2 | `ACCELERATION_STRUCTURE` | TLAS (shadow-visibility rays into the froxel grid) |
| 3 | `STORAGE_BUFFER` | Light buffer (`u32 count` + `GpuLight[]`) |
| 4 | `STORAGE_BUFFER` | Cluster grid (`ClusterEntry[]`) |
| 5 | `STORAGE_BUFFER` | Cluster light index list |
| 6 | `COMBINED_IMAGE_SAMPLER` (`sampler3D`) | Previous frame's froxel grid (temporal reprojection) |
| 7 | `STORAGE_BUFFER` | `GpuFogVolume[]` — authored local fog volumes (#2228/#2231) |
| 8 | `STORAGE_BUFFER` | Fog-volume cluster grid (`FogClusterEntry[]`) |
| 9 | `STORAGE_BUFFER` | Fog-volume cluster index list |
| 10 | `COMBINED_IMAGE_SAMPLER` (`sampler3D`) | Base density noise |
| 11 | `COMBINED_IMAGE_SAMPLER` (`sampler3D`) | Detail density noise |
| 12 | `STORAGE_IMAGE` (`r32f`, write-only) | `emissionHistory` — transported emission field, current slot (#2809) |
| 13 | `COMBINED_IMAGE_SAMPLER` (`sampler3D`) | `previousEmissionHistory` — prior frame-in-flight slot, for semi-Lagrangian backtrace |
| 14 | `STORAGE_IMAGE` (`rgba16f`, write-only) | `combustionState` — (fuel mass fraction, temperature K, extinction σ_t, visible-radiance calibration), current slot |
| 15 | `COMBINED_IMAGE_SAMPLER` (`sampler3D`) | `previousCombustionState` — prior frame-in-flight slot |
| 16 | `STORAGE_IMAGE` (`rgba16f`, write-only) | `combustionDynamics` — (world-space velocity xyz, specific overpressure), current slot |
| 17 | `COMBINED_IMAGE_SAMPLER` (`sampler3D`) | `previousCombustionDynamics` — prior frame-in-flight slot |
| 18 | `STORAGE_BUFFER` | `CombustionLightMomentBuffer` — coarse radiant moments (fixed-point luma/centroid/RGB/volume) of the transported field, the canonical bridge from participating-medium emission to surface lighting |
| 19 | `STORAGE_BUFFER` (read-only) | `BoundaryInstanceBuffer` — `GpuBoundaryInstance[]`, TLAS-backing geometry the boundary-geometry read path needs (#3829) |
| 20 | `STORAGE_BUFFER` (read-only) | `BoundaryVertexBuffer` — flat `float[]` vertex data for the above |
| 21 | `STORAGE_BUFFER` (read-only) | `BoundaryIndexBuffer` — flat `uint[]` index data for the above |
| 22 | `STORAGE_IMAGE` (`rgba16f`, write-only) | `combustionOptical` — (spectral scattering σ_s.rgb, reserved), current slot |
| 23 | `COMBINED_IMAGE_SAMPLER` (`sampler3D`) | `previousCombustionOptical` — prior frame-in-flight slot |

Bindings are not laid out in strictly ascending declaration order in the
source (12–18 declare in order, then 19–21, then 22–23 return to the
combustion-optical group started at 14/16) — this table is ordered by
binding number, not by source line, so cross-reference by number rather
than by position when checking against the shader.

`volumetrics_integrate.comp` (3 bindings — a separate, much smaller
descriptor set on the same `set = 0` index; do not conflate with the table
above):

| Binding | Type | Resource |
|---|---|---|
| 0 | `STORAGE_IMAGE` (`rgba16f`, read-only) | Froxel grid (injection input) |
| 1 | `STORAGE_IMAGE` (`rgba16f`, write-only) | Integrated froxel grid (output) |
| 2 | `UNIFORM_BUFFER` | `IntegrationParams` |

Per-pass private sets (SVGF, TAA, bloom, composite, SSAO, egui) hold their
own input/output images and are not enumerated here — they're simple enough
(one or two bindless-indexed images per pass) that the source is the
lower-maintenance reference; the volumetrics table above exists specifically
because the fog-volume additions made "froxel image + one UBO" stale enough
to mislead (#2314 / TD3-206).

---

## Pipeline Cache

[`vulkan/context/helpers.rs`](../../crates/renderer/src/vulkan/context/helpers.rs)

Disk path: `<executable directory>/pipeline_cache.bin`.

On startup, `load_or_create_pipeline_cache()` reads the binary blob and
validates the 32-byte VK_PIPELINE_CACHE_HEADER_VERSION_ONE prefix
(vendor ID, device ID, pipeline cache UUID) against the physical device.
A header mismatch (GPU swap, driver upgrade) triggers a warning and an
empty cache — no crash. The entire file is pre-validated before it is
handed to the driver (SAFE-11 / #91).

On shutdown, `save_pipeline_cache()` writes the updated blob (best-effort;
I/O failure is non-fatal). Cold pipeline creation: 10–50 ms. Warm
(cache hit): < 1 ms.

---

## Coordinate Spaces & Precision

Large worldspaces (Skyrim Tamriel, FO4 Commonwealth) place geometry tens
to hundreds of thousands of units from the origin, where f32 precision
thins out. Two distinct conventions keep this under control; mixing them
up is a precision bug, so they're documented here.

### Render-origin-relative (raster path) — `#markarth-precision`

`GpuCamera.renderOrigin` (`xyz`) is a **camera-relative render origin**,
snapped to the cell grid on the CPU. The raster geometry path runs
entirely in **render-origin-relative** space so `viewProj × worldPos`
keeps full f32 precision at large offsets:

> **`renderOrigin.w` is not padding.** It carries the FSR
> one-frame-reset flag, uploaded in `context/draw.rs` and read by
> `triangle.frag`'s FSR-reset debug view. Several shader-side comments
> described it as unused until #2164/L-10 — the same trap #1928 fixed for
> `VolumetricsParams.render_origin.w`. Don't claim the slot.

- Rigid draws: the instance `model` translation is rebased on the CPU.
- Skinned draws: `triangle.vert` rebases the blended bone-palette
  translation by `-renderOrigin` (#1486), since the bone palette and the
  skinned BLAS are built in absolute world space.
- The vertex shader emits `fragWorldPosRel` (the render-origin-relative
  position) as the `location = 3` varying. **#1496**: it is passed
  *relative* and the absolute is reconstructed in `triangle.frag`
  (`fragWorldPos = fragWorldPosRel + renderOrigin`) at the top of
  `main()`. This keeps the `dFdx/dFdy` consumers — flat-shading normal,
  derivative TBN (`perturbNormal`), POM (`parallaxDisplaceUV`), and the
  rtLOD footprint — operating on *small relative* magnitudes, moving the
  f32 quantization after the derivative stage. (Pre-#1496 the varying was
  absolute, feeding those derivatives up to ~0.0156 u ULP noise at
  `|world| ≥ 131k`.) Zero extra varying cost.

### Absolute world space (RT path) — and its f32 ceiling

Ray tracing is **not** rebased. By design these stay in **absolute**
world space:

- TLAS instance transforms (`acceleration/tlas.rs`).
- Skinned BLAS vertices (`skin_vertices.comp` bakes the absolute palette).
- Ray origins reconstructed in `triangle.frag` (`fragWorldPos`, lighting,
  fog) — the absolute reconstruction above feeds them.

The f32 ULP at coordinate magnitude `X` is `2^(floor(log2 X) − 23)`. Numerical
self-intersection avoidance no longer assumes one engine-unit epsilon:
`include/ray_origin.glsl` moves each origin to the next representable float on
the outgoing side of the surface and numerical ray initializers use `tMin =
0`. Reflection, GI, glass/window continuation, water reflection/refraction/
shoreline/caustic, and caustic-splat source/entry/exit/receiver queries share
that contract. Named non-zero ray distances are therefore physical thickness,
segment exclusion, range, or LOD policy—not a hidden fixed epsilon.

Absolute-space AS transforms still have a finite representable range, so the
conservative ceiling remains ~1 M units (`REN2-10` / **#1495**). Vanilla worldspaces top
out far below this (Skyrim Tamriel ≈ ±233 k), so nothing ships near the
limit — but a future mega-worldspace could trip it silently. The cell
loader guards against that: `cell_loader/references/` (a directory —
`RT_ABSOLUTE_PRECISION_CEILING` and the `worldspace_extent_over_rt_ceiling`
predicate live in `mod.rs`, the firing `debug_assert!` in `complete.rs`)
computes the loaded cell's worldspace bounds and asserts the max `|coord|`
stays below `RT_ABSOLUTE_PRECISION_CEILING` (`2^20 = 1_048_576` u). The
predicate is unit-tested (`import_tests.rs`).

**Any future absolute-space shader consumer inherits this same ceiling.** It
must include the shared origin helper rather than adding a local bias.

## See Also

- [Vulkan Renderer](renderer.md) — init chain, BLAS/TLAS lifecycle, sync, teardown
- [Asset Pipeline](asset-pipeline.md) — how NIF geometry reaches the vertex/index SSBOs
- [NIFAL](nifal.md) — how per-game materials become `GpuMaterial` entries
- [Shadow Pipeline Trade-offs](shadow-pipeline-tradeoffs.md) — W_CLAMP, TAA γ, seed values with invalidation conditions
- [`crates/renderer/src/vulkan/scene_buffer/`](../../crates/renderer/src/vulkan/scene_buffer/) — full Rust source for all GPU types and upload logic
