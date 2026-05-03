#version 450

// UI overlay vertex shader — passthrough to clip space, no transforms.
// Vertices are already in NDC ([-1,1] range).
// Uses UiVertex (position + UV only, 20 bytes).
// Reads texture index from instance SSBO for bindless sampling.

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec2 inUV;

// R1 Phase 6 — `GpuInstance` collapsed to per-DRAW data only. UI
// vertex stage reads `materialId` to look up the texture in the
// `MaterialBuffer` SSBO; other per-material fields live exclusively
// in `materials[materialId]` now. Layout mirror of triangle.{vert,frag}.
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

// ── R1 Phase 5: material table read for UI textureIndex ─────────────
//
// Mirrors the GpuMaterial declaration in triangle.frag — only the
// fields the UI vertex stage might read (today: textureIndex). The
// std430 stride must match the full 272 B layout, so every padded
// vec4 lands at the right offset; unconsumed fields are layout-only.
struct GpuMaterial {
    // `materialFlags` (#695): bitfield, layout-only here — the UI
    // vertex stage doesn't read it. Matches the slot
    // `triangle.frag::GpuMaterial.materialFlags` consumes.
    float roughness, metalness, emissiveMult;
    uint materialFlags;
    float emissiveR, emissiveG, emissiveB, specularStrength;
    float specularR, specularG, specularB, alphaThreshold;
    uint textureIndex, normalMapIndex, darkMapIndex, glowMapIndex;
    uint detailMapIndex, glossMapIndex, parallaxMapIndex, envMapIndex;
    uint envMaskIndex, alphaTestFunc, materialKind; float materialAlpha;
    float parallaxHeightScale, parallaxMaxPasses, uvOffsetU, uvOffsetV;
    float uvScaleU, uvScaleV, diffuseR, diffuseG;
    float diffuseB, ambientR, ambientG, ambientB;
    float avgAlbedoR, avgAlbedoG, avgAlbedoB, skinTintA;
    float skinTintR, skinTintG, skinTintB, hairTintR;
    float hairTintG, hairTintB, multiLayerEnvmapStrength, eyeLeftCenterX;
    float eyeLeftCenterY, eyeLeftCenterZ, eyeCubemapScale, eyeRightCenterX;
    float eyeRightCenterY, eyeRightCenterZ, multiLayerInnerThickness, multiLayerRefractionScale;
    float multiLayerInnerScaleU, multiLayerInnerScaleV, sparkleR, sparkleG;
    float sparkleB, sparkleIntensity, falloffStartAngle, falloffStopAngle;
    float falloffStartOpacity, falloffStopOpacity, softFalloffDepth, _padFalloff;
};

layout(std430, set = 1, binding = 13) readonly buffer MaterialBuffer {
    GpuMaterial materials[];
};

layout(location = 0) out vec2 fragUV;
layout(location = 1) flat out uint fragTexIndex;

void main() {
    gl_Position = vec4(inPosition.xy, 0.0, 1.0);
    fragUV = inUV;
    // R1 Phase 5 — read texture index from the material table.
    GpuInstance inst = instances[gl_InstanceIndex];
    fragTexIndex = materials[inst.materialId].textureIndex;
}
