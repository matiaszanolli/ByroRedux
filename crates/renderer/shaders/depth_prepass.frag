#version 460
#extension GL_EXT_nonuniform_qualifier : require

// Depth pre-pass fragment stage. Performs ONLY alpha-test discard so
// the depth buffer ends up correct for the alpha-tested holes (leaves,
// fences, hair, decals). No color outputs — this pass writes depth
// only. Main pass (`triangle.frag` with `layout(early_fragment_tests)`)
// then early-Z's against the populated depth and skips ray queries +
// G-buffer writes for overdrawn fragments. See #779.
//
// Mirrors the alpha-test logic of `triangle.frag:670-684` exactly. Any
// drift between the two would let a fragment pass the prepass discard
// but fail the main-pass test (or vice versa), producing visible
// depth/color mismatch. Pin both sites to the same Gamebryo
// alpha-function constants.

layout(location = 0) in vec2 fragUV;
layout(location = 1) flat in uint fragTextureIndex;
layout(location = 2) flat in uint fragMaterialId;

layout(set = 0, binding = 0) uniform sampler2D textures[];

// Mirror of triangle.frag's GpuMaterial — only the alpha-test relevant
// fields are read, but the std430 stride must match the full 272 B
// layout. Layout-only fields are commented out fields; we just need
// enough trailing slots so `alpha_threshold` lands at offset 44 and
// `alpha_test_func` at offset 84.
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
    GpuMaterial mat = materials[fragMaterialId];
    float aThresh = mat.alphaThreshold;

    // Skip the texture sample entirely for non-alpha-tested draws —
    // the dominant case. Depth gets written from gl_FragDepth implicit
    // = polygon depth.
    if (aThresh <= 0.0) {
        return;
    }

    // BGSM UV transform. Mirrors `triangle.frag:608-609`. Identity for
    // pre-BGSM content (offset=(0,0), scale=(1,1)) so the sample lands
    // at the same texel as the main pass.
    vec2 sampleUV = fragUV * vec2(mat.uvScaleU, mat.uvScaleV)
                  + vec2(mat.uvOffsetU, mat.uvOffsetV);
    float a = texture(textures[nonuniformEXT(fragTextureIndex)], sampleUV).a;

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
