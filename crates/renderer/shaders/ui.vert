#version 450
// REN-2026-07-28-02 / #2219 — GpuInstance grew a uint64_t member; this
// mirror never dereferences it, but the type must be recognized to keep
// the struct byte-for-byte identical to the other 4 declaration sites.
#extension GL_EXT_buffer_reference : require
#extension GL_ARB_gpu_shader_int64 : require

// UI overlay vertex shader — passthrough to clip space, no transforms.
// Vertices are already in NDC ([-1,1] range).
// Uses UiVertex (position + UV only, 20 bytes).
// Reads texture index from instance SSBO for bindless sampling.

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec2 inUV;

// `GpuInstance` layout mirror of triangle.{vert,frag} for struct-size
// lockstep. The UI vertex stage reads `textureIndex` (not `materialId`) —
// UI draws bypass the MaterialBuffer and sample the bindless texture
// directly via `textureIndex`. (#1065 / REN-D14-NEW-05)
struct GpuInstance {
    mat4 model;
    uint textureIndex;     // offset 64 — diffuse / albedo (kept for parity)
    uint boneOffset;       // offset 68
    uint vertexOffset;     // offset 72
    uint indexOffset;      // offset 76
    uint vertexCount;      // offset 80
    uint flags;            // offset 84
    uint materialId;       // offset 88
    float ior;             // offset 92 — per-draw optical IOR (read by caustic_splat.comp)
    float avgAlbedoR;      // offset 96 — kept for caustic_splat.comp
    float avgAlbedoG;      // offset 100
    float avgAlbedoB;      // offset 104
    uint surfaceId;        // offset 108 — stable temporal-shadow identity
    uint64_t skinnedVertexAddress; // offset 112 — #2219, unused here
    uvec2 _reserved;               // offset 120 -> total 128
    uint64_t morphDeltaAddress;  // offset 128 — #3231, unused here
    uint64_t morphWeightAddress; // offset 136 — #3231, unused here
    uint morphTargetCount;       // offset 144 — #3231, unused here
    // Deliberately three scalar uints, NOT uvec3 — see gpu_types.rs.
    uint _reserved2a; // offset 148
    uint _reserved2b; // offset 152
    uint _reserved2c; // offset 156 -> total 160
};

layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer {
    GpuInstance instances[];
};

layout(location = 0) out vec2 fragUV;
layout(location = 1) flat out uint fragTexIndex;

void main() {
    gl_Position = vec4(inPosition.xy, 0.0, 1.0);
    fragUV = inUV;
    // The UI quad is appended at draw.rs with `..GpuInstance::default()`,
    // which leaves `materialId = 0`. Post-#807 `materials[0]` is the
    // reserved neutral default (not an arbitrary scene material, as in
    // the pre-#807 days) — reading it here would still pull the wrong
    // texture, since the UI texture lives in per-instance `textureIndex`,
    // not in any GpuMaterial slot. Reading per-instance `textureIndex` is
    // the contracted path (scene_buffer/gpu_types.rs:191-197) and matches
    // triangle.vert.
    // See #776 / #785 for the Phase-5 regressions this guards against.
    GpuInstance inst = instances[gl_InstanceIndex];
    fragTexIndex = inst.textureIndex;
}
