#version 450

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
    float _padId0;         // offset 92
    float avgAlbedoR;      // offset 96 — kept for caustic_splat.comp
    float avgAlbedoG;      // offset 100
    float avgAlbedoB;      // offset 104
    float _padAlbedo;      // offset 108 → total 112
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
    // which leaves `materialId = 0`. The MaterialBuffer is keyed by per-
    // frame intern order, so `materials[0]` is the FIRST scene material
    // — not the UI texture. Reading per-instance `textureIndex` is the
    // contracted path (scene_buffer.rs:172-176) and matches triangle.vert.
    // See #776 / #785 for the Phase-5 regressions this guards against.
    GpuInstance inst = instances[gl_InstanceIndex];
    fragTexIndex = inst.textureIndex;
}
