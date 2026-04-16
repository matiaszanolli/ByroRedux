#version 460
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;
layout(location = 2) in vec3 inNormal;
layout(location = 3) in vec2 inUV;
layout(location = 4) in uvec4 inBoneIndices;
layout(location = 5) in vec4  inBoneWeights;

// Per-instance data from the instance SSBO. Each draw's gl_InstanceIndex
// maps to one entry containing model matrix, texture index, and bone offset.
// Must match Rust GpuInstance layout exactly — all scalars, no vec3.
struct GpuInstance {
    mat4 model;              // 64 bytes
    uint textureIndex;       // offset 64
    uint boneOffset;         // offset 68
    uint normalMapIndex;     // offset 72
    float roughness;         // offset 76
    float metalness;         // offset 80
    float emissiveMult;      // offset 84
    float emissiveR;         // offset 88
    float emissiveG;         // offset 92
    float emissiveB;         // offset 96
    float specularStrength;  // offset 100
    float specularR;         // offset 104
    float specularG;         // offset 108
    float specularB;         // offset 112
    uint vertexOffset;       // offset 116
    uint indexOffset;        // offset 120
    uint vertexCount;        // offset 124
    float alphaThreshold;    // offset 128
    uint alphaTestFunc;      // offset 132
    uint darkMapIndex;       // offset 136
    float avgAlbedoR;        // offset 140
    float avgAlbedoG;        // offset 144
    float avgAlbedoB;        // offset 148
    uint flags;              // offset 152 — bit 0: non-uniform scale, bit 1: alpha blend enabled
    uint _pad1;              // offset 156 → total 160
};

layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer {
    GpuInstance instances[];
};

// Camera UBO (set 1, binding 1) — per-frame, shared across all draws.
layout(set = 1, binding = 1) uniform CameraUBO {
    mat4 viewProj;
    mat4 prevViewProj;   // Previous frame's view-projection for motion vectors.
    mat4 invViewProj;    // Precomputed inverse(viewProj) for world reconstruction.
    vec4 cameraPos;
    vec4 sceneFlags;
    vec4 screen;
    vec4 fog;
    vec4 jitter;         // xy = sub-pixel TAA jitter in NDC, zw = reserved.
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
// Current + previous frame clip-space positions for screen-space motion
// vector computation in the fragment shader. Passing both positions as
// varyings (not the motion vector itself) avoids perspective interpolation
// artifacts near edges. Assumes static geometry — skinned meshes get the
// wrong motion vector but SVGF will detect that as a disocclusion.
layout(location = 6) out vec4 fragCurrClipPos;
layout(location = 7) out vec4 fragPrevClipPos;

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

    // Motion vector: current + previous clip-space positions. Fragment
    // shader does the perspective divide and screen-space delta.
    // Assumes static geometry (same world position both frames).
    // Both positions are UN-JITTERED — motion must reflect scene motion
    // only, not the per-frame sub-pixel sampling offset.
    fragCurrClipPos = currClip;
    fragPrevClipPos = prevViewProj * worldPos;

    // Apply sub-pixel jitter for TAA supersampling. jitter.xy is expressed
    // in NDC, so we scale by clip.w so the offset is constant in NDC after
    // the perspective divide. When jitter = vec2(0) (TAA disabled path),
    // this is a no-op.
    currClip.xy += jitter.xy * currClip.w;
    gl_Position = currClip;
}
