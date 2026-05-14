#version 460
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;
layout(location = 2) in vec3 inNormal;
layout(location = 3) in vec2 inUV;
layout(location = 4) in uvec4 inBoneIndices;
layout(location = 5) in vec4  inBoneWeights;
// Terrain splat weights — R8G8B8A8_UNORM → vec4 in [0,1] (#470).
// Zero for every non-terrain mesh; the fragment shader only consumes
// them when `instance.flags & INSTANCE_FLAG_TERRAIN_SPLAT` is set.
layout(location = 6) in vec4 inSplat0; // layers 0-3
layout(location = 7) in vec4 inSplat1; // layers 4-7
// Per-vertex tangent (xyz) + bitangent sign (w). Authored by Bethesda
// content under NiBinaryExtraData("Tangent space ...") (Oblivion / FO3
// / FNV) and inline in the BSTriShape vertex stream (Skyrim+ / FO4).
// Zero on rigid / particle / UI / terrain / non-Bethesda content; the
// fragment shader's perturbNormal detects the zero magnitude and
// falls back to screen-space derivative TBN reconstruction.
// See #783 / M-NORMALS.
layout(location = 8) in vec4 inTangent;

// Per-mesh bone-palette stride. Matches the Rust constant
// `MAX_BONES_PER_MESH` at `crates/core/src/ecs/components/skinned_mesh.rs:29`.
// Pinned in `skin_vertices.comp` to the same value so a future change
// updates both shader sites in lockstep. See #651 / SH-6.
const uint MAX_BONES_PER_MESH = 128u;

// Per-instance data from the instance SSBO. R1 Phase 6 collapsed the
// per-material fields onto the `MaterialBuffer` SSBO indexed by
// `materialId`; what's left is strictly per-DRAW (model + mesh refs +
// flags + materialId + caustic-source avgAlbedo). Must match Rust
// GpuInstance layout exactly — all scalars, no vec3.
struct GpuInstance {
    mat4 model;            // 64 bytes
    uint textureIndex;     // offset 64 — diffuse / albedo
    uint boneOffset;       // offset 68
    uint vertexOffset;     // offset 72
    uint indexOffset;      // offset 76
    uint vertexCount;      // offset 80
    uint flags;            // offset 84 — bit 0: non-uniform scale, bit 1: alpha blend, bit 2: caustic source, bit 3 + bits 16..32: terrain splat
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

// Camera UBO (set 1, binding 1) — per-frame, shared across all draws.
// MUST match `GpuCamera` in `crates/renderer/src/vulkan/scene_buffer.rs`
// in both field order AND field count. Pre-#1028 (R-D6-01) this block
// terminated at `jitter`, omitting the trailing `skyTint` field that
// `GpuCamera` carries. The vertex shader didn't consume `skyTint` so
// the truncation was benign, but any future vertex-stage effect that
// reads `skyTint` would silently OOB-read off the end of the bound
// UBO range — a latent footgun the audit flagged.
layout(set = 1, binding = 1) uniform CameraUBO {
    mat4 viewProj;
    mat4 prevViewProj;   // Previous frame's view-projection for motion vectors.
    mat4 invViewProj;    // Precomputed inverse(viewProj) for world reconstruction.
    vec4 cameraPos;
    vec4 sceneFlags;
    vec4 screen;
    vec4 fog;
    vec4 jitter;         // xy = sub-pixel TAA jitter in NDC, zw = reserved.
    vec4 skyTint;        // xyz = TOD/weather zenith colour, w = reserved. #1028.
};

// Bone palette SSBO (set 1, binding 3) — skinning matrices for the
// CURRENT frame.
layout(std430, set = 1, binding = 3) readonly buffer BoneBuffer {
    mat4 bones[];
};

// Previous-frame bone palette SSBO (set 1, binding 12) — same layout as
// `bones[]` but populated from the prior frame's upload. The descriptor
// is wired to the OTHER slot in the per-frame `bone_buffers` ring (see
// `SceneBuffers::new`), so reading it yields last frame's joint poses
// for the same bone indices. Used to compute a true skinned previous-
// frame world position for motion vectors. SH-3 / #641.
layout(std430, set = 1, binding = 12) readonly buffer BonesPrevBuffer {
    mat4 bones_prev[];
};

layout(location = 0) out vec3 fragColor;
layout(location = 1) out vec2 fragUV;
layout(location = 2) out vec3 fragNormal;
layout(location = 3) out vec3 fragWorldPos;
layout(location = 4) flat out uint fragTexIndex;
layout(location = 5) flat out int fragInstanceIndex;
// Current + previous frame clip-space positions for screen-space motion
// vector computation in the fragment shader. Passing both positions as
// varyings (not the motion vector itself) avoids perspective interpolation
// artifacts near edges. Skinned vertices reproject through last frame's
// bone palette via `bones_prev` (SH-3 / #641); rigid vertices reuse the
// per-instance model matrix (treated as static across the frame pair —
// fast-moving rigid actors / decals would still mis-reproject, tracked
// separately).
layout(location = 6) out vec4 fragCurrClipPos;
layout(location = 7) out vec4 fragPrevClipPos;
// Splat weights forwarded flat — terrain tile data is constant across
// a mesh draw, and per-vertex splat values interpolate linearly
// through the rasterizer by default (`flat` is not used because we
// *want* the blend to be smooth across the triangle).
layout(location = 8) out vec4 fragSplat0;
layout(location = 9) out vec4 fragSplat1;
// #783 / M-NORMALS — per-vertex tangent (xyz) + bitangent sign (w),
// transformed to world-space via the same xform used for position
// + normal. Zero (length < epsilon) signals "no authored tangent;
// fragment shader falls back to screen-space derivative TBN."
layout(location = 10) out vec4 fragTangent;

void main() {
    GpuInstance inst = instances[gl_InstanceIndex];

    // Rigid vs skinned vertex selection. `xform` is the current-frame
    // transform; `xformPrev` is the same composition through the prior
    // frame's bone palette (SH-3 / #641). For rigid vertices we reuse
    // the per-instance model matrix on both frames — `GpuInstance` is
    // rebuilt every frame with the current transform, so this is exact
    // for static geometry and a one-frame lag on moving rigid bodies.
    float wsum = inBoneWeights.x + inBoneWeights.y + inBoneWeights.z + inBoneWeights.w;
    mat4 xform;
    mat4 xformPrev;
    if (wsum < 0.001) {
        xform = inst.model;
        xformPrev = inst.model;
    } else {
        uint base = inst.boneOffset;
        // #651 / SH-6 SIBLING — same per-vertex bone-index clamp as
        // `skin_vertices.comp`. Raster mode is more forgiving than
        // the compute path's BLAS-refit output (a degenerate vertex
        // is one off-screen triangle, not corrupt geometry in the
        // TLAS), but mirroring the clamp keeps the two sites in
        // lockstep so a future regression / corrupt NIF index byte
        // can't silently diverge the two paths.
        uvec4 bIdx = min(inBoneIndices, uvec4(MAX_BONES_PER_MESH - 1u));
        xform =
              inBoneWeights.x * bones[base + bIdx.x]
            + inBoneWeights.y * bones[base + bIdx.y]
            + inBoneWeights.z * bones[base + bIdx.z]
            + inBoneWeights.w * bones[base + bIdx.w];
        xformPrev =
              inBoneWeights.x * bones_prev[base + bIdx.x]
            + inBoneWeights.y * bones_prev[base + bIdx.y]
            + inBoneWeights.z * bones_prev[base + bIdx.z]
            + inBoneWeights.w * bones_prev[base + bIdx.w];
    }

    vec4 worldPos = xform * vec4(inPosition, 1.0);
    vec4 prevWorldPos = xformPrev * vec4(inPosition, 1.0);
    vec4 currClip = viewProj * worldPos;
    // NOTE: gl_Position is jittered below for TAA. fragCurrClipPos must
    // remain un-jittered so motion vectors are correct across frames.
    fragColor = inColor;
    fragUV = inUV;
    // Normal transform. For orthogonal upper-3x3 (uniform or no scale),
    // m3 * normal gives the correct direction — normalize handles magnitude.
    // Only non-uniform scale (skew) requires the expensive inverse-transpose
    // (~40 ALU ops: determinant + cofactors + transpose). The CPU sets
    // flags bit 0 when column lengths differ. See #273.
    mat3 m3 = mat3(xform);
    vec3 n;
    if ((inst.flags & 1u) != 0u) {
        float det = determinant(m3);
        n = (abs(det) > 1e-6) ? transpose(inverse(m3)) * inNormal : inNormal;
    } else {
        n = m3 * inNormal;
    }
    fragNormal = (dot(n, n) > 0.0) ? normalize(n) : vec3(0.0, 1.0, 0.0);
    fragWorldPos = worldPos.xyz;
    fragTexIndex = inst.textureIndex;
    fragInstanceIndex = gl_InstanceIndex;

    // #783 / M-NORMALS — transform tangent direction by the same
    // upper-3x3 as positions. Tangents are CONTRAVARIANT vectors
    // (they transform like position derivatives), so the correct
    // transform under both uniform AND non-uniform scale is `m3 * T`
    // — NOT the inverse-transpose used for normals. Pre-#787 the
    // non-uniform-scale path used `transpose(inverse(m3)) * T` (which
    // produces the cotangent direction along the gradient of the
    // parametrization, not the tangent). On axis-aligned scales the
    // two coincide, but off-axis tangents on non-uniformly-scaled
    // meshes ended up rotated relative to the surface u-axis;
    // perturbNormal's Gram-Schmidt step at the fragment shader masks
    // the magnitude error but cannot recover the wrong direction.
    //
    // Magnitude is irrelevant — `perturbNormal` re-normalizes T after
    // Gram-Schmidt against N, so we only need to preserve direction.
    // Zero-length tangent (no authored data) is preserved as zero so
    // the fragment shader's fallback gate detects it and routes to
    // screen-space derivative TBN. See #787 / R-N3.
    vec3 t_world;
    if (dot(inTangent.xyz, inTangent.xyz) < 1e-6) {
        t_world = vec3(0.0);
    } else {
        t_world = m3 * inTangent.xyz;
        float t_len2 = dot(t_world, t_world);
        t_world = (t_len2 > 0.0) ? t_world * inversesqrt(t_len2) : vec3(0.0);
    }
    fragTangent = vec4(t_world, inTangent.w);

    // Motion vector: current + previous clip-space positions. Fragment
    // shader does the perspective divide and screen-space delta. Skinned
    // vertices project `prevWorldPos` (composed via `bones_prev`) so the
    // motion vector reflects per-vertex joint motion, not just camera
    // motion. Pre-#641 this used `worldPos` for both, leaving SVGF / TAA
    // to reproject the wrong source pixel on every actor body /
    // hand / face fragment. Both positions are UN-JITTERED — motion
    // must reflect scene motion only, not the per-frame sub-pixel
    // sampling offset.
    fragCurrClipPos = currClip;
    fragPrevClipPos = prevViewProj * prevWorldPos;
    fragSplat0 = inSplat0;
    fragSplat1 = inSplat1;

    // Apply sub-pixel jitter for TAA supersampling. jitter.xy is expressed
    // in NDC, so we scale by clip.w so the offset is constant in NDC after
    // the perspective divide. When jitter = vec2(0) (TAA disabled path),
    // this is a no-op.
    currClip.xy += jitter.xy * currClip.w;
    gl_Position = currClip;
}
