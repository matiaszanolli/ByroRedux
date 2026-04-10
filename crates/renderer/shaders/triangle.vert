#version 450
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;
layout(location = 2) in vec3 inNormal;
layout(location = 3) in vec2 inUV;
layout(location = 4) in uvec4 inBoneIndices;
layout(location = 5) in vec4  inBoneWeights;

// Per-instance data from the instance SSBO. Each draw's gl_InstanceIndex
// maps to one entry containing model matrix, texture index, and bone offset.
struct GpuInstance {
    mat4 model;
    uint textureIndex;
    uint boneOffset;
    uint _pad0;
    uint _pad1;
};

layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer {
    GpuInstance instances[];
};

// Camera UBO (set 1, binding 1) — per-frame, shared across all draws.
layout(set = 1, binding = 1) uniform CameraUBO {
    mat4 viewProj;
    vec4 cameraPos;
    vec4 sceneFlags;
};

// Bone palette SSBO (set 1, binding 3) — skinning matrices.
layout(std430, set = 1, binding = 3) readonly buffer BoneBuffer {
    mat4 bones[];
};

layout(location = 0) out vec3 fragColor;
layout(location = 1) out vec2 fragUV;
layout(location = 2) out vec3 fragNormal;
layout(location = 3) out vec3 fragWorldPos;
layout(location = 4) flat out uint fragTexIndex;
layout(location = 5) flat out int fragInstanceIndex;

void main() {
    GpuInstance inst = instances[gl_InstanceIndex];

    // Rigid vs skinned vertex selection.
    float wsum = inBoneWeights.x + inBoneWeights.y + inBoneWeights.z + inBoneWeights.w;
    mat4 xform;
    if (wsum < 0.001) {
        xform = inst.model;
    } else {
        uint base = inst.boneOffset;
        xform =
              inBoneWeights.x * bones[base + inBoneIndices.x]
            + inBoneWeights.y * bones[base + inBoneIndices.y]
            + inBoneWeights.z * bones[base + inBoneIndices.z]
            + inBoneWeights.w * bones[base + inBoneIndices.w];
    }

    vec4 worldPos = xform * vec4(inPosition, 1.0);
    gl_Position = viewProj * worldPos;
    fragColor = inColor;
    fragUV = inUV;
    // Correct normal transform for non-uniform scale (inverse-transpose).
    mat3 m3 = mat3(xform);
    float det = determinant(m3);
    vec3 n = (abs(det) > 1e-6) ? transpose(inverse(m3)) * inNormal : inNormal;
    fragNormal = (dot(n, n) > 0.0) ? normalize(n) : vec3(0.0, 1.0, 0.0);
    fragWorldPos = worldPos.xyz;
    fragTexIndex = inst.textureIndex;
    fragInstanceIndex = gl_InstanceIndex;
}
