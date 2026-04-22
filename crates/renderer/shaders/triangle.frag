#version 460
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec3 fragColor;
layout(location = 1) in vec2 fragUV;
layout(location = 2) in vec3 fragNormal;
layout(location = 3) in vec3 fragWorldPos;
layout(location = 4) flat in uint fragTexIndex;
layout(location = 5) flat in int fragInstanceIndex;
layout(location = 6) in vec4 fragCurrClipPos;
layout(location = 7) in vec4 fragPrevClipPos;
// Terrain splat weights — unorm bytes → vec4 in [0,1] (#470).
// Interpolated linearly across the triangle. Non-terrain meshes
// carry zero bytes here; the splat branch is gated on
// `(inst.flags & 8u) != 0u`.
layout(location = 8) in vec4 fragSplat0; // layers 0-3
layout(location = 9) in vec4 fragSplat1; // layers 4-7

// Main render pass has 6 color attachments (Phase 2).
layout(location = 0) out vec4 outColor;        // HDR color (direct light only)
layout(location = 1) out vec2 outNormal;       // octahedral-encoded normal (RG16_SNORM). #275
layout(location = 2) out vec2 outMotion;       // screen-space motion vector
layout(location = 3) out uint outMeshID;       // per-instance ID + 1
layout(location = 4) out vec4 outRawIndirect;  // demodulated indirect light (for SVGF)
layout(location = 5) out vec4 outAlbedo;       // surface color (composite re-multiplies)

// Bindless texture array.
layout(set = 0, binding = 0) uniform sampler2D textures[];

// Per-instance material data from the instance SSBO.
// CRITICAL: all scalars, NO vec3 (vec3 has 16-byte alignment in std430,
// which would mismatch the tightly-packed Rust #[repr(C)] struct).
struct GpuInstance {
    mat4 model;              // offset 0,  64 bytes
    uint textureIndex;       // offset 64, 4 bytes
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
    float alphaThreshold;    // offset 128 — 0.0 = no alpha test (#263)
    uint alphaTestFunc;      // offset 132 — Gamebryo TestFunction enum (#263)
    uint darkMapIndex;       // offset 136 — 0 = no dark map (#264)
    float avgAlbedoR;        // offset 140 — pre-computed average albedo for GI bounce
    float avgAlbedoG;        // offset 144
    float avgAlbedoB;        // offset 148
    // offset 152: per-instance bit flags + packed fields.
    //   bit 0      — non-uniform scale (#273)
    //   bit 1      — NiAlphaProperty blend bit (#263)
    //   bit 2      — caustic source (#321)
    //   bit 3      — terrain splat (#470); enables the ATXT blend loop
    //                against `terrainTiles[flags >> 16]`
    //   bits 16-31 — terrain tile index (only meaningful with bit 3)
    uint flags;
    uint materialKind;       // offset 156 — BSLightingShaderProperty.shader_type (0–19) for variant dispatch (#344). 0 = Default lit.
    // #399 — NiTexturingProperty extra slots (Oblivion glow/detail/gloss).
    // 0 = no map; sampling code below falls through to the inline material constants.
    uint glowMapIndex;       // offset 160 — slot 4 emissive overlay
    uint detailMapIndex;     // offset 164 — slot 2 high-frequency 2× UV overlay
    uint glossMapIndex;      // offset 168 — slot 3 specular mask (.r)
    // #453 — BSShaderTextureSet slots 3/4/5 + POM scalars. Reclaims the
    // old `_padExtraTextures` slot and extends the struct to 192 B.
    uint parallaxMapIndex;   // offset 172 — slot 3 height/POM (0 = disable POM)
    float parallaxHeightScale; // offset 176 — POM depth multiplier
    float parallaxMaxPasses;   // offset 180 — POM ray-march budget
    uint envMapIndex;        // offset 184 — slot 4 env reflection (2D proxy)
    uint envMaskIndex;       // offset 188 — slot 5 env-reflection mask
    // #492 — FO4 BGSM UV transform + material alpha. Plumbing only
    // in this pass; the fragment-shader consumer wiring lands in the
    // #494 follow-up (texture sample uses `uv * scale + offset`,
    // final alpha multiplies by `materialAlpha`).
    float uvOffsetU;         // offset 192
    float uvOffsetV;         // offset 196
    float uvScaleU;          // offset 200
    float uvScaleV;          // offset 204
    float materialAlpha;     // offset 208
    float _uvPad0;           // offset 212
    float _uvPad1;           // offset 216
    float _uvPad2;           // offset 220
    // ── Skyrim+ BSLightingShaderProperty variant payloads (#562) ──
    //
    // Activated by `materialKind` branches below: SkinTint (5),
    // HairTint (6), MultiLayerParallax (11), SparkleSnow (14),
    // EyeEnvmap (16). Default-lit and non-Skyrim meshes keep every
    // slot at zero and never read them.
    float skinTintR;                   // offset 224 — SkinTint tint RGBA
    float skinTintG;                   // offset 228
    float skinTintB;                   // offset 232
    float skinTintA;                   // offset 236
    float hairTintR;                   // offset 240 — HairTint tint RGB
    float hairTintG;                   // offset 244
    float hairTintB;                   // offset 248
    float multiLayerEnvmapStrength;    // offset 252 — MultiLayer envmap mix
    float eyeLeftCenterX;              // offset 256 — EyeEnvmap left iris
    float eyeLeftCenterY;              // offset 260
    float eyeLeftCenterZ;              // offset 264
    float eyeCubemapScale;             // offset 268 — EyeEnvmap cubemap scale
    float eyeRightCenterX;             // offset 272 — EyeEnvmap right iris
    float eyeRightCenterY;             // offset 276
    float eyeRightCenterZ;             // offset 280
    float _eyePad;                     // offset 284
    float multiLayerInnerThickness;    // offset 288 — MultiLayer inner-layer
    float multiLayerRefractionScale;   // offset 292
    float multiLayerInnerScaleU;       // offset 296
    float multiLayerInnerScaleV;       // offset 300
    float sparkleR;                    // offset 304 — SparkleSnow glint
    float sparkleG;                    // offset 308
    float sparkleB;                    // offset 312
    float sparkleIntensity;            // offset 316 → total 320
};

layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer {
    GpuInstance instances[];
};

struct GpuLight {
    vec4 position_radius;  // xyz = position, w = radius
    vec4 color_type;       // rgb = color, w = type (0=point, 1=spot, 2=directional)
    vec4 direction_angle;  // xyz = direction, w = spot angle cosine
};

layout(std430, set = 1, binding = 0) readonly buffer LightBuffer {
    uint lightCount;
    uint _pad0, _pad1, _pad2;
    GpuLight lights[];
};

layout(set = 1, binding = 1) uniform CameraUBO {
    mat4 viewProj;
    mat4 prevViewProj;  // Previous frame's viewProj for motion vectors
    mat4 invViewProj;   // Precomputed inverse(viewProj) for world reconstruction
    vec4 cameraPos;   // xyz = world position, w = frame counter
    vec4 sceneFlags;  // x = RT enabled (1.0), yzw = ambient color (RGB)
    vec4 screen;      // x = width, y = height, z = fog near, w = fog far
    vec4 fog;         // xyz = fog color (RGB), w = fog enabled (1.0)
    vec4 jitter;      // xy = sub-pixel TAA jitter in NDC, zw = reserved
};

layout(set = 1, binding = 2) uniform accelerationStructureEXT topLevelAS;

// Clustered lighting data (written by cluster_cull.comp each frame).
struct ClusterEntry {
    uint offset;
    uint count;
};

layout(std430, set = 1, binding = 5) readonly buffer ClusterGrid {
    ClusterEntry clusters[];
};

layout(std430, set = 1, binding = 6) readonly buffer ClusterLightIndices {
    uint clusterLightIndices[];
};

// SSAO texture (computed after the render pass, read next frame).
layout(set = 1, binding = 7) uniform sampler2D aoTexture;

// Global geometry SSBOs for RT reflection UV lookups.
// Vertex data: position (vec3) + color (vec3) + normal (vec3) + uv (vec2)
// + bone_indices (uvec4) + bone_weights (vec4) + splat_0/1 (2× u32 unorm)
// = 84 bytes/vertex. We only need the UV at offset 36 bytes
// (9 floats into each vertex). See #470 — splat bytes grew the stride.
layout(std430, set = 1, binding = 8) readonly buffer GlobalVertices {
    float vertexData[]; // flat array, stride = 21 floats (84 bytes)
};
layout(std430, set = 1, binding = 9) readonly buffer GlobalIndices {
    uint indexData[];
};

// Per-terrain-tile bindless texture indices for LAND splat layers
// (#470). Fragment shader reads `terrainTiles[tileIdx]` when the
// `INSTANCE_FLAG_TERRAIN_SPLAT` bit (flags bit 3) is set. The tile
// index is packed into the top 16 bits of `flags`.
struct GpuTerrainTile {
    uint layerTextureIndex[8];
};
// Binding 11: RT mipmap glass ray budget counter. The CPU zeroes this
// before each render pass; Phase-3 IOR glass fragments atomically increment
// it. Once the count exceeds GLASS_RAY_BUDGET the fragment falls back to
// the cheaper Fresnel-highlight path for the rest of that frame.
layout(std430, set = 1, binding = 11) coherent buffer RayBudgetBuffer {
    uint rayBudgetCount;
} rayBudget;

layout(std430, set = 1, binding = 10) readonly buffer TerrainTileBuffer {
    GpuTerrainTile terrainTiles[];
};

// Must match cluster_cull.comp constants.
const uint CLUSTER_TILES_X = 16;
const uint CLUSTER_TILES_Y = 9;
const uint CLUSTER_SLICES_Z = 24;
const float CLUSTER_NEAR = 0.1;
const float CLUSTER_FAR = 10000.0;

const float PI = 3.14159265359;

// ── Octahedral normal encoding (Cigolle et al. 2014) ────────────────
// Encodes a unit normal into 2 components for RG16_SNORM storage.
// Saves 50% G-buffer bandwidth vs RGBA16_SNORM. See #275.
vec2 octEncode(vec3 n) {
    n /= (abs(n.x) + abs(n.y) + abs(n.z));
    if (n.z < 0.0) {
        n.xy = (1.0 - abs(n.yx)) * vec2(n.x >= 0.0 ? 1.0 : -1.0,
                                          n.y >= 0.0 ? 1.0 : -1.0);
    }
    return n.xy;
}

vec3 octDecode(vec2 e) {
    vec3 n = vec3(e.xy, 1.0 - abs(e.x) - abs(e.y));
    if (n.z < 0.0) {
        n.xy = (1.0 - abs(n.yx)) * vec2(n.x >= 0.0 ? 1.0 : -1.0,
                                          n.y >= 0.0 ? 1.0 : -1.0);
    }
    return normalize(n);
}

// ── Noise for stochastic shadow rays ────────────────────────────────

// Interleaved gradient noise (Jimenez 2014) — excellent spatial distribution,
// cheap to compute, and when seeded with frame counter gives temporally
// varying patterns that average to smooth penumbra over a few frames.
float interleavedGradientNoise(vec2 fragCoord, float frameCount) {
    vec3 magic = vec3(0.06711056, 0.00583715, 52.9829189);
    float shifted = fract(magic.z * fract(dot(fragCoord + frameCount * vec2(5.588238, 5.588238), magic.xy)));
    return shifted;
}

// Generate a 2D sample on a unit disk using concentric mapping.
// t1, t2 in [0,1] → (x,y) uniformly distributed on unit disk.
vec2 concentricDiskSample(float t1, float t2) {
    float r = sqrt(t1);
    float theta = 2.0 * PI * t2;
    return vec2(r * cos(theta), r * sin(theta));
}

// Build an orthonormal basis from a direction vector (for jittering the ray).
void buildOrthoBasis(vec3 dir, out vec3 tangent, out vec3 bitangent) {
    vec3 up = abs(dir.y) < 0.999 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
    tangent = normalize(cross(up, dir));
    bitangent = cross(dir, tangent);
}

// ── Cosine-weighted hemisphere sampling for GI ─────────────────────

// Generate a cosine-weighted random direction in the hemisphere above N.
// u1, u2 in [0,1] — use noise functions seeded by fragCoord + frameCount.
vec3 cosineWeightedHemisphere(vec3 N, float u1, float u2) {
    float r = sqrt(u1);
    float theta = 2.0 * PI * u2;
    vec3 T, B;
    buildOrthoBasis(N, T, B);
    return normalize(T * (r * cos(theta)) + B * (r * sin(theta)) + N * sqrt(max(1.0 - u1, 0.0)));
}

// ── RT Reflection ───────────────────────────────────────────────────

// Look up UV coordinates at a ray hit point using barycentrics + vertex data.
vec2 getHitUV(uint instanceIdx, uint primitiveIdx, vec2 barycentrics) {
    GpuInstance hitInst = instances[instanceIdx];
    uint vOff = hitInst.vertexOffset;
    uint iOff = hitInst.indexOffset;

    // Triangle vertex indices from the global index buffer.
    uint i0 = indexData[iOff + primitiveIdx * 3 + 0];
    uint i1 = indexData[iOff + primitiveIdx * 3 + 1];
    uint i2 = indexData[iOff + primitiveIdx * 3 + 2];

    // Vertex stride: 21 floats (84 bytes). UV starts at float offset 9 (byte 36).
    const uint STRIDE = 21;
    const uint UV_OFFSET = 9;

    vec2 uv0 = vec2(vertexData[(vOff + i0) * STRIDE + UV_OFFSET],
                     vertexData[(vOff + i0) * STRIDE + UV_OFFSET + 1]);
    vec2 uv1 = vec2(vertexData[(vOff + i1) * STRIDE + UV_OFFSET],
                     vertexData[(vOff + i1) * STRIDE + UV_OFFSET + 1]);
    vec2 uv2 = vec2(vertexData[(vOff + i2) * STRIDE + UV_OFFSET],
                     vertexData[(vOff + i2) * STRIDE + UV_OFFSET + 1]);

    // Barycentric interpolation: bary.x = u (vertex 1), bary.y = v (vertex 2), w = 1-u-v (vertex 0).
    float w = 1.0 - barycentrics.x - barycentrics.y;
    return w * uv0 + barycentrics.x * uv1 + barycentrics.y * uv2;
}

// Cast a reflection ray and return the reflected color.
// Returns (color, hit) where hit is 1.0 if the ray hit geometry, 0.0 if it missed.
//
// Uses gl_RayFlagsTerminateOnFirstHitEXT — we only need ANY opaque hit
// (the first one becomes the reflection surface). Without the flag the
// driver pays "find closest hit" cost across the full maxDist=5000 unit
// reach. Fix #420.
vec4 traceReflection(vec3 origin, vec3 direction, float maxDist) {
    rayQueryEXT rq;
    rayQueryInitializeEXT(
        rq, topLevelAS,
        gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, 0xFF,
        origin, 0.01, direction, maxDist
    );
    rayQueryProceedEXT(rq);

    if (rayQueryGetIntersectionTypeEXT(rq, true) == gl_RayQueryCommittedIntersectionNoneEXT) {
        // Miss — return fog/ambient color.
        return vec4(fog.xyz * 0.5 + sceneFlags.yzw * 0.5, 0.0);
    }

    // Hit — get SSBO instance index via custom index (encodes the draw
    // command position, which matches the SSBO layout). InstanceId would
    // give the TLAS-internal index, which diverges when some meshes lack BLAS.
    int hitInstanceIdx = rayQueryGetIntersectionInstanceCustomIndexEXT(rq, true);
    int hitPrimitiveIdx = rayQueryGetIntersectionPrimitiveIndexEXT(rq, true);
    vec2 hitBary = rayQueryGetIntersectionBarycentricsEXT(rq, true);

    // Look up the hit surface's texture and UV.
    GpuInstance hitInst = instances[hitInstanceIdx];
    uint hitTexIdx = hitInst.textureIndex;
    vec2 hitUV = getHitUV(uint(hitInstanceIdx), uint(hitPrimitiveIdx), hitBary);
    // #494 — apply the hit instance's own BGSM UV transform before
    // sampling. Each hit carries its own per-material offset/scale;
    // the primary path's `baseUV` transform doesn't propagate.
    hitUV = hitUV * vec2(hitInst.uvScaleU, hitInst.uvScaleV)
          + vec2(hitInst.uvOffsetU, hitInst.uvOffsetV);

    // Sample the hit surface's texture.
    vec3 hitColor = texture(textures[nonuniformEXT(hitTexIdx)], hitUV).rgb;

    // Exponential distance attenuation: distant reflections gracefully fade
    // into ambient rather than persisting at near-full strength. The old
    // 1/(1+d*0.005) barely attenuated over the 5000-unit ray length. #320.
    float hitDist = rayQueryGetIntersectionTEXT(rq, true);
    float distFade = exp(-hitDist * 0.0015);

    return vec4(hitColor * distFade, 1.0);
}

// ── Cluster lookup ──────────────────────────────────────────────────

// Compute which cluster this fragment belongs to from screen position + depth.
uint getClusterIndex(vec2 fragCoord, float viewDepth, vec2 screenSize) {
    uint tileX = uint(fragCoord.x / screenSize.x * float(CLUSTER_TILES_X));
    uint tileY = uint(fragCoord.y / screenSize.y * float(CLUSTER_TILES_Y));
    tileX = min(tileX, CLUSTER_TILES_X - 1);
    tileY = min(tileY, CLUSTER_TILES_Y - 1);

    // Exponential depth slicing (must match cluster_cull.comp).
    // log(CLUSTER_FAR / CLUSTER_NEAR) = log(100000) ≈ 11.5129.
    const float LOG_RATIO = 11.512925;
    uint sliceZ = uint(log(max(viewDepth, CLUSTER_NEAR) / CLUSTER_NEAR) / LOG_RATIO * float(CLUSTER_SLICES_Z));
    sliceZ = min(sliceZ, CLUSTER_SLICES_Z - 1);

    return tileX + tileY * CLUSTER_TILES_X + sliceZ * CLUSTER_TILES_X * CLUSTER_TILES_Y;
}

// ── PBR: GGX / Cook-Torrance BRDF ──────────────────────────────────

// Normal Distribution Function (GGX/Trowbridge-Reitz).
float distributionGGX(float NdotH, float roughness) {
    float a = roughness * roughness;
    float a2 = a * a;
    float denom = NdotH * NdotH * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom);
}

// Geometry function (Smith's Schlick-GGX).
float geometrySmith(float NdotV, float NdotL, float roughness) {
    float r = roughness + 1.0;
    float k = (r * r) / 8.0;
    float g1v = NdotV / (NdotV * (1.0 - k) + k);
    float g1l = NdotL / (NdotL * (1.0 - k) + k);
    return g1v * g1l;
}

// Fresnel (Schlick approximation).
vec3 fresnelSchlick(float cosTheta, vec3 F0) {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

// ── Parallax occlusion mapping ──────────────────────────────────────
//
// Standard step + linear-interpolate POM using screen-space derivatives
// (no vertex tangents needed). Height values live in `parallaxMapIdx`'s
// `.r` channel in [0,1]; the surface is displaced INWARD (along -N) as
// the view grazes, so the sampled UV slides along -viewTS.xy.
//
// Returns the displaced UV. When `parallaxMapIdx == 0` the caller
// short-circuits and this function is never entered.
//
// `heightScale` is typically 0.02–0.08 (Bethesda brickwork range);
// `maxPasses` is clamped to [4, 32] — below 4 the stair-step artifacts
// are visible, above 32 the per-pixel cost spikes without a quality
// benefit at typical FOV. Caller feeds `BSShaderPPLightingProperty.
// parallax_max_passes` (default 4) and `parallax_scale` (default 0.04),
// matching the Gamebryo runtime defaults. See #453.
vec2 parallaxDisplaceUV(
    vec2 uv,
    vec3 viewWorld,
    vec3 N,
    vec3 worldPos,
    uint parallaxMapIdx,
    float heightScale,
    float maxPasses
) {
    // Build TBN from screen-space derivatives (same recipe as
    // perturbNormal — keep the derivation identical so the tangent
    // basis is consistent across the two passes).
    vec3 dPdx = dFdx(worldPos);
    vec3 dPdy = dFdy(worldPos);
    vec2 dUVdx = dFdx(uv);
    vec2 dUVdy = dFdy(uv);
    vec3 T = normalize(dPdx * dUVdy.y - dPdy * dUVdx.y);
    vec3 B = normalize(dPdy * dUVdx.x - dPdx * dUVdy.x);
    T = normalize(T - dot(T, N) * N);
    B = cross(N, T);

    // View direction in tangent space. xy is the planar slide the
    // ray makes per unit of depth; z > 0 means we're looking into
    // the surface (which is always the case for back-face-culled
    // draws we parallax-map anyway).
    vec3 V_ts = vec3(dot(viewWorld, T), dot(viewWorld, B), dot(viewWorld, N));
    float vz = max(V_ts.z, 0.05);
    vec2 planarSlide = V_ts.xy / vz * heightScale;

    int steps = int(clamp(maxPasses, 4.0, 32.0));
    float layerDepth = 1.0 / float(steps);
    vec2 deltaUV = planarSlide / float(steps);

    vec2 currentUV = uv;
    float currentDepth = 0.0;
    float sampledHeight =
        texture(textures[nonuniformEXT(parallaxMapIdx)], currentUV).r;
    for (int i = 0; i < steps; ++i) {
        if (currentDepth >= sampledHeight) {
            break;
        }
        currentUV -= deltaUV;
        currentDepth += layerDepth;
        sampledHeight = texture(textures[nonuniformEXT(parallaxMapIdx)], currentUV).r;
    }

    // Linear interpolate against the previous layer for smoother
    // transitions — avoids visible stair-stepping when the step count
    // is low.
    vec2 prevUV = currentUV + deltaUV;
    float afterDepth = sampledHeight - currentDepth;
    float beforeDepth =
        texture(textures[nonuniformEXT(parallaxMapIdx)], prevUV).r
        - (currentDepth - layerDepth);
    float weight = afterDepth / (afterDepth - beforeDepth + 1e-6);
    return mix(currentUV, prevUV, clamp(weight, 0.0, 1.0));
}

// ── Normal mapping via screen-space derivatives ─────────────────────

vec3 perturbNormal(vec3 N, vec3 worldPos, vec2 uv, uint normalMapIdx) {
    // Sample normal map (tangent-space, [0,1] → [-1,1]).
    vec3 tangentNormal = texture(textures[nonuniformEXT(normalMapIdx)], uv).rgb;
    tangentNormal = tangentNormal * 2.0 - 1.0;

    // Build TBN from screen-space derivatives (no vertex tangents needed).
    vec3 dPdx = dFdx(worldPos);
    vec3 dPdy = dFdy(worldPos);
    vec2 dUVdx = dFdx(uv);
    vec2 dUVdy = dFdy(uv);

    // Solve the linear system for T and B.
    vec3 T = normalize(dPdx * dUVdy.y - dPdy * dUVdx.y);
    vec3 B = normalize(dPdy * dUVdx.x - dPdx * dUVdy.x);

    // Ensure TBN is right-handed relative to N.
    T = normalize(T - dot(T, N) * N);
    B = cross(N, T);

    mat3 TBN = mat3(T, B, N);
    return normalize(TBN * tangentNormal);
}

// ── Main ────────────────────────────────────────────────────────────

void main() {
    // Read per-instance material data up-front — parallax-occlusion
    // mapping displaces `fragUV` before the base-albedo sample, and
    // the POM parameters + parallax map index live on the instance.
    GpuInstance inst = instances[fragInstanceIndex];

    // #494 — BGSM-authored UV transform. FO4 BGSM ships explicit
    // offset + scale per material (pre-#494 every sample used the
    // bare `fragUV`, so authored tiling / offsets were ignored).
    // Transform once up-front; downstream `sampleUV` already feeds
    // every texture fetch (base / normal / detail / glow / gloss /
    // dark / env-mask / terrain splat / parallax POM input), so the
    // transformation propagates without per-site edits. Identity
    // defaults (offset=(0,0), scale=(1,1), alpha=1.0) come from
    // `GpuInstance::default()` so pre-BGSM content is byte-identical
    // to the #492 baseline.
    vec2 baseUV = fragUV * vec2(inst.uvScaleU, inst.uvScaleV)
                + vec2(inst.uvOffsetU, inst.uvOffsetV);

    // #453 — parallax-occlusion mapping. Displace UV inward along the
    // view direction projected into tangent space, using the height
    // map in `parallaxMapIndex`. No-op when the instance has no
    // parallax map bound (the dominant case — every non-POM material
    // falls through to the BGSM-transformed UV). The displaced UV is
    // then used for every subsequent texture sample (base, normal,
    // detail, glow, gloss, dark) so the material stays visually
    // consistent. Parallax operates on the post-transform UV so the
    // displaced height lookup lines up with the other samplers.
    vec2 sampleUV = baseUV;
    if (inst.parallaxMapIndex != 0u) {
        vec3 N0 = normalize(fragNormal);
        vec3 V0 = normalize(cameraPos.xyz - fragWorldPos);
        sampleUV = parallaxDisplaceUV(
            baseUV,
            V0,
            N0,
            fragWorldPos,
            inst.parallaxMapIndex,
            inst.parallaxHeightScale,
            inst.parallaxMaxPasses
        );
    }

    vec4 texColor = texture(textures[nonuniformEXT(fragTexIndex)], sampleUV);
    // #494 — BGSM `materialAlpha` multiplier. Applied **before** the
    // alpha-test discard so the authored `alphaThreshold` still
    // operates on the final blended alpha (matching FO4's in-engine
    // order of operations). Identity default is `1.0` so pre-BGSM
    // content is unchanged.
    texColor.a *= inst.materialAlpha;

    // Terrain splat blending (#470). When the instance has the
    // TERRAIN_SPLAT flag set, BTXT (read above via `fragTexIndex`)
    // becomes the base layer, and up to 8 additional layers from the
    // per-tile `GpuTerrainTile` alpha-blend on top in layer order via
    // `mix(prev, layer, weight)`. Matches the UESP-documented ATXT
    // blend semantics. Static meshes skip the branch entirely.
    if ((inst.flags & 8u) != 0u) {
        uint tileIdx = (inst.flags >> 16) & 0xFFFFu;
        GpuTerrainTile tile = terrainTiles[nonuniformEXT(tileIdx)];
        vec4 splat[2] = vec4[2](fragSplat0, fragSplat1);
        for (uint i = 0u; i < 8u; ++i) {
            float w = splat[i / 4u][i & 3u];
            if (w <= 0.0) continue;
            uint layerIdx = tile.layerTextureIndex[i];
            if (layerIdx == 0u) continue; // layer slot unused
            vec4 layerColor = texture(
                textures[nonuniformEXT(layerIdx)], sampleUV);
            texColor.rgb = mix(texColor.rgb, layerColor.rgb, w);
            // Keep texColor.a from the base — terrain is opaque,
            // the alpha-test / alpha-blend machinery below must see
            // the base's alpha, not a splat layer's.
        }
    }

    // Per-instance alpha test (#263). When alphaThreshold > 0 the material
    // has an alpha test enabled; alphaTestFunc selects the Gamebryo comparison:
    //   0=ALWAYS, 1=LESS, 2=EQUAL, 3=LESSEQUAL,
    //   4=GREATER, 5=NOTEQUAL, 6=GREATEREQUAL, 7=NEVER.
    float aThresh = inst.alphaThreshold;
    if (aThresh > 0.0) {
        uint aFunc = inst.alphaTestFunc;
        float a = texColor.a;
        bool pass = true;
        if      (aFunc == 1u) pass = (a <  aThresh);        // LESS
        else if (aFunc == 2u) pass = (abs(a - aThresh) < 0.004); // EQUAL (~1/255)
        else if (aFunc == 3u) pass = (a <= aThresh);        // LESSEQUAL
        else if (aFunc == 4u) pass = (a >  aThresh);        // GREATER
        else if (aFunc == 5u) pass = (abs(a - aThresh) >= 0.004); // NOTEQUAL
        else if (aFunc == 6u) pass = (a >= aThresh);        // GREATEREQUAL
        else if (aFunc == 7u) pass = false;                  // NEVER
        // aFunc == 0 is ALWAYS → pass stays true
        if (!pass) discard;
    }

    // D3D9 fixed-function parity: blend-enabled meshes with
    // NiAlphaProperty threshold=0 still discard fully-transparent
    // texels. FNV picture/table NIFs ship blend=1, test=0 and rely
    // on this implicit discard — without it, noisy authored alpha
    // channels bleed through as ghost-translucent surfaces. Gate
    // only fires on the pure-blend path (inst.flags bit 1) so it
    // can't regress existing alpha-test meshes.
    if ((inst.flags & 2u) != 0u && aThresh == 0.0 && texColor.a < (1.0/255.0)) {
        discard;
    }

    float roughness = inst.roughness;
    float metalness = inst.metalness;
    float emissiveMult = inst.emissiveMult;
    vec3 emissiveColor = vec3(inst.emissiveR, inst.emissiveG, inst.emissiveB);
    float specStrength = inst.specularStrength;
    vec3 specColor = vec3(inst.specularR, inst.specularG, inst.specularB);
    uint normalMapIdx = inst.normalMapIndex;

    // Surface normal — perturbed by normal map if available.
    // Normal sampling uses `sampleUV` so the parallax displacement
    // propagates into the bump detail (otherwise the normal map and
    // albedo would disagree on which texel belongs to each fragment).
    vec3 N = normalize(fragNormal);
    if (normalMapIdx != 0u) {
        N = perturbNormal(N, fragWorldPos, sampleUV, normalMapIdx);
    }

    // ── G-buffer outputs (Phase 1) ────────────────────────────────────
    // Write these before any early return so SVGF has valid per-pixel
    // normal / motion / mesh_id regardless of which lighting path we take.
    outNormal = octEncode(N);

    // Screen-space motion vector: current-pixel UV → previous-pixel UV.
    // Perspective divide both clip-space positions to get NDC, halve to
    // go from NDC delta [-2,2] to UV delta [-1,1]. SVGF's temporal pass
    // reads it as: prev_uv = current_uv - motion.
    vec2 currNDC = fragCurrClipPos.xy / fragCurrClipPos.w;
    vec2 prevNDC = fragPrevClipPos.xy / fragPrevClipPos.w;
    outMotion = (currNDC - prevNDC) * 0.5;

    // Mesh ID: instance index + 1 so that "0" (clear value for background
    // pixels) is distinct from "instance 0". Caps at uint15 max — bit 15
    // (0x8000) is reserved for the ALPHA_BLEND_NO_HISTORY flag that
    // forces TAA + SVGF to disable temporal reuse on transparent
    // fragments. Phase 1 of Tier C glass — without this the TAA history
    // reprojects the wrong source pixel across glass z-fight flips,
    // amplifying sub-pixel jitter into cross-hatch moiré.
    uint meshIdBase = (uint(fragInstanceIndex) + 1u) & 0x7FFFu;
    bool alphaBlendFrag = (inst.flags & 2u) != 0u;
    outMeshID = meshIdBase | (alphaBlendFrag ? 0x8000u : 0u);

    // View direction. NdotV is clamped to 0.05 (~87°) to prevent the
    // Cook-Torrance `D*G*F / (4*NdotV*NdotL)` specular term from blowing
    // up at grazing view angles — the microfacet model is not valid in
    // that regime anyway, and the unclamped version produced bright
    // triangular specular hotspots along wall surfaces when the camera
    // was looking along them.
    vec3 V = normalize(cameraPos.xyz - fragWorldPos);
    float NdotV = max(dot(N, V), 0.05);

    bool rtEnabled = sceneFlags.x > 0.5;

    // ── RT mipmap LOD — analogous to textureGrad mip selection ──────────
    //
    // World-space size of one screen pixel at this fragment. Larger footprint
    // = more distant or smaller object = cheaper ray tier acceptable.
    //
    //   rtLOD 0–1: arm's reach (~0–2 m) — full IOR glass, RT reflections, GI
    //   rtLOD 1–2: room scale (~2–6 m)  — portal glass, RT reflections, GI
    //   rtLOD 2–3: background (~6–20 m) — Fresnel highlight only, no RT rays
    //   rtLOD ≥ 3: far field           — plain alpha-blend, no glass effects
    //
    // The LOD_SCALE constant controls the transition distances; raise to
    // favour quality (more pixels at tier 0), lower to favour performance.
    const float RT_LOD_SCALE   = 6.0;
    const float RT_LOD_REFLECT = 2.0;  // metal reflection ray ceiling
    const float RT_LOD_GI      = 2.0;  // GI ray ceiling
    const float RT_LOD_IOR     = 1.0;  // Phase-3 glass IOR ceiling (budget-gated)
    float rtFootprint = max(length(dFdx(fragWorldPos)), length(dFdy(fragWorldPos)));
    float rtLOD = clamp(log2(rtFootprint * RT_LOD_SCALE), 0.0, 3.0);

    // Base reflectance: dielectrics use 0.04, metals use albedo color.
    vec3 albedo = texColor.rgb * fragColor;

    // Dark / multiplicative lightmap (#264): baked shadow modulation.
    if (inst.darkMapIndex != 0u) {
        vec3 darkSample = texture(textures[nonuniformEXT(inst.darkMapIndex)], sampleUV).rgb;
        albedo *= darkSample;
    }

    // #399 — NiTexturingProperty slot 2 detail overlay. Sampled at
    // 2× UV scale (Gamebryo convention — high-frequency variation at
    // half the wavelength of the base diffuse) and modulated into the
    // albedo. Center the modulation around 1.0 so a 0.5 grey detail
    // sample is a no-op rather than halving the surface brightness.
    if (inst.detailMapIndex != 0u) {
        vec3 detailSample = texture(
            textures[nonuniformEXT(inst.detailMapIndex)],
            sampleUV * 2.0
        ).rgb;
        albedo *= detailSample * 2.0;
    }

    // #399 — NiTexturingProperty slot 3 gloss / specular mask. The
    // .r channel is per-texel specular strength (polished metal trim
    // vs. dull leather straps on the same armor mesh). Multiply the
    // inline `specularStrength` constant.
    if (inst.glossMapIndex != 0u) {
        float glossSample = texture(
            textures[nonuniformEXT(inst.glossMapIndex)],
            sampleUV
        ).r;
        specStrength *= glossSample;
    }

    // #399 — NiTexturingProperty slot 4 glow / self-illumination map.
    // Multiplies the inline `emissiveColor`. Enchanted weapon runes,
    // sigil stones, lava — all author the actual glow shape here and
    // leave the inline emissive as a tint constant. The sampled RGB
    // becomes the new emissive base; downstream emissive code below
    // multiplies by `emissiveMult` and any dark-cell amplification
    // unchanged.
    if (inst.glowMapIndex != 0u) {
        vec3 glowSample = texture(
            textures[nonuniformEXT(inst.glowMapIndex)],
            sampleUV
        ).rgb;
        emissiveColor *= glowSample;
    }

    // ── #562 Skyrim+ BSLightingShaderProperty variant ladder ────────
    //
    // Enums from `BSLightingShaderType` (nif.xml): 0 Default · 1 Envmap
    // · 2 Glow · 3 Parallax · 4 FaceTint · 5 SkinTint · 6 HairTint
    // · 7 ParallaxOcc · 8 MultiIndexSnow · 11 MultiLayerParallax
    // · 14 SparkleSnow · 16 EyeEnvmap · 19 MultiTexture.
    //
    // Already dispatched elsewhere by data presence (no ladder entry):
    //   · 1  Envmap       — `inst.envMapIndex != 0u` fed by POM/PBR path.
    //   · 2  Glow         — `inst.glowMapIndex` above.
    //   · 3  Parallax     — `inst.parallaxMapIndex` (POM ray-march).
    //   · 7  ParallaxOcc  — same path as 3.
    //   · 100 Glass       — engine-synthesized; handled below.
    //
    // The branches here cover the SKIN/HAIR/SPARKLE/MULTILAYER/EYE set
    // whose trailing payload (`skinTint*`, `hairTint*`, `sparkle*`,
    // `multiLayer*`, `eyeLeftCenter*`, …) can't be derived from
    // textures and must ride on GpuInstance. See plan in issue #562.
    if (inst.materialKind == 5u) {
        // SkinTint — multiply albedo by the authored tint color,
        // weighted by `skinTintA` so an alpha of 0 cleanly disables
        // the tint (identity = texture). FO76's `Fo76SkinTint` ships
        // a real alpha; pre-FO76 Skyrim `SkinTint` fills alpha=1.0.
        vec3 skinTint = vec3(inst.skinTintR, inst.skinTintG, inst.skinTintB);
        albedo = mix(albedo, albedo * skinTint, inst.skinTintA);
    } else if (inst.materialKind == 6u) {
        // HairTint — unconditional albedo multiply by the authored
        // hair color. No alpha field; author-set zero means
        // intentional black-out (never seen in vanilla).
        vec3 hairTint = vec3(inst.hairTintR, inst.hairTintG, inst.hairTintB);
        albedo *= hairTint;
    } else if (inst.materialKind == 14u) {
        // SparkleSnow — hash-driven glint overlay on top of the snow
        // albedo. `sparkleRGB` is the glint color (author-authored;
        // typical `(1, 1, 1)`), `sparkleIntensity` scales the overall
        // contribution. A simple per-pixel noise hash produces a
        // stable sparkle pattern that doesn't depend on view direction.
        // Full physically-based snow rendering (Bethesda's
        // multi-octave noise + view-dependent subsurface scattering)
        // is follow-up work.
        vec2 sparkleSeed = floor(sampleUV * 512.0);
        float sparkleHash = fract(sin(dot(sparkleSeed, vec2(12.9898, 78.233))) * 43758.5453);
        float glint = step(0.995, sparkleHash) * inst.sparkleIntensity;
        vec3 sparkleColor = vec3(inst.sparkleR, inst.sparkleG, inst.sparkleB);
        albedo += sparkleColor * glint;
    }
    // Variant stubs — data already lands in GpuInstance; the full
    // shading branches ship in follow-up issues. Listed explicitly so
    // a future reader searching by `materialKind == N` finds the
    // intended consumers.
    //   · materialKind == 11 (MultiLayerParallax): read
    //       `multiLayerInnerThickness`, `multiLayerRefractionScale`,
    //       `multiLayerInnerScaleU/V`, `multiLayerEnvmapStrength`.
    //       Compute a second sample of `textures[inst.textureIndex]`
    //       offset along `V` by `refractionScale * innerThickness`,
    //       blended with the outer layer by a Fresnel × envmapStrength
    //       mix. See `ShaderTypeData::MultiLayerParallax`.
    //   · materialKind == 16 (EyeEnvmap): use
    //       `eyeLeftCenter` / `eyeRightCenter` + `eyeCubemapScale` to
    //       reflect a cubemap sample off the iris center (not the
    //       sclera). Requires a per-instance cubemap descriptor binding
    //       that doesn't exist yet; add alongside the FO4 env-reflect
    //       cubemap path.

    vec3 F0 = mix(vec3(0.04), albedo, metalness);

    // Precompute the emissive term. Per Gamebryo's D3D9 FFP material model
    // (matching NiMaterialProperty / BSShaderPPLighting), the final color is
    //   ambient + diffuse + specular + emissive
    // — emissive is ADDITIVE on top of the normal lighting loop, not a
    // replacement for it. Previously this shader bypassed the entire Lo
    // loop for any emissiveMult > 0 mesh and wrote `ambient + emissive`
    // directly, which:
    //   (1) stopped oil lanterns from receiving direct light from sibling
    //       lanterns (clusters rendered individually flat-lit), and
    //   (2) zeroed outRawIndirect, which fed SVGF with zero indirect on
    //       lantern pixels and bled darkness into neighbors via the
    //       spatial filter.
    // Modulate by albedo so textured glass globes tint/shape the glow,
    // and clamp to tame HDR blowout on bright emissive texels before ACES.
    float emissiveLum = dot(emissiveColor, vec3(0.2126, 0.7152, 0.0722));
    vec3 emissive = vec3(0.0);
    if (emissiveMult > 0.01 && emissiveLum > 0.01) {
        emissive = min(emissiveColor * emissiveMult * albedo, vec3(1.5));
    }

    // ── Glass / transparent refraction ──────────────────────────────
    //
    // Glass surfaces (low roughness, non-metallic, semi-transparent alpha)
    // get screen-space refraction: the scene behind the glass is sampled
    // with a UV offset proportional to the surface normal, producing a
    // natural distortion effect. Fresnel controls the reflection/transmission
    // split — at grazing angles glass reflects more, at direct incidence
    // it's mostly transparent.
    // Glass detection: only surfaces explicitly classified as glass by the
    // material classifier (roughness ≤ 0.1) AND with visible transparency.
    // This excludes windows (which are just alpha-blended, not refractive)
    // and only catches actual glass/crystal/gem objects.
    // ── Window light portals ─────────────────────────────────────────
    //
    // In every Bethesda game (Morrowind→Starfield), interior windows are
    // fake portals — the exterior visible through them isn't real geometry.
    // We cast a ray through the window surface: if it escapes the cell
    // (no hit within range), we treat it as seeing sky and transmit
    // exterior light through the window, tinted by its texture color.
    //
    // Window/glass detection driven by the CPU-classified material_kind.
    //
    // Pre-Phase-2 this gate was a per-fragment heuristic on
    // `isAlphaBlend && metalness < 0.1 && texColor.a ∈ (0.02, 0.6)`.
    // `texColor.a` is sampled per-texel, so patterned textures with
    // semi-transparent regions (e.g. wire-mesh glass, etched panes)
    // toggled pixels in and out of the glass path across their own
    // surface — each flicker picked up / dropped RT reflection rays.
    //
    // Phase 2 moves classification to the CPU: render.rs tags every
    // `(alpha_blend && !is_decal && metalness < 0.3)` draw with
    // `MATERIAL_KIND_GLASS = 100u`, and that bit is stable across the
    // whole mesh. `isWindow` refines to the lower-alpha subset for
    // the portal-escape branch below — still texColor-gated but now
    // nested under a stable parent classification. See Tier C Phase 2.
    const uint MATERIAL_KIND_GLASS = 100u;
    bool isAlphaBlend = (inst.flags & 2u) != 0u;
    bool isGlass = inst.materialKind == MATERIAL_KIND_GLASS && roughness < 0.35;
    bool isWindow = isGlass && texColor.a < 0.5 && texColor.a > 0.02;

    // RT mipmap glass tier downgrade:
    //   Tier 3 (rtLOD ≥ 3.0): plain alpha-blend — glass effects disabled entirely.
    //   Tier 2 (rtLOD ≥ 2.0): Fresnel highlight only — window portal ray suppressed.
    //   Tiers 0–1 keep isGlass/isWindow as-is.
    if (rtLOD >= 3.0) {
        isGlass  = false;
        isWindow = false;
    } else if (rtLOD >= 2.0) {
        isWindow = false;
    }

    if (isWindow && rtEnabled) {
        // Fire the portal-escape ray along the surface OUTWARD normal,
        // not along `-V` (camera look direction). Pre-#421 the ray
        // used `-V`, which at oblique viewing angles continued along
        // the camera's line of sight and hit the interior sidewall /
        // ceiling / opposite wall — the `!hitsInterior` check failed
        // and the fragment fell through to the opaque alpha-blend
        // path. Only near-perpendicular window fragments lit up.
        // `-N` fires straight through the glass plane to the outside
        // regardless of viewing angle, which is what portal semantics
        // require. See #421 / audit REN-RT-H3.
        //
        // Defensive grazing-angle gate: at very oblique incidence
        // (dot < 0.1, ~84° from normal) portal escape is ambiguous
        // anyway — the glass is effectively edge-on and the fragment
        // barely covers a pixel. Fall back to the opaque alpha-blend
        // path rather than fire a ray whose hit result is noisy.
        float windowFacing = dot(-V, N);
        bool hitsInterior = true; // pessimistic default → alpha-blend path.
        if (windowFacing > 0.1) {
            vec3 throughDir = -N;
            rayQueryEXT windowRQ;
            rayQueryInitializeEXT(
                windowRQ, topLevelAS,
                gl_RayFlagsTerminateOnFirstHitEXT | gl_RayFlagsOpaqueEXT,
                0xFF,
                fragWorldPos - N * 0.15, // start slightly outside the pane (#269 R2-08: reduced from 0.5 to shrink blind zone)
                0.05,
                throughDir,
                2000.0 // if nothing hit within 2000 units, it's "outside"
            );
            rayQueryProceedEXT(windowRQ);
            hitsInterior = (rayQueryGetIntersectionTypeEXT(windowRQ, true)
                != gl_RayQueryCommittedIntersectionNoneEXT);
        }

        if (!hitsInterior) {
            // Ray escaped the cell — this window sees sky.
            // Blend the transmitted sky light with whatever opaque surface
            // was already drawn behind this pixel (e.g. the window frame).
            // Use the glass texture's alpha to control the blend — clear
            // glass (low alpha) shows mostly sky, tinted glass shows more
            // of the glass color.
            vec3 skyColor = vec3(0.6, 0.75, 1.0); // clear day sky
            // Use the authored glass color directly instead of biasing
            // toward white. Pre-fix this mix started from pure white
            // and leaned heavily that way for low-alpha clear glass
            // (α≈0.1 typical), producing blown-out panes. A vec3(0.15)
            // floor keeps very-dark authored glass from killing the
            // transmitted sky entirely.
            vec3 windowTint = max(texColor.rgb, vec3(0.15));
            vec3 transmitted = skyColor * windowTint * 0.3;
            // Write with the glass texture alpha so the window frame
            // (opaque, already in the framebuffer) shows through the
            // frame border areas. The alpha blend pipeline composites:
            //   result = transmitted × alpha + framebuffer × (1 - alpha)
            outColor = vec4(transmitted, texColor.a);
            outRawIndirect = vec4(0.0);
            outAlbedo = vec4(albedo, 1.0);
            return;
        }
    }

    float glassFresnel = 0.0;
    if (isGlass) {
        glassFresnel = fresnelSchlick(NdotV, vec3(0.04)).r;
        specStrength = max(specStrength, 3.0);
        F0 = vec3(0.04);
    }

    // RT glass Phase 3: IOR refraction + reflection for tier-0 fragments
    // (rtLOD < RT_LOD_IOR = 1.0, i.e. arm's-reach glass objects).
    // Gated by the per-frame ray budget counter — atomicAdd claims 2 units
    // (reflection + refraction) and falls back to the Fresnel-highlight path
    // when the budget is exhausted (glassFresnel + specStrength still active).
    // Window surfaces (isWindow) are excluded here — they already returned above.
    const uint GLASS_RAY_BUDGET = 512u;
    bool glassIORAllowed = isGlass && rtEnabled && !isWindow && rtLOD < RT_LOD_IOR;
    if (glassIORAllowed) {
        uint old = atomicAdd(rayBudget.rayBudgetCount, 2u);
        glassIORAllowed = (old + 2u <= GLASS_RAY_BUDGET);
    }
    if (glassIORAllowed) {
        // ── RT glass (Phase 3) ────────────────────────────────────────
        //
        // Physically-motivated reflect + refract + Fresnel mix:
        //
        //   reflColor = traceReflection(R)         — surface-reflected scene
        //   refrColor = traceRefraction(T)         — scene seen through glass
        //   F         = fresnelSchlick(NdotV, 0.04) — dielectric Fresnel
        //   surface   = mix(refrColor, reflColor, F)
        //
        // Glass has IOR ≈ 1.5 (soda-lime, window glass, drinking glass).
        // At normal incidence F ≈ 0.04 (96% transmits), at grazing
        // F → 1.0 (near-mirror). This is what separates the Phase 3
        // path from Phase 1's "fire `-V` through" — Phase 1 pretended
        // glass had no IOR and transmission was a straight line.
        const float GLASS_IOR = 1.5;
        const float ETA_AIR_TO_GLASS = 1.0 / GLASS_IOR;

        // View-aligned normal. The Phase 1 two-sided alpha-blend split
        // emits back-face then front-face passes; on the back-face pass
        // N points away from the camera (dot(N,V) < 0). We need the
        // camera-facing normal for refract() + the reflection hemisphere
        // to make physical sense. Flip when back-facing.
        vec3 N_view = dot(N, V) < 0.0 ? -N : N;
        float NdotV_v = max(dot(N_view, V), 0.05);
        float fresnelScalar = fresnelSchlick(NdotV_v, vec3(0.04)).r;

        // Reflection ray — picks up ceiling fixtures, sky through windows,
        // walls on either side of the pane.
        vec3 R = reflect(-V, N_view);
        vec4 reflRay = traceReflection(fragWorldPos + N_view * 0.05, R, 3000.0);
        vec3 reflColor = reflRay.rgb;

        // Refraction ray — the transmitted half. Snell's law:
        //   sin(θ_t) = (n1/n2) · sin(θ_i)
        // refract() returns vec3(0.0) on total internal reflection,
        // which happens when (n1/n2) · sin(θ_i) > 1. Going air→glass
        // (eta < 1) TIR never triggers, but we handle it defensively
        // for back-face content where N_view might flip the effective
        // eta. On TIR we reuse the reflection as the transmitted ray
        // — physically it's "all light bounces back on the inside".
        vec3 refractDir = refract(-V, N_view, ETA_AIR_TO_GLASS);
        bool totalInternalReflection = dot(refractDir, refractDir) < 0.0001;

        vec3 refrColor;
        if (totalInternalReflection) {
            refrColor = reflColor;
        } else {
            // Origin offset: step INTO the glass along -N_view so the
            // refraction ray starts on the back side of the surface and
            // the ray doesn't self-intersect the pane we're shading.
            // 0.1 units is ~1mm in Bethesda scale — safely past the
            // thinnest wall geometry + inside any drinking glass/bottle.
            rayQueryEXT refrRQ;
            rayQueryInitializeEXT(
                refrRQ, topLevelAS,
                gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, 0xFF,
                fragWorldPos - N_view * 0.1, 0.05, refractDir, 2000.0
            );
            rayQueryProceedEXT(refrRQ);

            if (rayQueryGetIntersectionTypeEXT(refrRQ, true)
                == gl_RayQueryCommittedIntersectionNoneEXT) {
                // Escaped scene — fall back to sky tint (matches window
                // portal contract). Interior cells will still see this
                // as a soft blue because no geometry lies behind the
                // refraction ray; for lit interiors that's acceptable
                // for the rare "looking out" angle.
                refrColor = vec3(0.6, 0.75, 1.0);
            } else {
                int tIdx = rayQueryGetIntersectionInstanceCustomIndexEXT(refrRQ, true);
                int tPrim = rayQueryGetIntersectionPrimitiveIndexEXT(refrRQ, true);
                vec2 tBary = rayQueryGetIntersectionBarycentricsEXT(refrRQ, true);
                GpuInstance tInst = instances[tIdx];
                vec2 tUV = getHitUV(uint(tIdx), uint(tPrim), tBary);
                // #494 — BGSM UV transform on the refraction hit too.
                tUV = tUV * vec2(tInst.uvScaleU, tInst.uvScaleV)
                    + vec2(tInst.uvOffsetU, tInst.uvOffsetV);
                vec3 tAlbedo = texture(textures[nonuniformEXT(tInst.textureIndex)], tUV).rgb;

                // Apply a lighting estimate to the hit albedo. The raw
                // texture without lighting reads as "raw diffuse" which
                // in dim interiors looks pitch-black through the glass
                // (since a brown wood wall is texColor≈0.2 regardless
                // of actual illumination). We multiply by the cell's
                // ambient floor + a small base so the refracted scene
                // matches the LIT look of the world, not its raw albedo.
                // Same cheap pattern the 1-bounce GI ray uses at :1179
                // — ambient × albedo instead of full shading.
                vec3 ambientLitFloor = sceneFlags.yzw + vec3(0.25);
                refrColor = tAlbedo * ambientLitFloor;

                // Distance attenuation — faraway refracted content
                // falls off gently so thick glass stacks don't become
                // view-to-infinity spotlights. Gentler slope than the
                // Phase 1 through-ray (0.002 vs 0.01) — we want to
                // preserve more of the refracted detail.
                float tDist = rayQueryGetIntersectionTEXT(refrRQ, true);
                refrColor *= 1.0 / (1.0 + tDist * 0.002);
            }
        }

        // ── Transmission split: clear-glass vs decal-opaque ────────────
        //
        // Bethesda glass textures fall into two categories that need
        // different shading even though both pass the MATERIAL_KIND_GLASS
        // classification:
        //
        //   Case A — tinted clear glass (pitchers, bottles, drinking
        //     glasses): texture alpha is relatively uniform (~0.2–0.4),
        //     diffuse RGB carries a soft tint plus some surface-detail
        //     noise. We want the uniform tint, not the noise — so
        //     sample the texture at a blurred mip level for absorption
        //     and multiply the refracted view by that.
        //
        //   Case B — clear pane with decals (broken windows, etched
        //     glass, dirt patches): alpha is bimodal — ~0 in clear
        //     regions, ~1 where the decal lives. Diffuse carries the
        //     decal content (leaves, cracks, painted sign). Decal
        //     pixels need to render AS-IS on the glass surface; the
        //     clear pixels around them refract as usual.
        //
        // Per-texel `smoothstep(0.3, 0.7, α)` drives the mix:
        // low α → pure clear-glass (refracted + mip-tinted), high α →
        // pure decal (raw texel), intermediate → soft transition. The
        // mip-6 sample erases the fine crosshatch pattern on
        // drinkingglass01.dds that was overlaying every refracted pixel
        // at ~29 FPS — visible as a wire-mesh shimmer on transparent
        // cups.
        float decalWeight = smoothstep(0.3, 0.7, texColor.a);
        vec3 glassTint = textureLod(
            textures[nonuniformEXT(inst.textureIndex)],
            sampleUV,
            6.0
        ).rgb;
        vec3 clearGlass = refrColor * glassTint;
        vec3 transmission = mix(clearGlass, texColor.rgb, decalWeight);

        // Fresnel mix. At normal incidence F ≈ 0.04 — mostly transmitted.
        // At grazing F → 1.0 — near-mirror. Classic "see-through
        // straight on, shiny at the edges" glass look.
        vec3 glassSurface = mix(transmission, reflColor, fresnelScalar);

        // Alpha: lift toward opaque at grazing angles so the glass
        // silhouette always reads. At normal incidence, honor the
        // authored texColor.a so the glass stays genuinely see-through.
        float outAlpha = mix(texColor.a, 1.0, fresnelScalar * 0.8);

        outColor = vec4(glassSurface, outAlpha);
        outNormal = octEncode(N_view);
        outRawIndirect = vec4(0.0);
        outAlbedo = vec4(albedo, 1.0);
        return;
    }

    // Ambient base from cell lighting — LIGHTING ONLY, no local albedo.
    // Albedo is re-applied in the composite pass so SVGF temporal/spatial
    // filtering operates on a texture-free lighting signal. See #268.
    vec3 ambient = sceneFlags.yzw * (1.0 - metalness);
    vec3 Lo = vec3(0.0); // Accumulated outgoing radiance.

    // ── RT reflection for metallic/glossy surfaces ──────────────────
    //
    // Metals reflect their environment. Cast a reflection ray, weight by
    // Fresnel, and add to the direct path. We deliberately route through
    // Lo (not ambient/outRawIndirect) because reflResult.rgb already
    // carries the HIT surface's albedo — sending it through the indirect
    // path would have composite multiply by the LOCAL albedo a second
    // time, losing 30-50% of reflection energy on tinted metals (#315).
    //
    // For metals, F ≈ F0 = albedo at normal incidence, so `* F * metalness`
    // provides the single, correct metal-tint modulation that composite
    // would otherwise apply via `indirect * albedo`. The direct path is
    // not albedo-modulated by composite (Lo already bakes in albedo per
    // dielectric kD), so this addition stays at full intensity.
    if (rtEnabled && metalness > 0.3 && roughness < 0.6 && rtLOD < RT_LOD_REFLECT) {
        // Roughness-driven ray jitter: GGX lobe widens with roughness^2.
        // Single sample per pixel, accumulated via temporal noise (IGN seeded
        // by frame counter). SVGF temporal filter smooths the result. #320.
        vec3 R = reflect(-V, N);
        float frameCount = cameraPos.w;
        float n1 = interleavedGradientNoise(gl_FragCoord.xy, frameCount + 89.0);
        float n2 = interleavedGradientNoise(gl_FragCoord.xy + vec2(53.7, 191.3), frameCount + 113.0);
        vec3 T2, B2;
        buildOrthoBasis(R, T2, B2);
        vec2 cone = concentricDiskSample(n1, n2) * (roughness * roughness);
        vec3 jitteredR = normalize(R + T2 * cone.x + B2 * cone.y);
        vec4 reflResult = traceReflection(fragWorldPos + N * 0.1, jitteredR, 5000.0);

        // Fresnel-weighted reflection: stronger at grazing angles.
        vec3 F = fresnelSchlick(NdotV, F0);

        // Roughness blurs the reflection: mix toward raw ambient (without
        // the per-pixel metalness factor that line 495 zeroed) for rough
        // metals so high-roughness surfaces still see environment color
        // when the ray miss path returns weak signal.
        float reflClarity = 1.0 - roughness;
        vec3 ambientFallback = sceneFlags.yzw;
        vec3 envColor = mix(ambientFallback, reflResult.rgb, reflClarity * reflResult.a);

        Lo += envColor * F * metalness;
    }

    // World-space distance from camera for cluster depth slicing.
    // Must match cluster_cull.comp's sliceDepth() which uses world-space distance.
    float worldDist = length(fragWorldPos - cameraPos.xyz);
    uint clusterIdx = getClusterIndex(gl_FragCoord.xy, worldDist, screen.xy);

    if (lightCount == 0) {
        // Fallback: single directional light.
        vec3 L = normalize(vec3(0.4, 0.8, 0.5));
        vec3 H = normalize(V + L);
        float NdotL = max(dot(N, L), 0.0);
        float NdotH = max(dot(N, H), 0.0);
        float HdotV = max(dot(H, V), 0.0);

        float D = distributionGGX(NdotH, roughness);
        float G = geometrySmith(NdotV, NdotL, roughness);
        vec3 F = fresnelSchlick(HdotV, F0);

        vec3 kD = (1.0 - F) * (1.0 - metalness);
        vec3 specular = (D * G * F) / max(4.0 * NdotV * NdotL, 0.01);
        Lo = (kD * albedo / PI + specular * specStrength * specColor) * vec3(0.8) * NdotL;
    } else {
        // Clustered lighting with streaming RIS shadow sampling.
        //
        // Each fragment maintains K weighted reservoirs that sample the
        // full cluster proportional to each light's unshadowed luminance
        // contribution (target pdf). A shadow ray is cast for each
        // selected reservoir; the shadowed-subtraction is unbiased by
        // the reservoir weight W = resWSum / (K · w_sel).
        //
        // Replaces the previous deterministic top-K, which had a
        // pathological failure: any light outside the top-K at a given
        // fragment was treated as unshadowed forever at that pixel.
        // WRS gives every light non-zero selection probability, so
        // large occluders blocking any light (not just the brightest)
        // cast shadows on large receivers. Temporal variance is
        // absorbed by SVGF.
        //
        // Full ReSTIR-DI (temporal + spatial reservoir reuse across
        // frames/neighbors) is a separate milestone — needs a GBuffer
        // reservoir attachment and a dedicated resample compute pass.
        //
        // Distance fallback: shadows fade out past camera range at
        // 4000–6000 units (~57–86 m at Bethesda's ~70u/m) so interior
        // halls self-shadow end-to-end. 12 GB VRAM budget makes the
        // extra ray cost fine.
        float shadowFade = 1.0 - smoothstep(4000.0, 6000.0, worldDist);
        const uint NUM_RESERVOIRS = 8;
        // Cap per-reservoir unbiasing weight to tame fireflies when a
        // dim light is sampled in a cluster dominated by bright ones.
        // 64× matches the ratio of a dim fill light to a hero light.
        const float RESERVOIR_W_CLAMP = 64.0;

        uint  resLight[NUM_RESERVOIRS];
        float resWSel[NUM_RESERVOIRS];
        vec3  resRadiance[NUM_RESERVOIRS];
        float resWSum = 0.0;
        for (uint s = 0; s < NUM_RESERVOIRS; s++) {
            resLight[s] = 0xFFFFFFFFu;
            resWSel[s] = 0.0;
            resRadiance[s] = vec3(0.0);
        }
        float resFrameSeed = cameraPos.w;

        ClusterEntry cluster = clusters[clusterIdx];
        for (uint ci = 0; ci < cluster.count; ci++) {
            uint i = clusterLightIndices[cluster.offset + ci];
            vec3 lightPos = lights[i].position_radius.xyz;
            float radius = lights[i].position_radius.w;
            vec3 lightColor = lights[i].color_type.rgb;
            float lightType = lights[i].color_type.w;

            vec3 L;
            float dist;
            float atten;

            if (lightType < 0.5) {
                // Point light — Gamebryo-matching 1/d attenuation.
                vec3 toLight = lightPos - fragWorldPos;
                dist = length(toLight);
                L = toLight / max(dist, 0.001);
                float effectiveRange = radius * 4.0;
                float ratio = dist / max(effectiveRange, 1.0);
                float window = clamp(1.0 - ratio * ratio, 0.0, 1.0);
                atten = window / (1.0 + dist * 0.01);
            } else if (lightType < 1.5) {
                // Spot light — same 1/d attenuation + cone factor.
                vec3 toLight = lightPos - fragWorldPos;
                dist = length(toLight);
                L = toLight / max(dist, 0.001);
                vec3 spotDir = normalize(lights[i].direction_angle.xyz);
                float spotAngle = lights[i].direction_angle.w;
                float effectiveRange = radius * 4.0;
                float ratio = dist / max(effectiveRange, 1.0);
                float window = clamp(1.0 - ratio * ratio, 0.0, 1.0);
                atten = window / (1.0 + dist * 0.01);
                float spotFactor = dot(-L, spotDir);
                atten *= clamp((spotFactor - spotAngle) / (1.0 - spotAngle), 0.0, 1.0);
            } else {
                // Directional light.
                L = normalize(lights[i].direction_angle.xyz);
                dist = 10000.0;
                atten = 1.0;
            }

            float NdotL = max(dot(N, L), 0.0);
            float contribution = NdotL * atten;
            if (contribution < 0.001) {
                continue;
            }

            // PBR: Cook-Torrance BRDF (unshadowed).
            vec3 H = normalize(V + L);
            float NdotH = max(dot(N, H), 0.0);
            float HdotV = max(dot(H, V), 0.0);

            float D = distributionGGX(NdotH, roughness);
            float G = geometrySmith(NdotV, NdotL, roughness);
            vec3 F = fresnelSchlick(HdotV, F0);

            vec3 kD = (1.0 - F) * (1.0 - metalness);
            vec3 specular = (D * G * F) / max(4.0 * NdotV * NdotL, 0.01);
            vec3 unshadowedRadiance = lightColor * atten;
            vec3 brdfResult = (kD * albedo + specular * specStrength * specColor) * NdotL;

            // Accumulate as if unshadowed.
            Lo += brdfResult * unshadowedRadiance;

            // Per-light ambient fill.
            Lo += lightColor * atten * albedo * 0.08;

            // Stream this light into every reservoir (WRS).
            bool unshadowed = radius < 0.0;
            if (rtEnabled && !unshadowed && shadowFade > 0.01) {
                vec3 shadowableRadiance = brdfResult * unshadowedRadiance;
                // Target pdf: luminance of the to-be-subtracted radiance.
                // Sampling proportional to this approximates the optimal
                // "importance sample by potential contribution".
                float w_i = max(dot(shadowableRadiance, vec3(0.2126, 0.7152, 0.0722)), 1e-6);
                resWSum += w_i;
                // Independent reservoir streams via per-slot noise offset.
                // With probability w_i / resWSum, replace the selection.
                for (uint s = 0; s < NUM_RESERVOIRS; s++) {
                    float u = interleavedGradientNoise(
                        gl_FragCoord.xy + vec2(float(s) * 13.1, float(s) * 27.7),
                        resFrameSeed + float(ci) * 0.37
                    );
                    if (u * resWSum < w_i) {
                        resLight[s] = i;
                        resWSel[s] = w_i;
                        resRadiance[s] = shadowableRadiance;
                    }
                }
            }
        }

        // ── Pass 2: shadow rays for sampled reservoirs ─────────────
        //
        // For each reservoir with a valid selection, cast a shadow ray
        // to the chosen light and subtract its weighted contribution
        // on hit. Weight W = resWSum / (K · w_sel) unbiases the
        // streaming WRS estimator back to Σ radiance_i · (1 − V_i).
        float invK = 1.0 / float(NUM_RESERVOIRS);
        for (uint s = 0; s < NUM_RESERVOIRS; s++) {
            if (resLight[s] == 0xFFFFFFFFu) continue;
            uint i = resLight[s];
            float W = min((resWSum / max(resWSel[s], 1e-6)) * invK, RESERVOIR_W_CLAMP);
            vec3 lightPos = lights[i].position_radius.xyz;
            float radius = lights[i].position_radius.w;
            float lightType = lights[i].color_type.w;

            // Recompute light direction for shadow ray (cheaper than
            // storing L per light in the top-K array).
            vec3 L;
            if (lightType < 1.5) {
                L = normalize(lightPos - fragWorldPos);
            } else {
                L = normalize(lights[i].direction_angle.xyz);
            }

            float frameCount = cameraPos.w;
            float noise1 = interleavedGradientNoise(gl_FragCoord.xy, frameCount + float(i) * 7.0);
            float noise2 = interleavedGradientNoise(gl_FragCoord.xy + vec2(113.5, 247.3), frameCount + float(i) * 13.0);

            vec3 T, B;
            buildOrthoBasis(L, T, B);
            vec2 diskSample = concentricDiskSample(noise1, noise2);

            vec3 rayOrigin = fragWorldPos + N * 0.05;
            vec3 rayDir;
            float rayDist;

            if (lightType < 1.5) {
                // Point / spot: jittered ray toward the light's physical disk.
                float lightDiskRadius = max(radius * 0.025, 1.5);
                vec3 jitteredTarget = lightPos + (T * diskSample.x + B * diskSample.y) * lightDiskRadius;
                rayDir = normalize(jitteredTarget - rayOrigin);
                rayDist = length(jitteredTarget - rayOrigin) - 0.1;
            } else {
                // Directional: small angular cone for penumbra.
                // Real sun subtends ~0.0047 rad (~0.27°) from Earth. The
                // previous 0.05 rad (~2.9°) was ~10× too soft, washing out
                // sharp shadow detail. Keep slight inflation vs physical
                // value to give visible penumbra at interior scale.
                const float sunAngularRadius = 0.0047;
                vec3 jitteredDir = L + (T * diskSample.x + B * diskSample.y) * sunAngularRadius;
                rayDir = normalize(jitteredDir);
                // 100 000 units covers the diagonal of a 7×7 exterior
                // grid (~58K units) with ~40K headroom so distant
                // mountains + cell-edge architecture still occlude
                // the sun. Pre-#102 the 10K cap clipped to one cell,
                // losing shadows from anything ≥2 cells away —
                // visible as "floating" lighting on distant terrain
                // faces and missing cast shadows from opposite-cell
                // architecture. BVH traversal is log-time so the
                // larger tmax is not a meaningful cost.
                rayDist = 100000.0;
            }

            rayQueryEXT rayQuery;
            rayQueryInitializeEXT(
                rayQuery,
                topLevelAS,
                gl_RayFlagsTerminateOnFirstHitEXT | gl_RayFlagsOpaqueEXT,
                0xFF,
                rayOrigin,
                0.001,
                rayDir,
                max(rayDist, 0.01)
            );
            rayQueryProceedEXT(rayQuery);
            if (rayQueryGetIntersectionTypeEXT(rayQuery, true) != gl_RayQueryCommittedIntersectionNoneEXT) {
                // Unbiased shadow subtraction: W compensates for the
                // WRS sampling probability. Clamp to prevent negative
                // radiance from rounding / fill overlap.
                Lo = max(Lo - resRadiance[s] * W * shadowFade, vec3(0.0));
            }
        }
    }

    // ── 1-bounce RT ambient GI ──────────────────────────────────────
    //
    // Cast a single cosine-weighted hemisphere ray per fragment. If it
    // hits geometry, sample the hit surface's texture color and multiply
    // by the ambient light level to approximate indirect illumination.
    // At 60+ FPS, temporal noise integration produces smooth color bleeding.
    vec3 indirect = vec3(0.0);
    // RT ambient occlusion derived from the GI ray's hit distance. Close
    // hits darken the ambient term so recesses/corners/behind-wall regions
    // get real occlusion, not just the screen-space SSAO approximation
    // (which can't see the hall-scale geometry off-screen). 1.0 = fully
    // open, 0.0 = hard against adjacent geometry.
    float rtAO = 1.0;
    if (rtEnabled && !isWindow && !isGlass && emissiveMult < 0.01 && rtLOD < RT_LOD_GI) {
        float giDist = length(fragWorldPos - cameraPos.xyz);
        float giFade = 1.0 - smoothstep(4000.0, 6000.0, giDist);
        if (giFade > 0.01) {
            // Use a slowly-varying noise seed: floor(frameCount/4) makes
            // each noise pattern persist for 4 frames, reducing flicker
            // while still converging over time.
            float frameCount = cameraPos.w;
            float giSeed = floor(frameCount * 0.25);
            float n1 = interleavedGradientNoise(gl_FragCoord.xy, giSeed);
            float n2 = interleavedGradientNoise(gl_FragCoord.xy + vec2(73.7, 191.3), giSeed + 37.0);

            vec3 giDir = cosineWeightedHemisphere(N, n1, n2);
            vec3 giOrigin = fragWorldPos + N * 0.1;

            rayQueryEXT giRQ;
            rayQueryInitializeEXT(
                giRQ, topLevelAS,
                gl_RayFlagsTerminateOnFirstHitEXT | gl_RayFlagsOpaqueEXT, 0xFF,
                giOrigin, 0.5, giDir, 3000.0
            );
            rayQueryProceedEXT(giRQ);

            if (rayQueryGetIntersectionTypeEXT(giRQ, true) != gl_RayQueryCommittedIntersectionNoneEXT) {
                int hitIdx = rayQueryGetIntersectionInstanceCustomIndexEXT(giRQ, true);
                float hitDist = rayQueryGetIntersectionTEXT(giRQ, true);

                // RT AO: near hits = strong occlusion, far hits = open.
                // Range tuned for Bethesda interior scale — a corner/wall
                // within ~120 units (~1.7 m) drives rtAO toward 0.3,
                // surfaces with 500+ unit clearance stay ~1.0.
                rtAO = smoothstep(60.0, 500.0, hitDist);
                rtAO = mix(0.3, 1.0, rtAO);

                // Use pre-computed average albedo from the hit instance's
                // SSBO entry instead of the full UV lookup + texture sample.
                // This reduces 11 divergent memory ops (3 index reads +
                // 6 vertex reads + 1 instance read + 1 texture sample) to
                // a single SSBO read. At 1-SPP with temporal filtering,
                // the texture detail was already noise — the color bleeding
                // effect comes from the average hue, not fine detail.
                GpuInstance hitInst = instances[hitIdx];
                vec3 hitAlbedo = vec3(hitInst.avgAlbedoR, hitInst.avgAlbedoG, hitInst.avgAlbedoB);

                // Ambient bounce: modulates hue from nearby surfaces.
                float hitFade = 1.0 / (1.0 + hitDist * 0.005);
                // Use raw XCLL ambient — no floor. The 0.15 clamp added in
                // commit 14f2e63 was compensation for the since-removed 2.5x
                // XCLL boost. RT bounce from hitAlbedo provides the fill
                // that the floor was artificially preserving. See #268.
                indirect = sceneFlags.yzw * hitAlbedo * hitFade * 0.3;
                // Soft clamp to tame outliers without killing the effect.
                indirect = min(indirect, vec3(0.4));
            } else {
                // Ray escaped — sky fill adds subtle blue to open areas.
                indirect = vec3(0.6, 0.75, 1.0) * 0.06;
            }
            // Smooth distance fade: attenuate GI contribution at range
            // to prevent a visible boundary at the cutoff distance.
            indirect *= giFade;
        }
    }

    // Sample ambient occlusion from the SSAO texture (computed last frame).
    // The floor was raised to 0.45 in commit 14f2e63 to compensate for a
    // 2.5x XCLL ambient boost that has since been removed (now 1.0x in
    // render.rs). With XCLL passed through raw and RT GI providing real
    // bounce light, the aggressive floor prevented AO from visibly biting
    // in crevices. 0.20 keeps a small safety margin for first-frame / edge
    // cases while letting proper contact shadows show through.
    vec2 aoUV = gl_FragCoord.xy / screen.xy;
    float ao = max(texture(aoTexture, aoUV).r, 0.20);

    // Phase 3: albedo-demodulated indirect lighting for SVGF.
    //
    // outColor       = direct lighting (Lo + glass tint) + fog [albedo-modulated]
    // outRawIndirect = indirect LIGHTING ONLY, no local albedo (for SVGF)
    // outAlbedo      = surface albedo (composite re-multiplies)
    //
    // The composite pass reassembles: final = direct + indirect * albedo
    //
    // The ambient and reflection terms above had `* albedo` factored out;
    // the GI bounce at line 769 never had local albedo (it carries the
    // ray-hit surface's color). Multiplying by local albedo at composite
    // re-adds the correct modulation without division-based demodulation,
    // avoiding dark-albedo amplification artifacts. See #268.
    vec3 directLight = Lo;
    // Combine screen-space SSAO with RT AO. SSAO catches fine detail
    // (texture crevices), RT AO catches hall-scale occlusion. Take min
    // so whichever sees the occluder wins.
    float combinedAO = min(ao, rtAO);
    vec3 indirectLight = (ambient + indirect) * combinedAO;

    // Glass compositing: Fresnel controls the output alpha.
    float finalAlpha = texColor.a;
    if (isGlass) {
        finalAlpha = mix(texColor.a, 1.0, glassFresnel * 0.7);
        // Glass tint adds to the direct-light output.
        directLight = directLight + albedo * 0.15;
    }

    // Additive emissive on the direct path (Gamebryo FFP model). Composite
    // does not multiply the direct attachment by albedo, so emissive here
    // is already pre-shaped by albedo (see precompute above) and stays
    // intact through tone mapping.
    directLight = directLight + emissive;

    // Distance fog is applied in the composite pass (#428) after SVGF
    // denoise, so fog attenuation is NOT baked into indirect history —
    // avoiding multi-frame ghosting on fog transitions. `fog` UBO is
    // still read above for the RT ray-miss background color.

    outColor = vec4(directLight, finalAlpha);
    outRawIndirect = vec4(indirectLight, 1.0);
    outAlbedo = vec4(albedo, 1.0);
}
