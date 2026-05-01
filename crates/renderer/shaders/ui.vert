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
    // doesn't sample either; layout mirror only to keep the 320 B
    // std430 array stride in lockstep with triangle.{vert,frag}.
    float uvOffsetU;         // offset 192
    float uvOffsetV;         // offset 196
    float uvScaleU;          // offset 200
    float uvScaleV;          // offset 204
    float materialAlpha;     // offset 208
    float _uvPad0;           // offset 212
    float _uvPad1;           // offset 216
    float _uvPad2;           // offset 220
    // #562 — Skyrim+ BSLightingShaderProperty variant payloads. UI
    // pipeline doesn't sample; layout mirror only to keep the 320 B
    // std430 stride in lockstep with triangle.{vert,frag}.
    float skinTintR, skinTintG, skinTintB, skinTintA;             // offset 224
    float hairTintR, hairTintG, hairTintB;                        // offset 240
    float multiLayerEnvmapStrength;                                // offset 252
    float eyeLeftCenterX, eyeLeftCenterY, eyeLeftCenterZ;          // offset 256
    float eyeCubemapScale;                                         // offset 268
    float eyeRightCenterX, eyeRightCenterY, eyeRightCenterZ;       // offset 272
    float _eyePad;                                                 // offset 284
    float multiLayerInnerThickness, multiLayerRefractionScale;     // offset 288
    float multiLayerInnerScaleU, multiLayerInnerScaleV;            // offset 296
    float sparkleR, sparkleG, sparkleB, sparkleIntensity;          // offset 304
    // #221 — NiMaterialProperty diffuse + ambient. UI pipeline doesn't
    // sample either; layout mirror only to keep the 352 B std430
    // array stride in lockstep with triangle.{vert,frag}.
    float diffuseR, diffuseG, diffuseB, _diffusePad;               // offset 320
    float ambientR, ambientG, ambientB, _ambientPad;               // offset 336
    // #620 — BSEffectShaderProperty falloff cone. Layout-only mirror;
    // UI pipeline never consumes these.
    float falloffStartAngle, falloffStopAngle,
          falloffStartOpacity, falloffStopOpacity;                 // offset 352
    float softFalloffDepth, _falloffPad0,
          _falloffPad1, _falloffPad2;                              // offset 368
    // ── R1 Phase 3: material table indirection ───────────────────────
    // R1 Phase 5 — material table indirection. UI pipeline reads
    // `textureIndex` from `materials[materialId]` (same set 1 binding
    // 13 as the triangle pipeline). Other per-material fields aren't
    // consumed by the UI vertex stage.
    uint materialId; float _matPad0, _matPad1, _matPad2;           // offset 384 → total 400
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
    float roughness, metalness, emissiveMult, _padPbr;
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
