#version 450
#extension GL_GOOGLE_include_directive : require
// REN-2026-07-28-02 / #2219 — `include/gpu_instance.glsl` declares `uint64_t`
// members, so the type must be recognized even where a lane is never
// dereferenced.
#extension GL_EXT_buffer_reference : require
#extension GL_ARB_gpu_shader_int64 : require

// UI overlay vertex shader — passthrough to clip space, no transforms.
// Vertices are already in NDC ([-1,1] range).
// Uses UiVertex (position + UV only, 20 bytes).
// Reads texture index from instance SSBO for bindless sampling.

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec2 inUV;

// The UI vertex stage reads `textureIndex` (not `materialId`) — UI draws
// bypass the MaterialBuffer and sample the bindless texture directly via
// `textureIndex`. (#1065 / REN-D14-NEW-05)
#include "include/gpu_instance.glsl"

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
