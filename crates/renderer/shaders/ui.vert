#version 450

// UI overlay vertex shader — passthrough to clip space, no transforms.
// Vertices are already in NDC ([-1,1] range).
// Uses UiVertex (position + UV only, 20 bytes).
// Reads texture index from instance SSBO for bindless sampling.

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec2 inUV;

struct GpuInstance {
    mat4 model;
    uint textureIndex;
    uint boneOffset;
    uint normalMapIndex;
    float roughness;
    float metalness;
    float emissiveMult;
    float emissiveR, emissiveG, emissiveB;
    float specularStrength;
    float specularR, specularG, specularB;
    uint vertexOffset;
    uint indexOffset;
    uint vertexCount;
    float alphaThreshold;
    uint alphaTestFunc;
    uint darkMapIndex;
    float avgAlbedoR, avgAlbedoG, avgAlbedoB;
    uint flags;  // offset 152 — bits 0-2 = scale/blend/caustic; bit 3 =
                 // terrain splat (#470), upper 16 bits = terrain tile
                 // index. Unused by ui.vert but named in lockstep with
                 // triangle.{vert,frag} and Rust `GpuInstance` (Shader
                 // Struct Sync invariant). See #318.
    uint materialKind;  // offset 156 — BSLightingShaderProperty.shader_type (0–19), unused by the UI pipeline; named in lockstep with the scene shaders. See #344.
    // #399 — NiTexturingProperty extra slots, named in lockstep with
    // triangle.{vert,frag}. UI pipeline doesn't sample them but the
    // SSBO stride must match (Shader Struct Sync invariant).
    uint glowMapIndex;       // offset 160
    uint detailMapIndex;     // offset 164
    uint glossMapIndex;      // offset 168
    // #453 — BSShaderTextureSet slots 3/4/5 + POM scalars. Named in
    // lockstep with triangle.{vert,frag}; UI pipeline doesn't sample
    // them but the SSBO stride must match the 192 B Rust struct.
    uint parallaxMapIndex;   // offset 172 (reclaimed from _padExtraTextures)
    float parallaxHeightScale; // offset 176
    float parallaxMaxPasses;   // offset 180
    uint envMapIndex;        // offset 184
    uint envMaskIndex;       // offset 188
    // #492 — FO4 BGSM UV transform + material alpha. UI pipeline
    // doesn't sample either; layout mirror only to keep the 224 B
    // std430 array stride in lockstep with triangle.{vert,frag}.
    float uvOffsetU;         // offset 192
    float uvOffsetV;         // offset 196
    float uvScaleU;          // offset 200
    float uvScaleV;          // offset 204
    float materialAlpha;     // offset 208
    float _uvPad0;           // offset 212
    float _uvPad1;           // offset 216
    float _uvPad2;           // offset 220 → total 224
};

layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer {
    GpuInstance instances[];
};

layout(location = 0) out vec2 fragUV;
layout(location = 1) flat out uint fragTexIndex;

void main() {
    gl_Position = vec4(inPosition.xy, 0.0, 1.0);
    fragUV = inUV;
    fragTexIndex = instances[gl_InstanceIndex].textureIndex;
}
