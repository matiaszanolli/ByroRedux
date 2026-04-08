#version 450

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;
layout(location = 2) in vec3 inNormal;
layout(location = 3) in vec2 inUV;
layout(location = 4) in uvec4 inBoneIndices;
layout(location = 5) in vec4  inBoneWeights;

layout(push_constant) uniform PushConstants {
    mat4 viewProj;
    mat4 model;
    uint boneOffset;
} pc;

// Bone palette: computed CPU-side as `bone_world * bind_inverse`, one
// entry per bone across all skinned meshes in the frame. Slot 0 is a
// reserved identity matrix. See crates/renderer/src/vulkan/scene_buffer.rs
// and the SkinnedMesh ECS component (issue #178).
layout(std430, set = 1, binding = 3) readonly buffer BoneBuffer {
    mat4 bones[];
};

layout(location = 0) out vec3 fragColor;
layout(location = 1) out vec2 fragUV;
layout(location = 2) out vec3 fragNormal;
layout(location = 3) out vec3 fragWorldPos;

void main() {
    // Rigid vertices are tagged by sum(weights) ≈ 0 and go through the
    // push-constant model matrix directly. Skinned vertices blend 4
    // bone-palette entries weighted by the per-vertex weights; the
    // palette entries are already in world space (bone_world * bind_inv)
    // so no additional model multiplication is needed.
    float wsum = inBoneWeights.x + inBoneWeights.y + inBoneWeights.z + inBoneWeights.w;
    mat4 xform;
    if (wsum < 0.001) {
        xform = pc.model;
    } else {
        uint base = pc.boneOffset;
        xform =
              inBoneWeights.x * bones[base + inBoneIndices.x]
            + inBoneWeights.y * bones[base + inBoneIndices.y]
            + inBoneWeights.z * bones[base + inBoneIndices.z]
            + inBoneWeights.w * bones[base + inBoneIndices.w];
    }

    vec4 worldPos = xform * vec4(inPosition, 1.0);
    gl_Position = pc.viewProj * worldPos;
    fragColor = inColor;
    fragUV = inUV;
    // Transform normal by xform's upper 3x3. For uniform scale this is
    // equivalent to inverse-transpose. Guard against zero-scale meshes
    // (NIF placeholders, animated transitions) where mat3(xform) is
    // degenerate and normalize() would produce NaN.
    vec3 n = mat3(xform) * inNormal;
    fragNormal = (dot(n, n) > 0.0) ? normalize(n) : vec3(0.0, 1.0, 0.0);
    fragWorldPos = worldPos.xyz;
}
