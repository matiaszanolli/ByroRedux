#version 460
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;
layout(location = 2) in vec3 inNormal;
layout(location = 3) in vec2 inUV;
layout(location = 4) in uvec4 inBoneIndices;
layout(location = 5) in vec4  inBoneWeights;
// Terrain splat weights — R8G8B8A8_UNORM → vec4 in [0,1] (#470).
// Zero for every non-terrain mesh; the fragment shader only consumes
// them when `instance.flags & INSTANCE_FLAG_TERRAIN_SPLAT` is set.
layout(location = 6) in vec4 inSplat0; // layers 0-3
layout(location = 7) in vec4 inSplat1; // layers 4-7

// Per-mesh bone-palette stride. Matches the Rust constant
// `MAX_BONES_PER_MESH` at `crates/core/src/ecs/components/skinned_mesh.rs:29`.
// Pinned in `skin_vertices.comp` to the same value so a future change
// updates both shader sites in lockstep. See #651 / SH-6.
const uint MAX_BONES_PER_MESH = 128u;

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
    uint flags;              // offset 152 — bit 0: non-uniform scale, bit 1: alpha blend, bit 2: caustic source (#321)
    uint materialKind;       // offset 156 — BSLightingShaderProperty.shader_type (0–19) for fragment-shader variant dispatch (#344). 0 = Default lit.
    uint glowMapIndex;       // offset 160 — NiTexturingProperty slot 4 (#399). Vertex stage doesn't sample, but the layout must mirror.
    uint detailMapIndex;     // offset 164 — NiTexturingProperty slot 2 (#399).
    uint glossMapIndex;      // offset 168 — NiTexturingProperty slot 3 (#399).
    // #453 — BSShaderTextureSet slots 3/4/5 + POM scalars. Vertex
    // stage doesn't sample these either; layout mirror only.
    uint parallaxMapIndex;   // offset 172 — slot 3 (reclaimed from _padExtraTextures)
    float parallaxHeightScale; // offset 176
    float parallaxMaxPasses;   // offset 180
    uint envMapIndex;        // offset 184 — slot 4
    uint envMaskIndex;       // offset 188
    // #492 — FO4 BGSM UV transform + material alpha. Vertex stage
    // doesn't apply these (fragment shader uses them for the
    // texture lookup); layout mirror only.
    float uvOffsetU;         // offset 192
    float uvOffsetV;         // offset 196
    float uvScaleU;          // offset 200
    float uvScaleV;          // offset 204
    float materialAlpha;     // offset 208
    float _uvPad0;           // offset 212
    float _uvPad1;           // offset 216
    float _uvPad2;           // offset 220
    // Skyrim+ BSLightingShaderProperty variant payloads (#562).
    // Vertex stage doesn't read these; layout mirror only.
    float skinTintR;                   // offset 224
    float skinTintG;                   // offset 228
    float skinTintB;                   // offset 232
    float skinTintA;                   // offset 236
    float hairTintR;                   // offset 240
    float hairTintG;                   // offset 244
    float hairTintB;                   // offset 248
    float multiLayerEnvmapStrength;    // offset 252
    float eyeLeftCenterX;              // offset 256
    float eyeLeftCenterY;              // offset 260
    float eyeLeftCenterZ;              // offset 264
    float eyeCubemapScale;             // offset 268
    float eyeRightCenterX;             // offset 272
    float eyeRightCenterY;             // offset 276
    float eyeRightCenterZ;             // offset 280
    float _eyePad;                     // offset 284
    float multiLayerInnerThickness;    // offset 288
    float multiLayerRefractionScale;   // offset 292
    float multiLayerInnerScaleU;       // offset 296
    float multiLayerInnerScaleV;       // offset 300
    float sparkleR;                    // offset 304
    float sparkleG;                    // offset 308
    float sparkleB;                    // offset 312
    float sparkleIntensity;            // offset 316
    // ── #221: NiMaterialProperty diffuse + ambient colors ──────────
    // Two padded vec4 slots appended at the end. Vertex shader doesn't
    // sample either; declared only to keep the std430 stride byte-
    // identical with triangle.frag (Shader Struct Sync invariant).
    float diffuseR;                    // offset 320
    float diffuseG;                    // offset 324
    float diffuseB;                    // offset 328
    float _diffusePad;                 // offset 332
    float ambientR;                    // offset 336
    float ambientG;                    // offset 340
    float ambientB;                    // offset 344
    float _ambientPad;                 // offset 348
    // ── #620: BSEffectShaderProperty falloff cone ────────────────────
    // Two padded vec4 slots appended at the end. Vertex shader doesn't
    // consume them; declared only to keep the std430 stride
    // byte-identical with triangle.frag (Shader Struct Sync invariant).
    float falloffStartAngle;           // offset 352
    float falloffStopAngle;            // offset 356
    float falloffStartOpacity;         // offset 360
    float falloffStopOpacity;          // offset 364
    float softFalloffDepth;            // offset 368
    float _falloffPad0;                // offset 372
    float _falloffPad1;                // offset 376
    float _falloffPad2;                // offset 380 → total 384
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

// Bone palette SSBO (set 1, binding 3) — skinning matrices for the
// CURRENT frame.
layout(std430, set = 1, binding = 3) readonly buffer BoneBuffer {
    mat4 bones[];
};

// Previous-frame bone palette SSBO (set 1, binding 12) — same layout as
// `bones[]` but populated from the prior frame's upload. The descriptor
// is wired to the OTHER slot in the per-frame `bone_buffers` ring (see
// `SceneBuffers::new`), so reading it yields last frame's joint poses
// for the same bone indices. Used to compute a true skinned previous-
// frame world position for motion vectors. SH-3 / #641.
layout(std430, set = 1, binding = 12) readonly buffer BonesPrevBuffer {
    mat4 bones_prev[];
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
// artifacts near edges. Skinned vertices reproject through last frame's
// bone palette via `bones_prev` (SH-3 / #641); rigid vertices reuse the
// per-instance model matrix (treated as static across the frame pair —
// fast-moving rigid actors / decals would still mis-reproject, tracked
// separately).
layout(location = 6) out vec4 fragCurrClipPos;
layout(location = 7) out vec4 fragPrevClipPos;
// Splat weights forwarded flat — terrain tile data is constant across
// a mesh draw, and per-vertex splat values interpolate linearly
// through the rasterizer by default (`flat` is not used because we
// *want* the blend to be smooth across the triangle).
layout(location = 8) out vec4 fragSplat0;
layout(location = 9) out vec4 fragSplat1;

void main() {
    GpuInstance inst = instances[gl_InstanceIndex];

    // Rigid vs skinned vertex selection. `xform` is the current-frame
    // transform; `xformPrev` is the same composition through the prior
    // frame's bone palette (SH-3 / #641). For rigid vertices we reuse
    // the per-instance model matrix on both frames — `GpuInstance` is
    // rebuilt every frame with the current transform, so this is exact
    // for static geometry and a one-frame lag on moving rigid bodies.
    float wsum = inBoneWeights.x + inBoneWeights.y + inBoneWeights.z + inBoneWeights.w;
    mat4 xform;
    mat4 xformPrev;
    if (wsum < 0.001) {
        xform = inst.model;
        xformPrev = inst.model;
    } else {
        uint base = inst.boneOffset;
        // #651 / SH-6 SIBLING — same per-vertex bone-index clamp as
        // `skin_vertices.comp`. Raster mode is more forgiving than
        // the compute path's BLAS-refit output (a degenerate vertex
        // is one off-screen triangle, not corrupt geometry in the
        // TLAS), but mirroring the clamp keeps the two sites in
        // lockstep so a future regression / corrupt NIF index byte
        // can't silently diverge the two paths.
        uvec4 bIdx = min(inBoneIndices, uvec4(MAX_BONES_PER_MESH - 1u));
        xform =
              inBoneWeights.x * bones[base + bIdx.x]
            + inBoneWeights.y * bones[base + bIdx.y]
            + inBoneWeights.z * bones[base + bIdx.z]
            + inBoneWeights.w * bones[base + bIdx.w];
        xformPrev =
              inBoneWeights.x * bones_prev[base + bIdx.x]
            + inBoneWeights.y * bones_prev[base + bIdx.y]
            + inBoneWeights.z * bones_prev[base + bIdx.z]
            + inBoneWeights.w * bones_prev[base + bIdx.w];
    }

    vec4 worldPos = xform * vec4(inPosition, 1.0);
    vec4 prevWorldPos = xformPrev * vec4(inPosition, 1.0);
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
    // shader does the perspective divide and screen-space delta. Skinned
    // vertices project `prevWorldPos` (composed via `bones_prev`) so the
    // motion vector reflects per-vertex joint motion, not just camera
    // motion. Pre-#641 this used `worldPos` for both, leaving SVGF / TAA
    // to reproject the wrong source pixel on every actor body /
    // hand / face fragment. Both positions are UN-JITTERED — motion
    // must reflect scene motion only, not the per-frame sub-pixel
    // sampling offset.
    fragCurrClipPos = currClip;
    fragPrevClipPos = prevViewProj * prevWorldPos;
    fragSplat0 = inSplat0;
    fragSplat1 = inSplat1;

    // Apply sub-pixel jitter for TAA supersampling. jitter.xy is expressed
    // in NDC, so we scale by clip.w so the offset is constant in NDC after
    // the perspective divide. When jitter = vec2(0) (TAA disabled path),
    // this is a no-op.
    currClip.xy += jitter.xy * currClip.w;
    gl_Position = currClip;
}
