// Bindless texture array + GPU structs + SSBO/UBO bindings + TLAS + vertex-layout constants
//
// NON-STANDALONE shader fragment. Included by triangle.frag in dependency
// order via GL_GOOGLE_include_directive; it references symbols (structs,
// SSBO/UBO bindings, helper functions, constants) defined in shader_constants.glsl
// and in earlier includes. Do not compile on its own.

// Bindless texture array.
layout(set = 0, binding = 0) uniform sampler2D textures[];

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
    float _padId0;         // offset 92
    float avgAlbedoR;      // offset 96 — kept for caustic_splat.comp (set 0 reads, not migrated)
    float avgAlbedoG;      // offset 100
    float avgAlbedoB;      // offset 104
    float _padAlbedo;      // offset 108 → total 112
};

layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer {
    GpuInstance instances[];
};

// ── R1 Phase 4: deduplicated material table ─────────────────────────
//
// Mirrors the Rust `GpuMaterial` (300 B std430) defined
// in `crates/renderer/src/vulkan/material.rs`. Indexed by
// `GpuInstance.materialId`. Phase 4 migrates one field (`roughness`)
// off the per-instance copy onto this path; Phases 5–6 do the rest
// and finally remove the redundant per-instance copies.
//
// **Shader Struct Sync**: any field added here must be added in
// lockstep to the Rust `GpuMaterial` struct + the matching
// `intern`/encoding sites; the size of this struct (300 B — was 260 B
// post-#804, grew through the #1248–#1250 Disney BSDF additions) is
// pinned by `gpu_material_size_is_300_bytes` on the Rust side.
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
    float ior;
    // #1249 — Disney diffuse lobe (offsets 284-292). subsurface
    // weights the Hanrahan-Krueger fake-SSS approximation against the
    // Burley diffuse; sheen + sheenTint drive the fabric-class edge
    // highlight. All zero by default → byte-identical Lambert
    // behaviour for legacy NIF content. Only consulted when
    // `MAT_FLAG_PBR_BSDF` is set. These complete the #1249 block at
    // offset 296; #1250 `anisotropic` then extends the struct to 300 B.
    float subsurface;
    float sheen;
    float sheenTint;
    // #1250 — anisotropic GGX strength. 0 = isotropic
    // (ax = ay = roughness; distributionGGXAniso degenerates to the
    // legacy distributionGGX lobe shape). 1 = maximum anisotropy
    // capped at `aspect = sqrt(0.1)` so the lobe doesn't fully
    // degenerate into a needle. Closes the 300 B struct.
    float anisotropic;
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
    vec4 color_type;       // rgb = color, w = type (0=point, 1=spot, 2=directional)
    vec4 direction_angle;  // xyz = direction, w = spot angle cosine
    vec4 params;           // x = falloff exponent, y = emitter radius; zw reserved
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
// Vertex layout (100 B = 25 floats per vertex, mirrors Rust `Vertex`
// struct in `crates/renderer/src/vertex.rs`):
//
//   float offset │ bytes  │ field           │ type     │ safe-as-float?
//   ─────────────┼────────┼─────────────────┼──────────┼───────────────
//        0..2    │  0..11 │ position        │ vec3     │ ✓
//        3..5    │ 12..23 │ color           │ vec3     │ ✓
//        6..8    │ 24..35 │ normal          │ vec3     │ ✓
//        9..10   │ 36..43 │ uv              │ vec2     │ ✓
//       11..14   │ 44..59 │ bone_indices    │ uvec4    │ ✗ u32 bits
//       15..18   │ 60..75 │ bone_weights    │ vec4     │ ✓
//       19       │ 76..79 │ splat_weights_0 │ 4× u8    │ ✗ packed unorm
//       20       │ 80..83 │ splat_weights_1 │ 4× u8    │ ✗ packed unorm
//       21..24   │ 84..99 │ tangent (#783)  │ vec4     │ ✓ (xyz + sign)
//
// **WARNING (#575 / SH-1)**: only float offsets 0..10, 15..18, and
// 21..24 may be read directly as `vertexData[base + N]`. Bone indices
// (11..14) and splat weights (19..20) are NOT IEEE-754 floats —
// reinterpreting their bit patterns silently produces NaN / denormal
// garbage.
//
// To recover the unsafe slots, use the same pattern
// `skin_vertices.comp:101-106` uses for bone indices:
//   `uvec4 idx = uvec4(floatBitsToUint(vertexData[base + 11]), …);`
//
// or for splat unorms (4× u8 packed into one float-aliased u32):
//   `vec4 splat = unpackUnorm4x8(floatBitsToUint(vertexData[base + 19]));`
//
// The current RT hit shader (`getHitUV` below) only reads UV at
// offsets 9..10 and is safe; this comment is the pit-of-failure
// guardrail for future RT shader authors. The
// `triangle_frag_no_unsafe_vertex_data_reads` test (scene_buffer.rs)
// statically grep-checks the source so the next forbidden read
// fails CI immediately.
layout(std430, set = 1, binding = 8) readonly buffer GlobalVertices {
    // flat array, stride = `VERTEX_STRIDE_FLOATS` floats (100 bytes) — #783.
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
// `getHitUV` carried its own local `const uint STRIDE = 25;` (REN-D6-
// NEW-01); the inline literal worked but each new hit-fetch site
// VERTEX_STRIDE_FLOATS and VERTEX_UV_OFFSET_FLOATS from shader_constants.glsl.
layout(std430, set = 1, binding = 9) readonly buffer GlobalIndices {
    uint indexData[];
};

// Per-terrain-tile bindless texture indices for LAND splat layers
// (#470). Fragment shader reads `terrainTiles[tileIdx]` when the
// `INSTANCE_FLAG_TERRAIN_SPLAT` bit (flags bit 3) is set. The tile
// index is packed into the top 16 bits of `flags`.
struct GpuTerrainTile {
    uint layerTextureIndex[8];
};
// Binding 11: RT mipmap glass ray budget counter. The CPU zeroes this
// before each render pass; Phase-3 IOR glass fragments atomically increment
// it. Once the count exceeds GLASS_RAY_BUDGET the fragment falls back to
// the cheaper Fresnel-highlight path for the rest of that frame.
layout(std430, set = 1, binding = 11) coherent buffer RayBudgetBuffer {
    uint rayBudgetCount;
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
// reprojects the previous reservoir via the motion vector (mesh_id +
// normal rejection, mirroring svgf_temporal.comp) so the soft-shadow
// estimate accumulates effective samples across frames instead of
// re-randomising every frame (the un-denoised WRS crawl). Gated by
// DBG_DISABLE_RESTIR; the legacy per-frame WRS path stays compiled for A/B.
struct Reservoir {
    uint  lightIndex;  // selected light index — temporal SELECTION reuse
    float W;           // unbiased contribution weight (w_sum / (M * pHat))
    float M;           // effective sample count (capped)
    float histLen;     // EMA history length for the accumulated radiance
    float accumR;      // accumulated direct-shadow radiance — R
    float accumG;      // accumulated direct-shadow radiance — G
    float accumB;      // accumulated direct-shadow radiance — B
    float pad0;        // geometric normal: octEncode → packSnorm2x16 → float
                       // bits. Consumed by spatial-reuse neighbour rejection
                       // (ReSTIR P2, Bitterli §5); keeps the struct at 32 B.
};

layout(std430, set = 1, binding = 16) buffer ReservoirCurrBuffer {
    Reservoir reservoirsCurr[];
};
layout(std430, set = 1, binding = 17) readonly buffer ReservoirPrevBuffer {
    Reservoir reservoirsPrev[];
};
