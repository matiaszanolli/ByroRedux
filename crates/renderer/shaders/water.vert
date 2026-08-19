#version 460
#extension GL_GOOGLE_include_directive : require
#extension GL_EXT_nonuniform_qualifier : require
// REN-2026-07-28-02 / #2219 — GpuInstance grew a uint64_t member; this
// mirror never dereferences it, but the type must be recognized to keep
// the struct byte-for-byte identical to the other 4 declaration sites.
#extension GL_EXT_buffer_reference : require
#extension GL_ARB_gpu_shader_int64 : require

#include "include/shader_constants.glsl"

// ── Water surface vertex shader ───────────────────────────────────────
//
// Drives:
//   • Calm lakes / pools             (`WaterKind::Calm`)
//   • Rivers and canals              (`WaterKind::River`)
//   • Rapids / whitewater            (`WaterKind::Rapids`)
//   • Waterfall sheets               (`WaterKind::Waterfall`)
//
// The water mesh is authored as a flat quad in mesh-local space. The
// raster path applies a bounded two-wave vertical displacement below;
// water remains excluded from the TLAS, so no per-frame BLAS rebuild is
// required. The fragment path adds higher-frequency normal detail.
//
// Inputs reuse the engine `Vertex` layout exactly so the renderer
// can share its global vertex SSBO + index buffer with the rest of
// the world. Bone indices / weights / splat weights are unused by
// water draws (they're authored zero on the water quad meshes); we
// keep the attribute slots wired so `triangle.vert`'s VAO is
// reusable without a second `VertexInputState`.

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec4 inColor;       // unused on water — kept for VAO compat
layout(location = 2) in vec3 inNormal;
layout(location = 3) in vec2 inUV;
layout(location = 4) in uvec4 inBoneIndices; // unused
layout(location = 5) in vec4  inBoneWeights; // unused
layout(location = 6) in vec4 inSplat0;       // unused
layout(location = 7) in vec4 inSplat1;       // unused
layout(location = 8) in vec4 inTangent;      // xyz = tangent, w = bitangent sign

// ── Per-instance SSBO (shared with triangle pipeline) ─────────────────
// We only consume `model`; the rest of the GpuInstance fields are
// not driven by the water material path (which lives in push
// constants — see water.frag). Layout must match the Rust struct
// at `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`
// byte-for-byte — the 112-byte invariant is pinned by
// `gpu_instance_is_112_bytes_std430_compatible` at
// `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`.
// Path moved from `vulkan/instance.rs` during the Session-34 split (#1187).
struct GpuInstance {
    mat4 model;
    uint textureIndex;
    uint boneOffset;
    uint vertexOffset;
    uint indexOffset;
    uint vertexCount;
    uint flags;
    uint materialId;
    float ior;             // offset 92 — per-draw optical IOR (read by caustic_splat.comp)
    float avgAlbedoR;
    float avgAlbedoG;
    float avgAlbedoB;
    uint surfaceId;
    uint64_t skinnedVertexAddress; // offset 112 — #2219, unused here
    uvec2 _reserved;               // offset 120 -> total 128
};

layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer {
    GpuInstance instances[];
};

layout(set = 1, binding = 1) uniform CameraUBO {
    mat4 viewProj;
    mat4 prevViewProj;
    mat4 invViewProj;
    vec4 cameraPos;
    vec4 sceneFlags;
    vec4 screen;
    vec4 fog;
    vec4 jitter;
    vec4 skyTint;
    vec4 sunDirection;
    vec4 dofParams;      // x = aperture half-radius (0.0 = pinhole), y = focus_dist, z = atten knee frac, w = camera_static (1.0 = parked).
    vec4 renderOrigin;   // #markarth-precision — xyz = camera-relative render origin; add to worldPos_rel for the absolute world position. w = FSR one-frame-reset flag (NOT padding — #2164/L-10).
    uvec4 renderDebug;   // x = structured RENDER_DEBUG_* mode; yzw reserved.
};

// Set 2 binding 1 is shared with water.frag. Vertex visibility is enabled
// so authored amplitude/frequency can affect the silhouette as well as the
// fragment normal, while the indexed record keeps the push block compact.
struct WaterParams {
    vec4 timing;
    vec4 flow;
    vec4 shallow;
    vec4 deep;
    vec4 scroll;
    vec4 tune;
    vec4 misc;
    vec4 tint_reflect;
    uvec4 noise_indices;
};
layout(std140, set = 2, binding = 1) uniform WaterParamsBlock {
    WaterParams params[256];
} waterParams;

layout(push_constant) uniform WaterDrawPush {
    uint waterIndex;
    uvec3 _reserved;
} drawPush;

layout(location = 0) out vec3 vWorldPos;
layout(location = 1) out vec3 vWorldNormal;
layout(location = 2) out vec3 vWorldTangent;
layout(location = 3) out float vWorldBitangentSign;
// #1036 / F-WAT-08 — `vUV` (loc 4) and `vInstanceIndex` (loc 5)
// were declared here but `water.frag` never read them; locations 4
// and 5 are now free. Water computes its UVs from world XZ (flat
// planes) or T/B projection (waterfalls) directly in the fragment
// shader, and the push-constant block carries every per-plane
// parameter that would otherwise need an instance lookup — there's
// no per-fragment `gl_InstanceIndex` dependency on this path.

void main() {
    GpuInstance inst = instances[gl_InstanceIndex];

    vec4 worldPos = inst.model * vec4(inPosition, 1.0);
    WaterParams water = waterParams.params[drawPush.waterIndex];
    uint kind = uint(water.timing.y + 0.5);
    // Flat cell planes use the authored wave pair. Waterfall meshes are
    // already artist-oriented sheets and keep their authored geometry.
    if (kind != WATER_WATERFALL) {
        vec3 absolutePos = worldPos.xyz + renderOrigin.xyz;
        vec2 dirA = water.scroll.xy;
        vec2 dirB = water.scroll.zw;
        if (length(dirA) < 1e-5) dirA = vec2(1.0, 0.35);
        if (length(dirB) < 1e-5) dirB = vec2(-0.4, 1.0);
        dirA = normalize(dirA);
        dirB = normalize(dirB);
        float spatialA = max(abs(water.tune.x), 1.0 / 2048.0);
        float spatialB = max(abs(water.tune.y), 1.0 / 4096.0);
        float frequency = max(abs(water.misc.y), 0.0);
        float phaseA = dot(absolutePos.xz, dirA) * spatialA * 6.2831853
                     + water.timing.x * frequency * 6.2831853;
        float phaseB = dot(absolutePos.xz, dirB) * spatialB * 6.2831853
                     - water.timing.x * frequency * 4.7123890;
        // Author data can contain corrupt/extreme values; keep displacement
        // finite and below a conservative shoreline-safe bound.
        float amplitude = clamp(water.tune.w, 0.0, 32.0);
        worldPos.y += amplitude * (sin(phaseA) * 0.60 + sin(phaseB) * 0.40);
    }
    // #markarth-precision — `inst.model` is rebased by the render origin (the
    // water plane reuses the same instance buffer as opaques), so `worldPos`
    // is relative; clip is computed from the relative viewProj below. Output
    // the ABSOLUTE world position for water.frag's lighting / RT reflection +
    // refraction (the TLAS is absolute).
    vWorldPos = worldPos.xyz + renderOrigin.xyz;

    // For the water quad, `inst.model` is composed of (translation,
    // axis-aligned rotation, uniform scale) — see the cell loader's
    // water-plane spawn. So the 3×3 upper block is a similarity
    // transform and we can transform normal / tangent with it
    // directly (no inverse-transpose needed). The renderer guarantees
    // water meshes never carry non-uniform scale (`INSTANCE_FLAG_NUS`
    // is clear).
    mat3 modelRot = mat3(inst.model);
    vWorldNormal       = normalize(modelRot * inNormal);
    vWorldTangent      = normalize(modelRot * inTangent.xyz);
    vWorldBitangentSign = inTangent.w;

    // TAA jitter pulled from the camera UBO — keeps water's projected
    // depth coherent with the opaque pass so the shoreline foam ray
    // and the depth buffer stay in lockstep.
    vec4 clip = viewProj * worldPos;
    clip.xy += jitter.xy * clip.w;
    gl_Position = clip;
}
