# Shader Pipeline

The ByroRedux renderer is a **deferred + ray-traced** Vulkan pipeline built on
Vulkan 1.3 with ray-query extensions. Every frame visits a fixed set of passes in
strict submission order. This document is the reference for the per-pass
responsibilities, GPU data layouts, and descriptor bindings. For the high-level
renderer architecture (BLAS/TLAS, sync, swapchain, teardown ordering) see
[renderer.md](renderer.md).

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
| `presentation.frag` | FSR 3.1 presentation pass — samples the upscaled (or native-blit-fallback) scene, applies ACES tone-mapping and underwater extinction, writes the swapchain (`PRESENT_SRC_KHR`) |

### Compute

| File | Role |
|------|------|
| `skin_palette.comp` | Build per-slot bone-matrix palette from world transforms + bind inverses |
| `skin_vertices.comp` | Deform skinned vertex positions / normals via palette lookup; output drives per-entity BLAS refit |
| `cluster_cull.comp` | Build per-froxel light lists (clustered shading) |
| `ssao.comp` | Screen-space ambient occlusion texture generation |
| `svgf_temporal.comp` | Temporal denoiser — motion-vector reprojection + color/moments accumulation for indirect lighting |
| `svgf_atrous.comp` | Spatial denoiser — edge-stopping à-trous wavelet filter, `ATROUS_ITERATIONS` = 3 ping-pong passes after the temporal dispatch; final slot feeds composite (Dugout ablation capped the footprint at 14 render pixels) |
| `taa.comp` | TAA resolve — Halton(2,3) jitter, YCoCg variance-clamp, history reproject |
| `bloom_downsample.comp` | Gaussian + downsample pyramid (bright content) |
| `bloom_upsample.comp` | Upsample + blur stages of bloom pyramid |
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
graphics+compute queue. Pass ordering is inside
[`vulkan/context/draw.rs`](../../crates/renderer/src/vulkan/context/draw.rs).

```
1  skin_palette.comp    ─┐ compute
2  skin_vertices.comp   ─┘ skinned BLAS input ready
3  AccelerationManager   ─  BLAS rebuild / refit + TLAS build
4  cluster_cull.comp     ─  per-froxel light lists (cluster grid +
                           light-index list); consumed by both the
                           triangle.frag fragment shader AND
                           volumetrics_inject (same per-frame buffers,
                           #977eb95a)
5  [Main render pass]   ─  raster (BEGIN → END):
     triangle.vert / .frag  geometry + RT ray-queries
     water.vert / .frag     water + caustic imageAtomicAdd
6  [Barrier]               SHADER_READ_ONLY_OPTIMAL on all G-buffer attachments
7  [Barrier]               caustic accum atomic-add → SHADER_READ
8  svgf_temporal.comp   ─  temporal denoiser (indirect lighting)
9  svgf_atrous.comp ×3  ─  à-trous spatial denoiser (ATROUS_ITERATIONS),
   [COMPUTE→COMPUTE]        ping-pong slots gated each iteration by a
                           COMPUTE→COMPUTE barrier; final (odd count → slot 0)
                           is what composite samples via indirect_view(frame)
10 caustic_splat.comp   ─  caustic scatter
11 volumetrics_inject   ─┐ froxel grid (gated: VOLUMETRIC_OUTPUT_CONSUMED);
                           reads cluster_cull's cluster grid + light-index
                           list from step 4
12 volumetrics_integrate ─┘
13 taa.comp              ─  TAA resolve
14 ssao.comp             ─  SSAO texture
15 bloom_downsample ×N   ─┐ bloom pyramid
   bloom_upsample   ×N   ─┘
16 [Composite render pass]─ raster:
     composite.vert / .frag  HDR combine → intermediate HDR image
                           (`R16G16B16A16_SFLOAT`, `SHADER_READ_ONLY_OPTIMAL`;
                           no tone-map, does NOT write the swapchain)
17 frame_upscaler.record  ─  FSR 3.1 SDK dispatch (Quality preset default) or
                           native-blit fallback (`--upscaler taa`) — render-
                           resolution HDR → output-resolution HDR
18 [Presentation pass]    ─  raster: composite.vert / presentation.frag —
                           exposure + ACES tone-map + underwater extinction,
                           writes the swapchain (`PRESENT_SRC_KHR`)
19 [Egui render pass]    ─  egui overlay (blended on swapchain)
20 [Screenshot copy]     ─  transfer blit → staging buffer (if requested)
21 Queue submit
22 Present
```

Steps 17–18 are the FSR 3.1 tail added 2026-07-22→24 (`crates/fsr3-sys`,
`vulkan/frame_upscaler.rs`, `vulkan/presentation.rs`, `vulkan/exposure.rs`);
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

After `vkCmdEndRenderPass` all attachments transition to `SHADER_READ_ONLY_OPTIMAL`.

> **Why Mesh ID carries two representations.** `fragInstanceIndex` follows the
> per-frame *sorted* draw order, so an actor changing depth bucket used to make
> static architecture look like a different surface and reset TAA + SVGF across
> the room. Opaque draws therefore write the ECS-derived `surface_id`, which is
> stable across frames. Alpha-blended draws bypass both histories anyway (bit
> 31), so they keep the sorted index in the low bits — `caustic_splat.comp`
> consumes it to index the current-frame instance SSBO. `0` remains the
> clear/background value. See `triangle.frag`'s `stableSurfaceId` block.

---

## GPU Data Types

### `GpuCamera` — 336 bytes, uniform buffer (Set 1, Binding 1)

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

### `GpuInstance` — 128 bytes, SSBO (Set 1, Binding 4)

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
| 120 | 8 | `_reserved` | Padding to a 16-byte-aligned std430 stride — no live data |

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

### `GpuMaterial` — 348 bytes, SSBO (Set 1, Binding 13)

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
| 48–83 | texture indices | diffuse, normal, dark, glow, detail, gloss, parallax, env, env_mask (9 × u32) |
| 84 | `alpha_test_func` | 0=ALWAYS … 7=NEVER |
| 88 | `material_kind` | Classification — see below |
| 92 | `material_alpha` | Authored material alpha (`NiAlphaProperty`-independent) |
| 96–100 | parallax POM | height scale, max sample passes |
| 104–119 | UV transform | offset U/V + scale U/V |
| 120–140 | diffuse/ambient | legacy diffuse RGB + ambient RGB |
| 144–171 | tinting | skin tint ARGB, hair tint RGB (Skyrim+) |
| 172–231 | multi-layer / eye / sparkle | envmap strength, eye cubemap centers + scale, refraction scale, sparkle RGB |
| 232 | `sparkle_intensity` | Sparkle/glitter effect strength |
| 236–255 | BSEffect falloff | start/stop angle, start/stop opacity, soft depth |
| 256 | `greyscale_lut_index` | Bindless index of the BSEffectShaderProperty palette LUT (0 = none) |
| 260–276 | BGSM translucency | subsurface RGB, transmissive scale, turbulence |
| 280 | `ior` | Refractive index (default 1.5) |
| 284 | `subsurface` | Disney diffuse subsurface strength |
| 288 | `sheen` | Disney sheen strength |
| 292 | `sheen_tint` | 0 = white sheen, 1 = albedo-tinted sheen |
| 296 | `anisotropic` | Anisotropic GGX strength [0, 1] |
| 300 | `tint_map_index` | Supplemental role — bindless index (0 = none) |
| 304 | `inner_layer_map_index` | Supplemental role |
| 308 | `specular_map_index` | Supplemental role |
| 312 | `lighting_map_index` | Supplemental role — imported/uploaded but deliberately **unsampled** pending coordinate semantics |
| 316 | `flow_map_index` | Supplemental role — deliberately **unsampled** |
| 320 | `wrinkle_map_index` | Supplemental role — deliberately **unsampled** pending actor-control semantics |
| 324 | `reflectance_map_index` | Supplemental role |
| 328 | `emittance_gradient_map_index` | Supplemental role |
| 332–344 | `decal_map_0..3_index` | Four decal role indices (4 × u32) → total **348** |

The twelve entries at 300–344 are the source-agnostic supplemental texture
roles introduced with `MaterialTextureSet<T>`; they are what grew the record
from 300 B to 348 B. Three of them (`lighting_map`, `flow_map`,
`wrinkle_map`) are populated and hashed but not yet sampled by any shader —
that is intentional, not drift.

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
| 10 | *(unused/reserved)* | — |
| 11 | `MAT_FLAG_THIN_GLASS` | Non-occluding glass — zero-ray Fresnel/framebuffer-transmission path, no RT (#883f57cd) |

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

### `GpuLight` — 64 bytes, SSBO (Set 1, Binding 0)

Prefixed by a 16-byte header (`u32 count` + 3 × `u32` padding). Up to
`MAX_LIGHTS` = 512 entries per frame.

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
| 56 | `shadow_policy` | `SHADOW_POLICY_*` encoded as f32 — see `decodeShadowPolicy` in `shadow_common.glsl` |
| 60–63 | *(reserved)* | — |

---

## Scene Buffer Capacity Constants

[`constants.rs`](../../crates/renderer/src/vulkan/scene_buffer/constants.rs)

| Constant | Value | Notes |
|---|---|---|
| `MAX_LIGHTS` | 512 | Per-frame point/spot/directional lights |
| `MAX_INSTANCES` | 262 144 | One indirect draw command per instance worst-case |
| `MAX_MATERIALS` | 16 384 | 348 B each; deduplicated per frame |
| `MAX_TOTAL_BONES` | 196 608 | 144 slots × 1 364 skinned meshes (M29.6) |
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
| 0 | 0 | `COMBINED_IMAGE_SAMPLER` (bindless array) | All scene textures | triangle, water, ui, composite, caustic, volumetrics |
| 0 | 1 | `STORAGE_IMAGE` (bindless) | Per-pass read/write images | bloom, svgf, taa, caustic |
| 1 | 0 | `STORAGE_BUFFER` | Light buffer (`u32 count` + `GpuLight[]`) | triangle, cluster_cull, caustic_splat |
| 1 | 1 | `UNIFORM_BUFFER` | `GpuCamera` (336 B) | triangle, water, cluster_cull, caustic_splat, volumetrics |
| 1 | 2 | `ACCELERATION_STRUCTURE` | TLAS | triangle, water, caustic_splat, volumetrics |
| 1 | 3 | `STORAGE_BUFFER` | Bone palette (current frame) | triangle |
| 1 | 4 | `STORAGE_BUFFER` | `GpuInstance[]` | triangle, ui, water, caustic_splat |
| 1 | 5 | `STORAGE_BUFFER` | Cluster grid (`ClusterEntry[]`) | triangle |
| 1 | 6 | `STORAGE_BUFFER` | Cluster light index list | triangle |
| 1 | 7 | `COMBINED_IMAGE_SAMPLER` | SSAO texture | triangle |
| 1 | 8 | `STORAGE_BUFFER` | Global vertex SSBO (RT UV fetch) | triangle |
| 1 | 9 | `STORAGE_BUFFER` | Global index SSBO (RT UV fetch) | triangle |
| 1 | 10 | `STORAGE_BUFFER` | Terrain tile buffer | triangle |
| 1 | 11 | `STORAGE_BUFFER` | Ray budget counter (`u32`) | triangle |
| 1 | 12 | `STORAGE_BUFFER` | Bone palette (previous frame) | triangle |
| 1 | 13 | `STORAGE_BUFFER` | Material table (`GpuMaterial[]`) | triangle |
| 1 | 14 | `UNIFORM_BUFFER` | DALC cube (6-axis ambient) | triangle |
| 1 | 15 | `COMBINED_IMAGE_SAMPLER` | Depth history texture (previous frame, D32) | triangle (soft-particle feather) |
| 1 | 16 | `STORAGE_BUFFER` | ReSTIR reservoir buffer (current frame) | triangle (Session-49 ReSTIR) |
| 1 | 17 | `STORAGE_BUFFER` | ReSTIR reservoir buffer (previous frame) | triangle (Session-49 ReSTIR) |
| 2 | 0 | `STORAGE_IMAGE` (`R32_UINT`) | Water caustic accumulator | water.frag (atomic add) |

Volumetrics uses its own private `set = 0` layout, split across two shaders
that do NOT share one binding scheme — neither binds any Set-1 resource
above.

`volumetrics_inject.comp` (12 bindings, widened by #2228/#2231's fog-volume
work — verify against the source before relying on this table for a new
binding):

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

The f32 ULP at coordinate magnitude `X` is `2^(floor(log2 X) − 23)`. The
RT shadow/reflection/GI rays bias their origins off the surface by
~`0.05–0.15` u (tMin + normal-bias). Headroom (ULP vs the bias margin):

| `\|world\|`        | f32 ULP   | vs ~0.05–0.15 u bias margin |
|------------------|-----------|------------------------------|
| ~131k (`2^17`)   | 0.0156 u  | ~3–10× headroom             |
| ~176k            | ~0.02 u   | ~2–7× headroom (REN2-10)    |
| ~524k (`2^19`)   | 0.0625 u  | tight 0.05 u margin lost     |
| ~1.05M (`2^20`)  | 0.125 u   | even the 0.15 u margin lost  |

So absolute-space RT precision **starts thinning near ~0.5 M units and
the ceiling is ~1 M** (`REN2-10` / **#1495**). Vanilla worldspaces top
out far below this (Skyrim Tamriel ≈ ±233 k), so nothing ships near the
limit — but a future mega-worldspace could trip it silently. The cell
loader guards against that: `cell_loader/references.rs` computes the
loaded cell's worldspace bounds and `debug_assert!`s the max `|coord|`
stays below `RT_ABSOLUTE_PRECISION_CEILING` (`2^20 = 1_048_576` u). The
predicate (`worldspace_extent_over_rt_ceiling`) is unit-tested.

**Any future absolute-space shader consumer inherits this same ceiling.**

## See Also

- [Vulkan Renderer](renderer.md) — init chain, BLAS/TLAS lifecycle, sync, teardown
- [Asset Pipeline](asset-pipeline.md) — how NIF geometry reaches the vertex/index SSBOs
- [NIFAL](nifal.md) — how per-game materials become `GpuMaterial` entries
- [Shadow Pipeline Trade-offs](shadow-pipeline-tradeoffs.md) — W_CLAMP, TAA γ, seed values with invalidation conditions
- [`crates/renderer/src/vulkan/scene_buffer/`](../../crates/renderer/src/vulkan/scene_buffer/) — full Rust source for all GPU types and upload logic
