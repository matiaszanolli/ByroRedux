#version 460
#extension GL_EXT_nonuniform_qualifier : require

// Depth pre-pass fragment stage. Performs ONLY alpha-test discard so
// the depth buffer ends up correct for the alpha-tested holes (leaves,
// fences, hair, decals). No color outputs — this pass writes depth
// only. Main pass (`triangle.frag` with `layout(early_fragment_tests)`)
// then early-Z's against the populated depth and skips ray queries +
// G-buffer writes for overdrawn fragments. See #779.
//
// CRITICAL: This shader is paired with `triangle.vert` (NOT a separate
// prepass vertex shader). Sharing the vertex shader guarantees bit-
// identical FP math between prepass and main pass, so depth values
// match exactly under `depth_compare_op = LESS_OR_EQUAL`. A separate
// vertex shader compiled to different SPIR-V can produce one-ULP-
// drifted depth — the main pass then early-Z-rejects valid fragments,
// G-buffer goes unwritten, and lighting blows up (chrome-skin /
// over-bright specular). See revert at 649996a.
//
// Input layout therefore must be a SUBSET of triangle.vert's outputs:
//   loc 0  vec3   fragColor          (unused)
//   loc 1  vec2   fragUV             (used)
//   loc 2  vec3   fragNormal         (unused)
//   loc 3  vec3   fragWorldPos       (unused)
//   loc 4  flat uint fragTexIndex    (used)
//   loc 5  flat int  fragInstanceIndex (used — look up materialId via SSBO)
//   loc 6  vec4   fragCurrClipPos    (unused)
//   loc 7  vec4   fragPrevClipPos    (unused)
//   loc 8  vec4   fragSplat0         (unused)
//   loc 9  vec4   fragSplat1         (unused)
//
// Vulkan allows the fragment shader to consume FEWER inputs than the
// vertex shader provides; unused outputs are silently dropped. We
// declare only the locations we actually read.

layout(location = 1) in vec2 fragUV;
layout(location = 4) flat in uint fragTexIndex;
layout(location = 5) flat in int fragInstanceIndex;

layout(set = 0, binding = 0) uniform sampler2D textures[];

// Mirror of the GpuInstance struct used by triangle.vert / triangle.frag /
// ui.vert. Only `materialId` is actually read here; the rest of the
// fields are layout-only so the std430 stride matches the 112-byte
// instance struct that the rest of the engine writes.
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

// Mirror of triangle.frag's GpuMaterial — the std430 stride must match
// the full 272 B layout. Only the alpha-test fields are actually read,
// but every padded slot must be present so `alpha_threshold` and
// `alpha_test_func` land at the correct std430 offsets.
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

void main() {
    GpuInstance inst = instances[fragInstanceIndex];
    GpuMaterial mat = materials[inst.materialId];
    float aThresh = mat.alphaThreshold;

    // Skip the texture sample entirely for non-alpha-tested draws —
    // the dominant case. Depth gets written from the polygon's
    // implicit gl_FragDepth.
    if (aThresh <= 0.0) {
        return;
    }

    // BGSM UV transform. Mirrors `triangle.frag:608-609`. Identity for
    // pre-BGSM content (offset=(0,0), scale=(1,1)) so the sample lands
    // at the same texel as the main pass.
    vec2 sampleUV = fragUV * vec2(mat.uvScaleU, mat.uvScaleV)
                  + vec2(mat.uvOffsetU, mat.uvOffsetV);
    float a = texture(textures[nonuniformEXT(fragTexIndex)], sampleUV).a;

    // Same Gamebryo alpha-function ladder as triangle.frag:670-684.
    uint aFunc = mat.alphaTestFunc;
    bool pass = true;
    if      (aFunc == 1u) pass = (a <  aThresh);
    else if (aFunc == 2u) pass = (abs(a - aThresh) < 0.004);
    else if (aFunc == 3u) pass = (a <= aThresh);
    else if (aFunc == 4u) pass = (a >  aThresh);
    else if (aFunc == 5u) pass = (abs(a - aThresh) >= 0.004);
    else if (aFunc == 6u) pass = (a >= aThresh);
    else if (aFunc == 7u) pass = false;
    if (!pass) discard;
}
