// Bindless texture array + GPU structs + SSBO/UBO bindings + TLAS + vertex-layout constants
//
// NON-STANDALONE shader fragment. Included by triangle.frag and water.frag in
// dependency order via GL_GOOGLE_include_directive; it references constants
// defined in shader_constants.glsl. Do not compile on its own.

// Bindless texture array.
layout(set = 0, binding = 0) uniform sampler2D textures[];
// Authored DDS environment cubemaps. Kept in a separate binding so a cube
// image view is never consumed through the sampler2D type above.
layout(set = 0, binding = 1) uniform samplerCube cubemaps[];

// Per-instance data from the instance SSBO. R1 Phase 6 collapsed the
// per-material fields onto the `MaterialBuffer` SSBO indexed by
// `materialId`; what's left is strictly per-DRAW data. Each draw's
// gl_InstanceIndex maps to one entry containing the model matrix,
// mesh refs, flags, materialId, and avgAlbedo (kept for caustic).
//
// CRITICAL: all scalars, NO vec3 (vec3 has 16-byte alignment in
// std430, which would mismatch the tightly-packed Rust #[repr(C)]
// struct).
struct GpuInstance {
    mat4 model;            // offset 0,  64 bytes
    uint textureIndex;     // offset 64 — diffuse / albedo
    uint boneOffset;       // offset 68
    uint vertexOffset;     // offset 72
    uint indexOffset;      // offset 76
    uint vertexCount;      // offset 80
    // offset 84: per-instance bit flags + packed fields.
    //   bit 0      — non-uniform scale (#273)
    //   bit 1      — NiAlphaProperty blend bit (#263)
    //   bit 2      — caustic source (#321)
    //   bit 3      — terrain splat (#470); enables the ATXT blend loop
    //                against `terrainTiles[flags >> 16]`
    //   bits 16-31 — terrain tile index (only meaningful with bit 3)
    uint flags;
    uint materialId;       // offset 88 — index into MaterialBuffer SSBO (R1)
    float ior;             // offset 92 — per-draw optical IOR (read by caustic_splat.comp)
    float avgAlbedoR;      // offset 96 — kept for caustic_splat.comp (set 0 reads, not migrated)
    float avgAlbedoG;      // offset 100
    float avgAlbedoB;      // offset 104
    uint surfaceId;        // offset 108 — stable temporal-shadow identity
    // REN-2026-07-28-02 / #2219 — GPU address of this entity's skinned-
    // vertex output buffer (position-only, SKIN_OUTPUT_STRIDE_FLOATS
    // floats/vertex, already absolute world space). `0` for rigid
    // instances (boneOffset == 0). Dereferenced via the SkinnedVertexRef
    // buffer_reference type below by ray_hit.glsl's hit-normal helpers
    // instead of the bind-pose vertexData SSBO, which otherwise produces
    // a bind-pose-shaped normal at the entity's root transform for every
    // animated actor's secondary rays (reflection/shadow/refraction).
    uint64_t skinnedVertexAddress; // offset 112, 8 bytes
    uvec2 _reserved;               // offset 120, 8 bytes -> total 128
    // #3231 — GPU morph-target blending. `morphDeltaAddress` points at
    // `targetCount` groups of `vertexCount` `vec4` position deltas (xyz,
    // w unused); `morphWeightAddress` points at `targetCount` scalar
    // floats, re-uploaded every frame from `AnimatedMorphWeights`. Both
    // `0` when this entity has no morph data (rigid instances, or
    // skinned instances whose mesh carries no `NiGeomMorpherController`
    // — the overwhelming majority). Dereferenced via the MorphDeltaRef /
    // MorphWeightRef buffer_reference types below.
    uint64_t morphDeltaAddress;  // offset 128, 8 bytes
    uint64_t morphWeightAddress; // offset 136, 8 bytes
    uint morphTargetCount;       // offset 144, 4 bytes
    // Deliberately three scalar uints, NOT uvec3 — see the matching
    // Rust field doc (gpu_types.rs) for why: uvec3 is 16-byte-aligned
    // under std430 (same footgun as vec3), which desyncs the array
    // stride the shader computes from the one the CPU uploads with.
    uint _reserved2a; // offset 148, 4 bytes
    uint _reserved2b; // offset 152, 4 bytes
    uint _reserved2c; // offset 156, 4 bytes -> total 160
};

// REN-2026-07-28-02 / #2219 — buffer_reference handle for
// `GpuInstance.skinnedVertexAddress`. Requires `GL_EXT_buffer_reference`
// enabled in the including shader (triangle.frag / water.frag).
layout(buffer_reference, std430, buffer_reference_align = 4) readonly buffer SkinnedVertexRef {
    float data[];
};

// #3231 — buffer_reference handles for `GpuInstance.morphDeltaAddress` /
// `.morphWeightAddress`. Requires `GL_EXT_buffer_reference` (same
// extension `SkinnedVertexRef` above already needs). `MorphDeltaRef` is
// `vec4`-aligned (not `vec3`) for the same std430 reason every other
// struct in this file avoids `vec3` — `data[target * vertexCount +
// localVertexIndex].xyz` is the per-vertex delta; `.w` is unused padding.
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer MorphDeltaRef {
    vec4 data[];
};
layout(buffer_reference, std430, buffer_reference_align = 4) readonly buffer MorphWeightRef {
    float data[];
};

layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer {
    GpuInstance instances[];
};

// ── R1 Phase 4: deduplicated material table ─────────────────────────
//
// Mirrors the Rust `GpuMaterial` (396 B std430) defined
// in `crates/renderer/src/vulkan/material.rs`. Indexed by
// `GpuInstance.materialId`. Phase 4 migrates one field (`roughness`)
// off the per-instance copy onto this path; Phases 5–6 do the rest
// and finally remove the redundant per-instance copies.
//
// **Shader Struct Sync**: any field added here must be added in
// lockstep to the Rust `GpuMaterial` struct + the matching
// `intern`/encoding sites; the size of this struct (396 B) is pinned by
// `gpu_material_size_is_396_bytes` on the Rust side.
struct GpuMaterial {
    // PBR scalars (vec4 #1)
    float roughness;
    float metalness;
    float emissiveMult;
    /// Bitfield of material-level flags. Bit 0
    /// (`MAT_FLAG_VERTEX_COLOR_EMISSIVE`): per-vertex `fragColor.rgb`
    /// drives self-illumination instead of modulating albedo. See #695.
    uint materialFlags;
    // Emissive RGB + specular_strength (vec4 #2)
    float emissiveR, emissiveG, emissiveB, specularStrength;
    // Specular RGB + alpha_threshold (vec4 #3)
    float specularR, specularG, specularB, alphaThreshold;
    // Texture indices group A (vec4 #4)
    uint textureIndex, normalMapIndex, darkMapIndex, glowMapIndex;
    // Texture indices group B (vec4 #5)
    uint detailMapIndex, glossMapIndex, parallaxMapIndex, envMapIndex;
    // env_mask + alpha_test_func + material_kind + alpha (vec4 #6)
    uint envMaskIndex, alphaTestFunc, materialKind;
    float materialAlpha;
    // Parallax + UV offset (vec4 #7)
    float parallaxHeightScale, parallaxMaxPasses, uvOffsetU, uvOffsetV;
    // UV scale + diffuse RG (vec4 #8)
    float uvScaleU, uvScaleV, diffuseR, diffuseG;
    // diffuse_b + ambient RGB (vec4 #9)
    float diffuseB, ambientR, ambientG, ambientB;
    // #804 / R1-N4 — `avgAlbedoR/G/B` (offsets 144-152) removed; no
    // shader read `mat.avgAlbedo*`. Subsequent fields shift down by 12.
    // skin_tint A/R/G/B (offsets 144-156)
    float skinTintA, skinTintR, skinTintG, skinTintB;
    // hair_tint RGB + multi_layer_envmap_strength (offsets 160-172)
    float hairTintR, hairTintG, hairTintB, multiLayerEnvmapStrength;
    // eye_left RGB + eye_cubemap_scale (offsets 176-188)
    float eyeLeftCenterX, eyeLeftCenterY, eyeLeftCenterZ, eyeCubemapScale;
    // eye_right RGB + multi_layer_inner_thickness (offsets 192-204)
    float eyeRightCenterX, eyeRightCenterY, eyeRightCenterZ, multiLayerInnerThickness;
    // refraction_scale + multi_layer_inner_scale UV + sparkle_r (208-220)
    float multiLayerRefractionScale, multiLayerInnerScaleU, multiLayerInnerScaleV, sparkleR;
    // sparkle GB + sparkle_intensity + falloff_start (224-236)
    float sparkleG, sparkleB, sparkleIntensity, falloffStartAngle;
    // falloff_stop + opacities + soft_falloff_depth (240-252)
    float falloffStopAngle, falloffStartOpacity, falloffStopOpacity, softFalloffDepth;
    // #890 Stage 2c — bindless handle for
    // `BSEffectShaderProperty.greyscale_texture`. 0 = no LUT (the
    // shader's effect branch then samples the source texture raw).
    // Offset 256.
    uint greyscaleLutIndex;
    // #1147 Phase 2b — BGSM v>=8 translucency suite. Read only when
    // `materialFlags & MAT_FLAG_TRANSLUCENCY != 0u`. Layout must
    // match the Rust `GpuMaterial::translucency_*` block byte-for-byte
    // (pinned by `gpu_material_field_offsets_match_shader_contract`).
    float translucencySubsurfaceR, translucencySubsurfaceG, translucencySubsurfaceB;
    float translucencyTransmissiveScale;
    float translucencyTurbulence;
    // #1248 — per-material refractive index. Drives Schlick F0 via
    // `F0 = ((1-η)/(1+η))²` at every dielectric / glass site. Default
    // 1.5 reproduces the pre-#1248 hardcoded `vec3(0.04)` behaviour
    // for legacy NIF content with no authored IOR. Offset 280.
    //
    // #2232 — this field is discriminated by `materialKind` and carries
    // THREE distinct, incompatible-range meanings:
    //   - ordinary dielectric materials: physical refractive index (~1.0-2.5)
    //     as described above.
    //   - `MATERIAL_KIND_GLASS`: canonical glass IOR, 1.45
    //     (`GLASS_SURFACE_BEHAVIOR` in `crates/core/.../material.rs`).
    //   - `MATERIAL_KIND_FIRE_REFRACTION`: NOT a refractive index. Authored
    //     `refraction_strength`, a 0-1 heat-haze distortion scalar consumed
    //     via `clamp(mat.ior, 0.0, 1.0)` in `triangle.frag`'s fire-refraction
    //     branch. Do not "fix" values outside ~1.0-2.5 for this kind.
    float ior;
    // #1249 — Disney diffuse lobe (offsets 284-292). subsurface
    // weights the Hanrahan-Krueger fake-SSS approximation against the
    // Burley diffuse; sheen + sheenTint drive the fabric-class edge
    // highlight. All zero by default → byte-identical Lambert
    // behaviour for legacy NIF content. Only consulted when
    // `MAT_FLAG_PBR_BSDF` is set. These complete the #1249 block at
    // offset 296; #1250 `anisotropic` then completed the prior 300 B layout.
    float subsurface;
    float sheen;
    float sheenTint;
    // #1250 — anisotropic GGX strength. 0 = isotropic
    // (ax = ay = roughness; distributionGGXAniso degenerates to the
    // legacy distributionGGX lobe shape). 1 = maximum anisotropy
    // capped at `aspect = sqrt(0.1)` so the lobe doesn't fully
    // degenerate into a needle.
    float anisotropic;
    // Common supplemental semantic texture roles (offsets 300-344).
    // Source-game slot numbering has already been translated away.
    //
    // #2712 — `lightingMapIndex`, `flowMapIndex` and `wrinkleMapIndex`
    // are declared here for layout parity with the Rust `GpuMaterial`
    // (the struct is a byte contract; the fields cannot be omitted) but
    // are sampled by NO shader. That is deliberate, not an oversight:
    // lighting-map semantics are undecided for an RT-lit frame, flow
    // needs a settled UV-advection convention, and wrinkle needs
    // per-expression weights the animation path doesn't deliver yet.
    // See the field notes in crates/renderer/src/vulkan/material.rs.
    uint tintMapIndex;
    uint innerLayerMapIndex;
    uint specularMapIndex;
    uint lightingMapIndex; // unsampled — see #2712 above
    uint flowMapIndex;     // unsampled — see #2712 above
    uint wrinkleMapIndex;  // unsampled — see #2712 above
    uint reflectanceMapIndex;
    uint emittanceGradientMapIndex;
    uint decalMap0Index;
    uint decalMap1Index;
    uint decalMap2Index;
    uint decalMap3Index;
    // Animated BSShaderProperty color/scalar (offsets 348-360, #2221).
    // Same "layout parity, no shader consumer yet" precedent as the
    // three unsampled map indices above — see the field notes in
    // crates/renderer/src/vulkan/material.rs.
    float shaderColorR, shaderColorG, shaderColorB; // unsampled — see #2221
    float shaderFloat;                              // unsampled — see #2221
    // BGEM v21+/v22 authored glass optics (offsets 364-392).
    float glassFresnelR, glassFresnelG, glassFresnelB;
    float glassRefractionScale;
    float glassBlurScale;
    float glassBlurScaleFactor;
    uint glassRoughnessScratchMapIndex;
    uint glassDirtOverlayMapIndex;
    // Bethesda-authored direct-light response and translated mask roles
    // (offsets 396-428). Feature flags independently gate soft, rim and
    // back lighting so non-zero serialized defaults cannot activate them.
    float lightingEffect1;
    float lightingEffect2;
    float subsurfaceRolloff;
    float rimlightPower;
    float backlightPower;
    float fresnelPower;
    float grayscaleToPaletteScale;
    uint lightingMaskMapIndex;
    uint backLightingMapIndex;
};

layout(std430, set = 1, binding = 13) readonly buffer MaterialBuffer {
    GpuMaterial materials[];
};

// `GpuMaterial::material_flags` bit catalog. The active flags and
// shift constants (`MAT_FLAG_VERTEX_COLOR_EMISSIVE`, `_EFFECT_SOFT`,
// `_EFFECT_PALETTE_COLOR`, `_EFFECT_PALETTE_ALPHA`, `_EFFECT_LIT`,
// `MAT_FLAG_EFFECT_LI_SHIFT`) are `#define`d in
// `include/shader_constants.glsl` (the single source of truth,
// mirrored from `material_flag::*` in
// `crates/renderer/src/vulkan/material.rs`). See #1190.

// Material-feature flags. Bits 5-9 of `materialFlags` (PBR BSDF / SSS /
// model-space-normals suite) come from the generated
// `include/shader_constants.glsl` (`#include` at the top of this file),
// emitted by build.rs from `shader_constants_data.rs` and pinned by
// `generated_header_contains_all_defines` + `material_flag_bits_match_material_consts`.
// They were hand-written `#define`s here until #1285 — do NOT re-add
// them. Per `feedback_format_translation.md` the shader gates on
// material *features*, not source formats (the `BGSM_` prefix was
// dropped in the Stage 3 rollout).

struct GpuLight {
    vec4 position_radius;  // xyz = position, w = radius
    vec4 color_type;       // rgb=color, w=type (0=point, 1=spot, 2=directional)
    vec4 direction_angle;  // xyz = direction, w = spot angle cosine
    vec4 params;           // x = falloff, y = source radius, z = visibility bits, w = attenuation model
};

layout(std430, set = 1, binding = 0) readonly buffer LightBuffer {
    uint lightCount;
    uint _pad0, _pad1, _pad2;
    GpuLight lights[];
};

layout(set = 1, binding = 1) uniform CameraUBO {
    mat4 viewProj;
    mat4 prevViewProj;  // Previous frame's viewProj for motion vectors
    mat4 invViewProj;   // Precomputed inverse(viewProj) for world reconstruction
    vec4 cameraPos;   // xyz = world position, w = frame counter
    vec4 sceneFlags;  // x = RT enabled (1.0), yzw = ambient color (RGB)
    vec4 screen;      // x = width, y = height, z = fog near, w = fog far
    vec4 fog;         // xyz = fog color (RGB), w = fog enabled (1.0)
    vec4 jitter;      // xy = sub-pixel TAA jitter in NDC, z = bitcast<f32>(render_debug_flags), w = is_exterior flag (1.0 = exterior cell, 0.0 = interior). #1125.
    // #925 / REN-D15-NEW-03 — mirror of composite's `sky_zenith.xyz`
    // (linear RGB). Used by the window-portal escape below so
    // interior windows transmit a sky tint that matches whatever
    // `compute_sky` paints behind the world (TOD / weather cross-fade
    // already wired upstream). Pre-fix the portal site hardcoded
    // `vec3(0.6, 0.75, 1.0)` and every window looked clear-noon.
    vec4 skyTint;     // xyz = TOD/weather zenith colour, w = sun_angular_radius (rad; SkyParams::sun_angular_radius, #1023)
    vec4 sunDirection;
    vec4 dofParams;      // x = aperture half-radius (0.0 = pinhole), y = focus_dist, z = atten knee frac, w = camera_static (1.0 = parked).
    vec4 renderOrigin;   // #markarth-precision / #1496 — camera-relative render origin (cell-grid snapped). main() adds .xyz to the render-origin-relative `fragWorldPosRel` varying to reconstruct the absolute world position for lighting / RT / fog.
    uvec4 renderDebug;   // x = structured RENDER_DEBUG_* mode; y = optional bitcast RT LOD scale, z = LOD telemetry enable, w = packed weather surface (low 16 wetness, high 16 snow). Legacy feature-ablation flags remain in jitter.z.
    // #3323 — the EXTERIOR TOD/weather zenith colour, carried even on
    // interior cells. `skyTint` above is `SkyParams::default()` on any
    // interior by design (#1199 / #2226: an interior must never read a
    // stale exterior sky), which is right for every consumer except the
    // window-portal escape below, where the ray genuinely left the cell.
    // Read ONLY there; widening it anywhere else re-opens #2226.
    vec4 exteriorSkyTint; // xyz = live exterior zenith colour, w reserved (0)
};

layout(set = 1, binding = 2) uniform accelerationStructureEXT topLevelAS;

// Clustered lighting data (written by cluster_cull.comp each frame).
struct ClusterEntry {
    uint offset;
    uint count;
};

layout(std430, set = 1, binding = 5) readonly buffer ClusterGrid {
    ClusterEntry clusters[];
};

layout(std430, set = 1, binding = 6) readonly buffer ClusterLightIndices {
    uint clusterLightIndices[];
};

// SSAO texture (computed after the render pass, read next frame).
layout(set = 1, binding = 7) uniform sampler2D aoTexture;

// Soft-particle depth history: previous frame's opaque depth (non-linear,
// D32). Effect-shader (kind 101) FX feather their alpha against the geometry
// behind them so authored `soft = true` mist / steam / dust volumes dissolve
// at surfaces instead of showing hard box silhouettes. Copied from the depth
// buffer after the main pass (see `VulkanContext::copy_depth_to_history`).
layout(set = 1, binding = 15) uniform sampler2D depthHistoryTex;

// Global geometry SSBOs for RT reflection UV lookups.
//
// Vertex layout (104 B = 26 floats per vertex, mirrors Rust `Vertex`
// struct in `crates/renderer/src/vertex.rs`):
//
//   float offset │ bytes  │ field           │ type     │ safe-as-float?
//   ─────────────┼────────┼─────────────────┼──────────┼───────────────
//        0..2    │  0..11 │ position        │ vec3     │ ✓
//        3..6    │ 12..27 │ color           │ vec4     │ ✓ (RGBA)
//        7..9    │ 28..39 │ normal          │ vec3     │ ✓
//       10..11   │ 40..47 │ uv              │ vec2     │ ✓
//       12..15   │ 48..63 │ bone_indices    │ uvec4    │ ✗ u32 bits
//       16..19   │ 64..79 │ bone_weights    │ vec4     │ ✓
//       20       │ 80..83 │ splat_weights_0 │ 4× u8    │ ✗ packed unorm
//       21       │ 84..87 │ splat_weights_1 │ 4× u8    │ ✗ packed unorm
//       22..25   │ 88..103│ tangent (#783)  │ vec4     │ ✓ (xyz + sign)
//
// **WARNING (#575 / SH-1)**: only float offsets 0..11, 16..19, and
// 22..25 may be read directly as `vertexData[base + N]`. Bone indices
// (12..15) and splat weights (20..21) are NOT IEEE-754 floats —
// reinterpreting their bit patterns silently produces NaN / denormal
// garbage.
//
// To recover the unsafe slots, use the same pattern
// `skin_vertices.comp:101-106` uses for bone indices:
//   `uvec4 idx = uvec4(floatBitsToUint(vertexData[base + 12]), …);`
//
// or for splat unorms (4× u8 packed into one float-aliased u32):
//   `vec4 splat = unpackUnorm4x8(floatBitsToUint(vertexData[base + 20]));`
//
// The current RT hit shader reads positions, normals, UVs, and tangents
// only from the safe float lanes above. This comment is the pit-of-failure
// guardrail for future RT shader authors. The
// `rt_hit_shaders_have_no_unsafe_vertex_data_reads` test statically checks
// the source so the next forbidden read fails CI immediately.
layout(std430, set = 1, binding = 8) readonly buffer GlobalVertices {
    // flat array, stride = `VERTEX_STRIDE_FLOATS` floats (104 bytes).
    // The named const lives below so RT hit-fetch sites have one source of
    // truth for the vertex layout. See REN-D6-NEW-01 (audit 2026-05-09).
    float vertexData[];
};

// ── Vertex layout constants ──────────────────────────────────────────
//
// Mirror of the Rust `Vertex` struct's float-indexed layout (see the
// big comment block above the `GlobalVertices` SSBO). Pulled out to
// file scope so every RT hit-fetch site — `getHitUV` and any future
// hit-shader code — reads from the same named source. Pre-fix
// `getHitUV` once carried its own local stride literal (REN-D6-
// NEW-01); the inline literal worked but each new hit-fetch site
// VERTEX_STRIDE_FLOATS plus the position/normal/UV/tangent offsets come from
// shader_constants.glsl.
layout(std430, set = 1, binding = 9) readonly buffer GlobalIndices {
    uint indexData[];
};

// Per-terrain-tile bindless texture indices for LAND splat layers
// (#470). Fragment shader reads `terrainTiles[tileIdx]` when the
// `INSTANCE_FLAG_TERRAIN_SPLAT` bit (flags bit 3) is set. The tile
// index is packed into the top 16 bits of `flags`.
struct GpuTerrainTile {
    uint layerDiffuseIndex[8];
    uint layerNormalIndex[8];
    uint layerSpecularIndex[8];
};
// Binding 11: adaptive RT quality + glass-work telemetry. The CPU zeroes the
// first word before each render pass; Phase-3 IOR glass fragments atomically
// add their estimated query cost. `qualityTier` selects bounded loop limits
// coherently for the whole frame. Never use the unordered counter return as a
// per-fragment IOR admission gate: alpha glass has no temporal history, so
// atomic winners/losers become a permanent stipple.
layout(std430, set = 1, binding = 11) coherent buffer RayBudgetBuffer {
    uint rayBudgetCount;
    uint glassRayLimit;
    uint directShadowSamples;
    uint maxPathSegments;
    uint maxShadedHits;
    uint volumetricLightCap;
    uint qualityTier;
    uint _rayBudgetReserved;
    uint rtLodFragments;
    uint rtLodBin0;
    uint rtLodBin1;
    uint rtLodBin2;
    uint rtLodBin3;
    uint rtReflectionTraced;
    uint rtReflectionLodCulled;
    uint rtGiTraced;
    uint rtGiLodCulled;
} rayBudget;

layout(std430, set = 1, binding = 10) readonly buffer TerrainTileBuffer {
    GpuTerrainTile terrainTiles[];
};

// 6-axis directional ambient cube (Skyrim WTHR.DALC, per-TOD-lerped on
// the host). `dalcFlags.x == 1.0` when the cube is authored (Skyrim
// cells); zero means fall back to the legacy AMBIENT_AO_FLOOR path so
// FNV / FO3 / Oblivion exteriors render unchanged. Each axis vec4
// stores RGB in xyz with `.w` reserved for padding. #993 / REN-AMBIENT-DALC.
layout(set = 1, binding = 14) uniform DalcCubeUBO {
    vec4 dalcPosX;
    vec4 dalcNegX;
    vec4 dalcPosY;     // engine +Y = sky-fill
    vec4 dalcNegY;     // engine -Y = ground-bounce / cavity-fill
    vec4 dalcPosZ;
    vec4 dalcNegZ;
    vec4 dalcSpecularFresnel; // xyz = specular tint, w = fresnel power
    vec4 dalcFlags;           // x = enabled (0/1), yzw = reserved
};

// ── ReSTIR-DI direct-shadow reservoirs (Bitterli 2020) ──────────────
// One reservoir per screen pixel, indexed `pixelY * screenWidth + pixelX`.
// Persisted across frames as a ping-pong pair of per-frame-in-flight
// SSBOs: `reservoirsCurr` (this frame's write) + `reservoirsPrev` (last
// frame's read, the temporal source). 32 B/reservoir. The temporal reuse
// reprojects the previous reservoir via the motion vector (packed surface ID
// + normal rejection, mirroring svgf_temporal.comp) so the soft-shadow
// estimate accumulates effective samples across frames instead of
// re-randomising every frame (the un-denoised WRS crawl). Gated by
// DBG_DISABLE_RESTIR; the legacy per-frame WRS path stays compiled for A/B.
struct Reservoir {
    uint  lightAndSurface; // low 10b light index, high 22b surface ID
    float W;           // unbiased contribution weight (w_sum / (M * pHat))
    float M;           // effective sample count (capped)
    float histLenAndDepth; // bitcast packHalf2x16(history length, camera distance)
    float accumR;      // accumulated direct-shadow radiance — R
    float accumG;      // accumulated direct-shadow radiance — G
    float accumB;      // accumulated direct-shadow radiance — B
    float pad0;        // geometric normal: octEncode → packSnorm2x16 → float
                       // bits. Consumed by temporal + spatial rejection;
                       // keeps the struct at 32 B.
};

layout(std430, set = 1, binding = 16) buffer ReservoirCurrBuffer {
    Reservoir reservoirsCurr[];
};
layout(std430, set = 1, binding = 17) readonly buffer ReservoirPrevBuffer {
    Reservoir reservoirsPrev[];
};

// One bounded selected-light visibility-ray record per frame-in-flight.
// control.y is atomically claimed by the first eligible invocation at the
// requested pixel: 0=disabled, 1=armed, 2=claimed, 3=ready.
layout(std430, set = 1, binding = 19) coherent buffer SelectedRayProbeBuffer {
    uvec4 selectedRayProbeControl;       // generation, state, pixel x, pixel y
    uvec4 selectedRayProbeIds;           // light index, mask, hit instance, flags
    vec4 selectedRayProbeOriginTMin;     // origin.xyz, tMin
    vec4 selectedRayProbeDirectionTMax;  // direction.xyz, tMax
    vec4 selectedRayProbeHitVisibility;  // hit distance, averaged visibility.rgb
    vec4 selectedRayProbeLightPositionRadius;
    vec4 selectedRayProbeLightColorType;
    vec4 selectedRayProbeLightDirectionAngle;
    vec4 selectedRayProbeLightParams;
};
