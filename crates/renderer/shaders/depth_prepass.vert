#version 460
#extension GL_EXT_nonuniform_qualifier : require

// Depth pre-pass vertex stage. Mirrors the transform / skinning / jitter
// math of `triangle.vert` exactly so depth produced here matches the
// main pass byte-for-byte. Outputs ONLY what the alpha-test discard in
// `depth_prepass.frag` needs: UV + texture index + materialId + flags.
//
// Why this exists: see #779. With `layout(early_fragment_tests) in;` on
// `triangle.frag` we need the depth buffer pre-populated with correct
// alpha-test discards before the main pass runs. This shader runs in
// the depth-only pre-pass, performs alpha-test discard, and writes
// depth — main pass then sees correct depth and early-Z safely culls
// overdrawn fragments without ghost-rectangle artifacts on alpha-tested
// geometry (foliage, fences, hair cards, decals).

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;       // unused but kept for vertex-input compat with triangle.vert
layout(location = 2) in vec3 inNormal;      // unused
layout(location = 3) in vec2 inUV;
layout(location = 4) in uvec4 inBoneIndices;
layout(location = 5) in vec4  inBoneWeights;
layout(location = 6) in vec4  inSplat0;     // unused
layout(location = 7) in vec4  inSplat1;     // unused

const uint MAX_BONES_PER_MESH = 128u;

struct GpuInstance {
    mat4 model;
    uint textureIndex;
    uint boneOffset;
    uint vertexOffset;
    uint indexOffset;
    uint vertexCount;
    uint flags;
    uint materialId;
    float _padId0;
    float avgAlbedoR;
    float avgAlbedoG;
    float avgAlbedoB;
    float _padAlbedo;
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
};

layout(std430, set = 1, binding = 3) readonly buffer BoneBuffer {
    mat4 bones[];
};

layout(location = 0) out vec2 fragUV;
layout(location = 1) flat out uint fragTextureIndex;
layout(location = 2) flat out uint fragMaterialId;

void main() {
    GpuInstance inst = instances[gl_InstanceIndex];

    float wsum = inBoneWeights.x + inBoneWeights.y + inBoneWeights.z + inBoneWeights.w;
    mat4 xform;
    if (wsum < 0.001) {
        xform = inst.model;
    } else {
        uint base = inst.boneOffset;
        uvec4 bIdx = min(inBoneIndices, uvec4(MAX_BONES_PER_MESH - 1u));
        xform =
              inBoneWeights.x * bones[base + bIdx.x]
            + inBoneWeights.y * bones[base + bIdx.y]
            + inBoneWeights.z * bones[base + bIdx.z]
            + inBoneWeights.w * bones[base + bIdx.w];
    }

    vec4 worldPos = xform * vec4(inPosition, 1.0);
    vec4 currClip = viewProj * worldPos;

    // Same TAA jitter as triangle.vert — depth must match main pass.
    currClip.xy += jitter.xy * currClip.w;

    gl_Position = currClip;
    fragUV = inUV;
    fragTextureIndex = inst.textureIndex;
    fragMaterialId = inst.materialId;
}
