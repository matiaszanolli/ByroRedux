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
| `triangle.frag` | Main PBR fragment shader — Disney BSDF, RT ray-query shadows / reflections / 1-bounce GI, glass RT refraction, terrain splatting, terrain blend |
| `water.vert` | Water quad vertex — flat local-space mesh (no per-frame BLAS rebuild) |
| `water.frag` | Water surface — RT reflection/refraction, Fresnel mix, caustic accumulator `imageAtomicAdd`, shoreline foam RT ray |
| `ui.vert` | UI quad passthrough — position already in NDC [-1, 1] |
| `ui.frag` | UI bindless texture sampling — no shading, straight texel output |
| `composite.vert` | Fullscreen triangle via `gl_VertexIndex` — no vertex buffer |
| `composite.frag` | HDR compose — direct + SVGF-denoised indirect, ACES tone-map, bloom add, volumetric froxel sample, underwater FX |

### Compute

| File | Role |
|------|------|
| `skin_palette.comp` | Build per-slot bone-matrix palette from world transforms + bind inverses |
| `skin_vertices.comp` | Deform skinned vertex positions / normals via palette lookup; output drives per-entity BLAS refit |
| `cluster_cull.comp` | Build per-froxel light lists (clustered shading) |
| `ssao.comp` | Screen-space ambient occlusion texture generation |
| `svgf_temporal.comp` | Temporal denoiser — motion-vector reprojection + color/moments accumulation for indirect lighting |
| `svgf_atrous.comp` | Spatial denoiser — edge-stopping à-trous wavelet filter, `ATROUS_ITERATIONS` = 5 ping-pong passes after the temporal dispatch; final slot feeds composite (Session-49) |
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

---

## Per-Frame Submission Order

All passes record into a single command buffer and are submitted to one
graphics+compute queue. Pass ordering is inside
[`vulkan/context/draw.rs`](../../crates/renderer/src/vulkan/context/draw.rs).

```
1  skin_palette.comp    ─┐ compute
2  skin_vertices.comp   ─┘ skinned BLAS input ready
3  AccelerationManager   ─  BLAS rebuild / refit + TLAS build
4  [Main render pass]   ─  raster (BEGIN → END):
     triangle.vert / .frag  geometry + RT ray-queries
     water.vert / .frag     water + caustic imageAtomicAdd
5  [Barrier]               SHADER_READ_ONLY_OPTIMAL on all G-buffer attachments
6  [Barrier]               caustic accum atomic-add → SHADER_READ
7  svgf_temporal.comp   ─  temporal denoiser (indirect lighting)
8  svgf_atrous.comp ×5  ─  à-trous spatial denoiser (ATROUS_ITERATIONS),
   [COMPUTE→COMPUTE]        ping-pong slots gated each iteration by a
                           COMPUTE→COMPUTE barrier; final (odd count → slot 0)
                           is what composite samples via indirect_view(frame)
9  caustic_splat.comp   ─  caustic scatter
10 volumetrics_inject   ─┐ froxel grid (gated: VOLUMETRIC_OUTPUT_CONSUMED)
11 volumetrics_integrate ─┘
12 taa.comp              ─  TAA resolve
13 ssao.comp             ─  SSAO texture
14 bloom_downsample ×N   ─┐ bloom pyramid
   bloom_upsample   ×N   ─┘
15 [Composite render pass]─ raster:
     composite.vert / .frag  HDR combine → swapchain (PRESENT_SRC_KHR)
16 [Egui render pass]    ─  egui overlay (blended on swapchain)
17 [Screenshot copy]     ─  transfer blit → staging buffer (if requested)
18 Queue submit
19 Present
```

---

## G-Buffer Layout

Six colour attachments + depth, all double-buffered (one set per
`MAX_FRAMES_IN_FLIGHT` = 2). Written by the main render pass
(`triangle.frag` + `water.frag`), read by SVGF, TAA, SSAO, and composite.

| Attachment | `VkFormat` | Contents | Layout during pass |
|---|---|---|---|
| HDR colour | `R16G16B16A16_SFLOAT` | Direct lighting (pre-denoised); alpha feeds SRC_ALPHA blend + water | `COLOR_ATTACHMENT_OPTIMAL` |
| Normal | `R16G16_SNORM` | Octahedral-encoded world normal | `COLOR_ATTACHMENT_OPTIMAL` |
| Motion | `R16G16_SFLOAT` | Screen-space motion vector (current → previous NDC) | `COLOR_ATTACHMENT_OPTIMAL` |
| Mesh ID | `R32_UINT` | Bits 0–30: instance ID + 1; bit 31: `ALPHA_BLEND_NO_HISTORY` (skip SVGF accumulation) | `COLOR_ATTACHMENT_OPTIMAL` |
| Raw indirect | `B10G11R11_UFLOAT_PACK32` | Albedo-demodulated indirect light (SVGF input) | `COLOR_ATTACHMENT_OPTIMAL` |
| Albedo | `B10G11R11_UFLOAT_PACK32` | Surface colour (diffuse × vertex colour) | `COLOR_ATTACHMENT_OPTIMAL` |
| Depth | `D32_SFLOAT` | Standard depth (0.0 = near, 1.0 = far), `LESS_OR_EQUAL`, clear = 1.0 | `DEPTH_STENCIL_ATTACHMENT_OPTIMAL` |

After `vkCmdEndRenderPass` all attachments transition to `SHADER_READ_ONLY_OPTIMAL`.

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
| 288 | 16 | `sun_direction` | xyz = direction **from** sun (unit); w = sun intensity |
| 304 | 16 | `dof_params` | x = aperture half-radius; y = focus distance; zw reserved |
| 320 | 16 | `render_origin` | xyz = camera-relative render origin (#markarth-precision); w reserved |

### `GpuInstance` — 112 bytes, SSBO (Set 1, Binding 4)

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
| 92 | 4 | *(padding)* | — |
| 96 | 4 | `avg_albedo_r` | Pre-computed average albedo R |
| 100 | 4 | `avg_albedo_g` | Pre-computed average albedo G |
| 104 | 4 | `avg_albedo_b` | Pre-computed average albedo B |
| 108 | 4 | *(padding)* | — |

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
| 16–31 | terrain tile index | `(flags >> 16) & 0xFFFF` (when bit 3 set) |

### `GpuMaterial` — 300 bytes, SSBO (Set 1, Binding 13)

Indexed by `GpuInstance.material_id`. Deduplicated per frame: identical
material params share one entry. Up to `MAX_MATERIALS` = 16 384 entries.

Selected fields (full layout in `gpu_types.rs`):

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
| 96–119 | UV transform | offset U/V + scale U/V; diffuse/ambient colour legacy |
| 144–171 | tinting | skin tint ARGB, hair tint RGB (Skyrim+) |
| 232–255 | BSEffect falloff | start/stop angle, start/stop opacity, soft depth |
| 280 | `ior` | Refractive index (default 1.5) |
| 284 | `subsurface` | Disney diffuse subsurface strength |
| 288 | `sheen` | Disney sheen strength |
| 292 | `sheen_tint` | 0 = white sheen, 1 = albedo-tinted sheen |
| 296 | `anisotropic` | Anisotropic GGX strength [0, 1] |

**`material_flags`** (offset 12):

| Bit | Constant | Meaning |
|---|---|---|
| 0 | `MAT_FLAG_VERTEX_COLOR_EMISSIVE` | Vertex colour drives emissive instead of albedo |
| 5 | `MAT_FLAG_PBR_BSDF` | Disney diffuse + sheen enabled (else Lambert) |
| 6 | `MAT_FLAG_TRANSLUCENCY` | BGSM v≥8 translucency suite |
| 7 | `MAT_FLAG_MODEL_SPACE_NORMALS` | Normal map is model-space, not tangent-space |

**`material_kind`** (offset 88):

| Value | Constant | Meaning |
|---|---|---|
| 0–19 | — | Skyrim+ `BSLightingShaderProperty.shader_type` (forwarded verbatim) |
| 100 | `MATERIAL_KIND_GLASS` | Alpha-blend + metalness < 0.3 → RT reflection/refraction path |
| 101 | `MATERIAL_KIND_EFFECT_SHADER` | BSEffectShaderProperty — emissive additive, no scene lights |
| 102 | `MATERIAL_KIND_NO_LIGHTING` | BSShaderNoLightingProperty — fullbright, no lights/GI |

### `GpuLight` — 64 bytes, SSBO (Set 1, Binding 0)

Prefixed by a `u32 lightCount`. Up to `MAX_LIGHTS` = 512 entries per frame.

| Offset | Field | Contents |
|---|---|---|
| 0–11 | `position.xyz` | World position |
| 12 | `radius` | Light radius (Bethesda units) |
| 16–27 | `color.rgb` | Linear colour [0, 1] |
| 28 | `type` | 0 = point, 1 = spot, 2 = directional |
| 32–43 | `direction.xyz` | Unit direction (spot/directional) |
| 44 | `spot_angle_cos` | Spot outer cone angle (cosine) |
| 48 | `falloff_exponent` | LIGH DATA falloff exponent (0 = 1.0) |
| 52–63 | *(reserved)* | — |

---

## Scene Buffer Capacity Constants

[`constants.rs`](../../crates/renderer/src/vulkan/scene_buffer/constants.rs)

| Constant | Value | Notes |
|---|---|---|
| `MAX_LIGHTS` | 512 | Per-frame point/spot/directional lights |
| `MAX_INSTANCES` | 262 144 | One indirect draw command per instance worst-case |
| `MAX_MATERIALS` | 16 384 | 300 B each; deduplicated per frame |
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
| 1 | 11 | `STORAGE_BUFFER` | Ray budget counter (`u32`) | triangle, volumetrics |
| 1 | 12 | `STORAGE_BUFFER` | Bone palette (previous frame) | triangle |
| 1 | 13 | `STORAGE_BUFFER` | Material table (`GpuMaterial[]`) | triangle |
| 1 | 14 | `UNIFORM_BUFFER` | DALC cube (6-axis ambient) | triangle |
| 2 | 0 | `STORAGE_IMAGE` (`R32_UINT`) | Water caustic accumulator | water.frag (atomic add) |

Per-pass private sets (SVGF, TAA, bloom, composite, SSAO, volumetrics,
egui) hold their own input/output images and are not shared.

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
