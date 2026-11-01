// Descriptor set 0 contract shared by every shader that evaluates the
// volumetric combustion model (tracking #6531, Phase 1 #6533): the froxel
// inject pass and, from stage 1b, the world-space local-grid sim pass bind the
// SAME set, so the declarations live in one place and cannot drift.
//
// Needs, before the include: shader_constants.glsl (MAX_LIGHTS) and the
// shader's own `#extension GL_EXT_ray_query`. Moved verbatim from
// volumetrics_inject.comp.
#ifndef BYRO_VOLUMETRICS_BINDINGS_GLSL
#define BYRO_VOLUMETRICS_BINDINGS_GLSL

layout(set = 0, binding = 0, rgba16f) uniform writeonly image3D froxel;

layout(set = 0, binding = 1) uniform VolumetricsParams {
    mat4 inv_view_proj;
    mat4 prev_view_proj;
    // xyz = camera world position, w = extinction coefficient (1 / world unit)
    vec4 camera_pos;
    // xyz = previous camera world position, w = history-valid flag
    vec4 prev_camera_pos;
    // xyz = direction TO the sun (world space, unit length — matches
    // GpuLight.direction_angle / GpuCamera.sun_direction and every other
    // consumer in the renderer, #1937), w = HG phase function asymmetry
    // parameter g in (-1, 1).
    vec4 sun_dir;
    // rgb = sun radiance (already multiplied by intensity), a = the cell's
    // XCLL fog-far distance. NOT unused: the local-light loop below reads
    // `.a` as the `clusterFar` basis so this pass computes the same cluster
    // index `cluster_cull.comp` did from its own `screen.w` — see the
    // `clusterFar` computation and the Rust-side `VolumetricsParams::sun_color`
    // doc, which is the authority on this field. (#3831)
    vec4 sun_color;
    // x = volume far plane, y = 5 m linear floor, z = fraction of slices
    // assigned to that floor, w = frame index. Distances are world units.
    vec4 volume_params;
    // #markarth-precision — xyz = camera-relative render origin; added to the
    // inv_view_proj-reconstructed far point to recover ABSOLUTE froxel world
    // positions (the TLAS shadow rays below are cast in absolute space).
    // w = sky-access bit (1.0 exterior or Show Sky interior, 0.0 sealed
    // interior) — selects the shadow-ray strategy below.
    vec4 render_origin;
    // x = single-scatter albedo, y = backward HG g,
    // z = forward-lobe mix, w = exponential height scale.
    vec4 medium_params;
    // rgb = normalized authored CELL/WTHR fog chromaticity. Multiplying by
    // medium_params.x yields spectral single-scatter albedo while keeping
    // extinction and the authored peak scattering energy independent of hue.
    // a = sanitized authored fog-colour peak. rgb * a reconstructs the
    // apparent fog radiance used by the homogeneous equilibrium source term;
    // without it, sunless interiors only absorb light and their procedural
    // fog is effectively invisible.
    vec4 fog_tint;
    // x = history weight, y = density-change rejection strength,
    // z = total time seconds, w = procedural coverage.
    vec4 temporal_params;
    // xyz = minimum corner of the camera-centered local-volume cluster cube;
    // w = reciprocal cluster-cell size.
    vec4 local_volume_grid;
    // x = height-fog reference altitude (REN-D16-01 / #2225), absolute
    // world space matching camera_pos/render_origin — ground height near
    // the camera, or camera Y as a fallback. y = emissive history weight,
    // z = adaptive local-light cap, w = clamped simulation delta seconds.
    // w = -1 clears expired transport; -2 means this slot is already empty.
    vec4 fog_reference;
    // xy = normalized horizontal wind direction in renderer X/Z space,
    // z = base speed and w = gust amplitude, in world units per second.
    vec4 wind_params;
    // x = gust cycles per second. y = BFECC advection error-correction
    // multiplier — the host's BYRO_BFECC same-binary A/B switch (1.0 on,
    // 0.0 off; multiplied into COMBUSTION_BFECC_ERROR_CORRECTION_STRENGTH).
    // Remaining lanes are reserved.
    vec4 wind_gust;
} params;

// Phase 2c — scene TLAS. Updated each frame from the host side via
// `VolumetricsPipeline::write_tlas` before dispatch. Used solely for
// shadow visibility from sample → sun; no closest-hit shading is
// needed because sun radiance is constant.
layout(set = 0, binding = 2) uniform accelerationStructureEXT topLevelAS;

#include "include/shadow_common.glsl"

// Phase 2b — point/spot light in-scattering. Bindings 3/4/5 mirror the
// SAME per-frame lights SSBO + cluster grid + light-index list
// `cluster_cull.comp` builds for the fragment shader (struct layouts
// copied from `include/bindings.glsl`'s `GpuLight` / `ClusterEntry` —
// not `#include`d wholesale since that file's bindings are numbered
// for the fragment shader's own descriptor set, not this compute
// shader's). Written per-frame via
// `VolumetricsPipeline::write_lights_and_clusters`.
struct GpuLight {
    vec4 position_radius; // xyz = position, w = radius
    vec4 color_type;       // rgb = color, w = type (0=point, 1=spot, 2=directional)
    vec4 direction_angle;  // xyz = direction, w = spot angle cosine
    vec4 params;           // x = falloff, y = source radius, z = visibility bits, w = attenuation model
};

layout(std430, set = 0, binding = 3) readonly buffer LightBuffer {
    uint lightCount;
    uint _pad0, _pad1, _pad2;
    uint previousLightToCurrent[MAX_LIGHTS + 1u];
    GpuLight lights[];
};

struct ClusterEntry {
    uint offset;
    uint count;
};

layout(std430, set = 0, binding = 4) readonly buffer ClusterGrid {
    ClusterEntry clusters[];
};

layout(std430, set = 0, binding = 5) readonly buffer ClusterLightIndices {
    uint clusterLightIndices[];
};

// Previous frame's raw V-buffer (rgb in-scattering, a extinction), sampled
// after projecting the jittered current froxel center through prev_view_proj.
layout(set = 0, binding = 6) uniform sampler3D previousFroxel;

// Spatially bounded authored media. The CPU transforms ECS primitives to
// absolute world space, sorts them near-to-far, and builds a camera-centered
// 16^3 fixed-capacity cluster list every frame. This keeps the per-froxel loop
// bounded even in scenes with many legacy smoke systems.
struct GpuFogVolume {
    vec4 center_shape;             // xyz center, w 0 sphere / 1 ellipsoid / 2 box / 3 cone
    vec4 half_extents_extinction;  // xyz half extents, cone x/z bottom/top radius; w sigma_t / world unit
    vec4 inverse_rotation;         // world-to-local quaternion
    vec4 albedo_edge;              // rgb single-scatter albedo, w edge softness
    vec4 emission_temperature;     // rgb emitted radiance L_e, w blackbody K
    vec4 profile_params;           // x profile; y explosion age; z lifetime; w explicit sky aperture
};

layout(std430, set = 0, binding = 7) readonly buffer FogVolumeBuffer {
    uint fogVolumeCount;
    uint fogRayQualityTier;
    uint _fogPad1, _fogPad2;
    GpuFogVolume fogVolumes[];
};

struct FogClusterEntry {
    uint offset;
    uint count;
    uint portalOffset;
    uint portalCount;
};

layout(std430, set = 0, binding = 8) readonly buffer FogClusterBuffer {
    FogClusterEntry fogClusters[];
};

layout(std430, set = 0, binding = 9) readonly buffer FogClusterIndexBuffer {
    uint fogClusterIndices[];
};

layout(set = 0, binding = 10) uniform sampler3D baseDensityNoise;
layout(set = 0, binding = 11) uniform sampler3D detailDensityNoise;

// Current and previous deterministic emissive share. The raw V-buffer cannot
// reconstruct this after emission, authored fog, and stochastic lighting have
// been folded into one RGB source. Keeping one scalar history channel lets a
// departing flame retain the same short temporal time constant as its leading
// edge without treating binary shadow-ray changes as disocclusions. #2809.
layout(r32f, set = 0, binding = 12) uniform writeonly image3D emissionHistory;
layout(set = 0, binding = 13) uniform sampler3D previousEmissionHistory;

// Canonical transported combustion state. These fields deliberately contain
// no source-game identity: per-game particle/material authoring has already
// converged to GpuFogVolume::profile_params at the import boundary.
//
// chemistry = (fuel mass fraction, temperature K, total extinction sigma_t/world-unit,
//              visible-radiance calibration at 1850 K)
// dynamics  = (world-space velocity xyz in world-units/second,
//              specific overpressure in m²/s²)
// optical   = (spectral scattering sigma_s.rgb/world-unit, reserved zero)
//
// Current and previous slots are distinct frame-in-flight images, so this
// shader can semi-Lagrangian backtrace the old field while writing the new
// one without an intra-dispatch data hazard.
layout(rgba16f, set = 0, binding = 14) uniform writeonly image3D combustionState;
layout(set = 0, binding = 15) uniform sampler3D previousCombustionState;
layout(rgba16f, set = 0, binding = 16) uniform writeonly image3D combustionDynamics;
layout(set = 0, binding = 17) uniform sampler3D previousCombustionDynamics;
layout(rgba16f, set = 0, binding = 22) uniform writeonly image3D combustionOptical;
layout(set = 0, binding = 23) uniform sampler3D previousCombustionOptical;

// #4784 — coarse transport occupancy on the 16^3 fog-cluster grid, one bit
// per cell. Binding 24 is THIS frame's mask (the inject pass marks it with
// atomicOr wherever the state it writes carries combustion); binding 25 is
// the PREVIOUS frame's mask, written once per dispatch by the host (zeroed,
// then seed-marked for every cluster a transported-source volume spans) and
// by last frame's GPU marks. A froxel whose cell and its 26 neighbours were
// all empty last frame cannot receive transported mass this frame (per-step
// displacement is far below one cell), so it skips the RK2 backtrace /
// neighbour gather / midpoint-source probes entirely and falls through to
// the dt == 0 carry — the empty majority of the grid stops paying ~27
// trilinear history fetches per froxel whenever any emitter is active.
layout(std430, set = 0, binding = 24) buffer CombustionOccupancyOut {
    uint combustionOccupancyOut[];
};
layout(std430, set = 0, binding = 25) readonly buffer CombustionOccupancyIn {
    uint combustionOccupancyIn[];
};

// Coarse radiant moments of the *transported* field, consumed by the host on
// the next safe reuse of this frame-in-flight slot. This is the canonical
// bridge from participating-medium emission to surface lighting: no source
// primitive or source-game identity is visible here. Each 32-byte bin stores
// fixed-point total luma, luma-weighted bin-local centroid, RGB radiant
// intensity, and luminous volume. Rust owns the matching repr(C) layout.
struct CombustionLightMoment {
    uint weight;
    uint weighted_x;
    uint weighted_y;
    uint weighted_z;
    uint radiant_r;
    uint radiant_g;
    uint radiant_b;
    uint luminous_volume;
};

layout(std430, set = 0, binding = 18) buffer CombustionLightMomentBuffer {
    CombustionLightMoment combustionLightMoments[];
};

// Geometry backing the current TLAS. `instance_custom_index` is built from
// the same compacted index as this per-frame instance SSBO, so a committed
// ray-query hit can recover its triangle without a second scene map. Only the
// prefix needed for rigid boundary normals is named; the `_boundaryTail*`
// words cover the u64 skinned-vertex address, the two u64 morph-target
// addresses, the morph-target count and their reserved padding, preserving
// the exact 160-byte std430 stride of the canonical Rust `GpuInstance`.
//
// This is a deliberate prefix-mirror under its own name, so it sits outside
// `gpu_instance_glsl_copies_stay_in_lockstep`'s SOURCES list (which keys on
// the literal `struct GpuInstance`). The stride is instead pinned by
// `gpu_boundary_instance_stride_matches_gpu_instance` — the tail exists only
// to make the stride right, so agreeing on the total size is the whole
// contract. Growing `GpuInstance` without growing the tail here misaligns
// every read past instance index 0 (#3829).
struct GpuBoundaryInstance {
    mat4 model;
    uint textureIndex;
    uint boneOffset;
    uint vertexOffset;
    uint indexOffset;
    uint vertexCount;
    uint flags;
    uint materialId;
    float ior;
    float avgAlbedoR;
    float avgAlbedoG;
    float avgAlbedoB;
    uint surfaceId;
    // 112..128 — u64 skinned-vertex address + two reserved words.
    uvec4 _boundaryTail0;
    // 128..144 — u64 morph-delta address + u64 morph-weight address.
    uvec4 _boundaryTail1;
    // 144..160 — morph-target count + three reserved words.
    uvec4 _boundaryTail2;
};

layout(std430, set = 0, binding = 19) readonly buffer BoundaryInstanceBuffer {
    GpuBoundaryInstance boundaryInstances[];
};

layout(std430, set = 0, binding = 20) readonly buffer BoundaryVertexBuffer {
    float boundaryVertexData[];
};

layout(std430, set = 0, binding = 21) readonly buffer BoundaryIndexBuffer {
    uint boundaryIndexData[];
};

// ── World-space combustion local grid (tracking #6531, Phase 1 #6533) ─────
// 26: box table + sun for the close-range march (also bound by composite).
// 27/28/29: THIS frame's local-grid state / dynamics / optical atlas, written
// by `volumetrics_local_sim.comp` earlier in the same command buffer. A froxel
// inside a box fills its combustion volumes from these instead of simulating.
// 30/31: the froxel-resolution irradiance field the close-range march lights
// the fire with — written here, previous slot sampled for temporal smoothing.
#define LOCAL_GRID_PARAMS_BINDING 26
#include "include/local_grid_params.glsl"
layout(set = 0, binding = 27) uniform sampler3D localAtlasState;
layout(set = 0, binding = 28) uniform sampler3D localAtlasDynamics;
layout(set = 0, binding = 29) uniform sampler3D localAtlasOptical;
layout(set = 0, binding = 30, rgba16f) uniform writeonly image3D localIrradiance;
layout(set = 0, binding = 31) uniform sampler3D previousLocalIrradiance;

#endif
