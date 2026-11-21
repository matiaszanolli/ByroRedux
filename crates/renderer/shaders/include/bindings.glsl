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
// M39 — the chain level each `textures[]` slot's image starts at
// (`Texture::first_level`, written with the slot's descriptor). 0 for a whole
// texture; `first_resident` for a partly resident streamed one, whose view
// starts at that level of the authored chain.
layout(std430, set = 0, binding = 2) readonly buffer TextureFirstLevels {
    uint textureFirstLevel[];
};

// An explicit LOD counted from the full authored chain, as a LOD into the view
// of `textures[handle]`. Implicit-derivative and size-relative LODs need no
// help: they are computed from the view's own extent. An absolute level
// (roughness·8 reflection blur, the refraction and glass blurs) is not, and
// would read `first_level` levels coarser than the same texture fully
// resident. A level the view holds is reached exactly; a finer one clamps to
// the finest resident level. LOD 0 skips the table read.
float residentLod(uint handle, float chainLod) {
    if (chainLod <= 0.0) {
        return chainLod;
    }
    return max(chainLod - float(textureFirstLevel[handle]), 0.0);
}

#include "include/gpu_instance.glsl"

// The buffer_reference types. Requires `GL_EXT_buffer_reference` enabled in the
// including shader (triangle.frag / water.frag).
#include "include/buffer_references.glsl"

layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer {
    GpuInstance instances[];
};

// Current posed bone palette, also consumed by triangle.vert. Secondary-hit
// normal/tangent reconstruction must use the same pose as the skinned BLAS.
layout(std430, set = 1, binding = 3) readonly buffer BoneBuffer {
    mat4 bones[];
};

// ── R1 Phase 4: deduplicated material table ─────────────────────────
//
// Mirrors the Rust `GpuMaterial` (432 B std430) defined
// in `crates/renderer/src/vulkan/material.rs`. Indexed by
// `GpuInstance.materialId`. Phase 4 migrates one field (`roughness`)
// off the per-instance copy onto this path; Phases 5–6 do the rest
// and finally remove the redundant per-instance copies.
//
// **Shader Struct Sync**: any field added here must be added in
// lockstep to the Rust `GpuMaterial` struct + the matching
// `intern`/encoding sites; the size of this struct (432 B) is pinned by
// `gpu_material_size_is_432_bytes` on the Rust side.
//
// (#3909 shrank it 432 -> 428 by removing the unsampled `textureIndex` lane;
// #4422 then appended `detailNeutral`, 428 -> 432.)
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
    // Texture indices group A. #3909 — `textureIndex` (the diffuse / albedo
    // handle) used to lead this group; it was sampled by no shader (every
    // `textureIndex` read in `shaders/` comes from `inst`, never `mat`) and
    // is removed. Everything below shifts down by 4 bytes. Same precedent as
    // #804's `avgAlbedo*` removal a few fields down.
    uint normalMapIndex, darkMapIndex, glowMapIndex;
    // Texture indices group B
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
    // #804 / R1-N4 — `avgAlbedoR/G/B` (offsets 140-148) removed; no
    // shader read `mat.avgAlbedo*`. Subsequent fields shift down by 12.
    // skin_tint A/R/G/B (offsets 140-152)
    float skinTintA, skinTintR, skinTintG, skinTintB;
    // hair_tint RGB + multi_layer_envmap_strength (offsets 156-168)
    float hairTintR, hairTintG, hairTintB, multiLayerEnvmapStrength;
    // eye_left RGB + eye_cubemap_scale (offsets 172-184)
    float eyeLeftCenterX, eyeLeftCenterY, eyeLeftCenterZ, eyeCubemapScale;
    // eye_right RGB + multi_layer_inner_thickness (offsets 188-200)
    float eyeRightCenterX, eyeRightCenterY, eyeRightCenterZ, multiLayerInnerThickness;
    // refraction_scale + multi_layer_inner_scale UV + sparkle_r
    float multiLayerRefractionScale, multiLayerInnerScaleU, multiLayerInnerScaleV, sparkleR;
    // sparkle GB + sparkle_intensity + falloff_start
    float sparkleG, sparkleB, sparkleIntensity, falloffStartAngle;
    // falloff_stop + opacities + soft_falloff_depth
    float falloffStopAngle, falloffStartOpacity, falloffStopOpacity, softFalloffDepth;
    // #890 Stage 2c — bindless handle for
    // `BSEffectShaderProperty.greyscale_texture`. 0 = no LUT (the
    // shader's effect branch then samples the source texture raw).
    // Offset 252.
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
    // for legacy NIF content with no authored IOR. Offset 276.
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
    // #1249 — Disney diffuse lobe (offsets 280-288). subsurface
    // weights the Hanrahan-Krueger fake-SSS approximation against the
    // Burley diffuse; sheen + sheenTint drive the fabric-class edge
    // highlight. All zero by default → byte-identical Lambert
    // behaviour for legacy NIF content. Only consulted when
    // `MAT_FLAG_PBR_BSDF` is set. These complete the #1249 block at
    // offset 292; #1250 `anisotropic` then completed the prior 300 B layout.
    float subsurface;
    float sheen;
    float sheenTint;
    // #1250 — anisotropic GGX strength. 0 = isotropic
    // (ax = ay = roughness; distributionGGXAniso degenerates to the
    // legacy distributionGGX lobe shape). 1 = maximum anisotropy
    // capped at `aspect = sqrt(0.1)` so the lobe doesn't fully
    // degenerate into a needle.
    float anisotropic;
    // Common supplemental semantic texture roles (offsets 296-340).
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
    // Animated BSShaderProperty color/scalar (offsets 344-356, #2221).
    // Same "layout parity, no shader consumer yet" precedent as the
    // three unsampled map indices above — see the field notes in
    // crates/renderer/src/vulkan/material.rs.
    float shaderColorR, shaderColorG, shaderColorB; // unsampled — see #2221
    float shaderFloat;                              // unsampled — see #2221
    // BGEM v21+/v22 authored glass optics (offsets 360-388).
    float glassFresnelR, glassFresnelG, glassFresnelB;
    float glassRefractionScale;
    float glassBlurScale;
    float glassBlurScaleFactor;
    uint glassRoughnessScratchMapIndex;
    uint glassDirtOverlayMapIndex;
    // Bethesda-authored direct-light response and translated mask roles
    // (offsets 392-424). Feature flags independently gate soft, rim and
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
    // #4422 — producer-declared detail-combine neutral (offset 428).
    // The detail sample is bound as a raw UNORM view and divided by
    // this, in the encoded space the combine was authored in:
    // FaceTint declares 65/255 (vanilla's own blank detail texture),
    // everything else the classic MODULATE2X 128/255. Declared by the
    // NIFAL boundary; this struct carries no per-game branch.
    float detailNeutral;
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

#include "include/gpu_light.glsl"

layout(std430, set = 1, binding = 0) readonly buffer LightBuffer {
    uint lightCount;
    uint _pad0, _pad1, _pad2;
    uint previousLightToCurrent[MAX_LIGHTS + 1u];
    GpuLight lights[];
};

layout(set = 1, binding = 1) uniform CameraUBO {
#include "include/camera_ubo_members.glsl"
};

layout(set = 1, binding = 2) uniform accelerationStructureEXT topLevelAS;

// Clustered lighting data (written by cluster_cull.comp each frame).
#include "include/cluster_entry.glsl"

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

// SKYAL — the baked sky cubemap (`sky_cube.comp`), one per frame in
// flight. Holds the same analytic sky `composite.frag` paints the
// background with, evaluated per cube texel, so shading rays that escape
// the world can resolve a real sky instead of a single flat colour.
//
// Guarded by `exteriorSkyTint.w`: the bake is allowed to fail to
// initialise (VRAM pressure), and this binding is PARTIALLY_BOUND, so
// every read must be gated on the flag rather than on the binding
// existing.
layout(set = 1, binding = 20) uniform samplerCube skyCube;

#include "include/sky_sh.glsl"
layout(std430, set = 1, binding = 21) readonly buffer SkyDiffuseBuffer {
    vec4 skyDiffuseCoefficients[SKY_SH_COEFFICIENTS];
};

// Incident sky convolved with cosine / PI. Material diffuse weight and albedo
// are applied by the caller. Interiors retain their local authored ambient.
vec3 exteriorSkyDiffuseOr(vec3 normal, vec3 fallback) {
    if (jitter.w <= 0.5 || exteriorSkyTint.w <= 0.5) return fallback;
    vec3 value = vec3(0.0);
    for (uint i = 0u; i < uint(SKY_SH_COEFFICIENTS); ++i) value += skyDiffuseCoefficients[i].rgb * sky_sh_basis(i, normal);
    return max(value, vec3(0.0)); // band-limited SH can ring below zero
}

// The exterior sky radiance along `direction`: the baked cube when the bake
// is live, `fallback` otherwise. Every exterior sky-escape site reads the
// cube through here, so the ready-flag gate above lives in one place (#4292).
// Explicit LOD is essential in divergent ray paths, where derivatives are
// undefined. The mip chain encodes GGX roughness, not a screen footprint.
vec3 exteriorSkyRadianceOr(vec3 direction, vec3 fallback, float roughness) {
    if (exteriorSkyTint.w > 0.5) {
        float lod = clamp(roughness, 0.0, 1.0) * float(textureQueryLevels(skyCube) - 1);
        return textureLod(skyCube, direction, lod).rgb;
    }
    return fallback;
}

vec3 exteriorSkyRadianceOr(vec3 direction, vec3 fallback) {
    return exteriorSkyRadianceOr(direction, fallback, 0.0);
}

// SKYAL — the cloud-shadow map (`cloud_shadow.comp`): direct-sun
// transmittance through the sky's volumetric deck, per world position. Baked with the cube at binding 20, so it shares its
// `exteriorSkyTint.w` ready flag and PARTIALLY_BOUND contract.
layout(set = 1, binding = 24) uniform sampler2D cloudShadowMap;

#include "include/cloud_shadow.glsl"

// How much of the sun reaches `worldPos` (world units) through the clouds,
// for a directional light arriving from `toSun`. The sun's upload is the
// light ABOVE the clouds; every direct-sun site multiplies by this. 1.0 in
// interiors (their key light is the room's XCLL directional, not the sun)
// and while the bake is absent.
float exteriorCloudSunTransmittance(vec3 worldPos, vec3 toSun) {
    if (jitter.w <= 0.5 || exteriorSkyTint.w <= 0.5) return 1.0;
    vec3 receiver = worldPos / WORLD_UNITS_PER_METER;
    vec3 viewer = cameraPos.xyz / WORLD_UNITS_PER_METER;
    vec2 crossing = cloud_shadow_base_crossing(receiver.xz, receiver.y - viewer.y, toSun);
    vec2 uv = cloud_shadow_uv(crossing, cloud_shadow_map_centre(viewer.xz));
    // The map covers CLOUD_SHADOW_MAP_EXTENT around the viewer and knows no
    // local cloud beyond it. The bake fades its outer tenth to the uniform
    // overcast term alone (`cloud_shadow_edge_weight`), so the clamped sampler
    // returns that term there rather than smearing a local shadow.
    return textureLod(cloudShadowMap, uv, 0.0).r;
}

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

#include "include/terrain_tile.glsl"

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
    uint shadowFilterIterations; // PAF-007; CPU-consumed, word kept for layout
    uint cacheUpdateStride;      // RCG-UPD-001: one cache update path per stride x stride pixel block; 0 = none
    uint rtLodFragments;
    uint rtLodBin0;
    uint rtLodBin1;
    uint rtLodBin2;
    uint rtLodBin3;
    uint rtReflectionTraced;
    uint rtReflectionLodCulled;
    uint rtGiTraced;
    uint rtGiLodCulled;
    uint rtCacheResolved;        // RCG-DBG-002: traced-share pixels the radiance cache resolved (telemetry; 0 until a lookup exists)
    uint rtCacheMisses;          // RCG-DBG-002 / RCG-COV-003: lookups that missed and fell back for the frame
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
// frame's read, the temporal source). 36 B/reservoir. The temporal reuse
// reprojects the previous reservoir via the motion vector (packed surface ID
// + normal rejection, mirroring svgf_temporal.comp) so the soft-shadow
// estimate accumulates effective samples across frames instead of
// re-randomising every frame (the un-denoised WRS crawl). Gated by
// DBG_DISABLE_RESTIR; the legacy per-frame WRS path stays compiled for A/B.
#include "include/reservoir.glsl"

// writeonly (#5062): triangle.frag only ever stores this frame's reservoirs;
// reads come through ReservoirPrevBuffer next frame. Keeps sync validation
// from classifying binding 16 as a fragment SHADER_STORAGE_READ interface.
// Never load through it (#5765): which fragment may store is decided by the
// claim word at binding 22, not by reading the slot back.
layout(std430, set = 1, binding = 16) writeonly buffer ReservoirCurrBuffer {
    Reservoir reservoirsCurr[];
};
layout(std430, set = 1, binding = 17) readonly buffer ReservoirPrevBuffer {
    Reservoir reservoirsPrev[];
};
// #5765 — per-pixel ownership claim for this frame's reservoir slot: the
// float bits of the nearest claiming fragment's camera distance (positive
// floats order as their bits). `begin_frame` fills it with 0xFFFFFFFF; a
// fragment stores its reservoir only when atomicMin reports it is at least as
// near as every earlier claimant. Lives in the tail of the same per-frame
// buffer as binding 16 (`ReservoirBuffers::claim_offset`).
layout(std430, set = 1, binding = 22) buffer ReservoirClaimBuffer {
    uint reservoirClaim[];
};
// The claim winner's unshadowed direct radiance U (restirUnshadowedSum), the
// factor its shadow ratio multiplies, packed as
// uvec2(packHalf2x16(U.rg), packHalf2x16(vec2(U.b, 0.0))). The shadow filter
// pass reads it beside the reservoir to correct direct light by
// U * (filteredRatio - rawRatio). Written only with the reservoir, so a slot
// whose reservoir is zero-filled carries a stale U the pass never reads.
layout(std430, set = 1, binding = 23) writeonly buffer DirectUnshadowedBuffer {
    uvec2 directUnshadowed[];
};
// #6250 — this build's moved stable surface IDs, small enough to scan per
// reprojection: element [0] is the count N (clamped to
// RESTIR_MAX_MOVED_SURFACES by the uploader), [1..N] the surface IDs of
// every instance flagged INSTANCE_FLAG_MOVED_THIS_FRAME. The mover taint
// matches last frame's recorded occluder (low bits of `occluderAndShadowSequence`)
// against it, because instance indices are per-frame sort order and cannot
// be resolved across frames — surface IDs are stable. Partially bound is
// not relied on: the CPU writes every frame slot at init.
layout(std430, set = 1, binding = 25) readonly buffer MovedSurfaceSet {
    uint movedSurfaceSet[];
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
