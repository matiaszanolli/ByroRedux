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

// Main render pass has 6 color attachments (Phase 2).
layout(location = 0) out vec4 outColor;        // HDR color (direct light only)
layout(location = 1) out vec4 outNormal;       // world-space normal (xyz), unused w
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
    uint _pad0;              // offset 140 → total 144
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
    mat4 prevViewProj;  // Phase 1: previous frame's viewProj for motion vectors
    vec4 cameraPos;   // xyz = world position, w = frame counter
    vec4 sceneFlags;  // x = RT enabled (1.0), yzw = ambient color (RGB)
    vec4 screen;      // x = width, y = height, z = fog near, w = fog far
    vec4 fog;         // xyz = fog color (RGB), w = fog enabled (1.0)
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
// Vertex data: position (vec3) + color (vec3) + normal (vec3) + uv (vec2) + bones = 76 bytes/vertex.
// We only need the UV at offset 36 bytes (9 floats into each vertex).
layout(std430, set = 1, binding = 8) readonly buffer GlobalVertices {
    float vertexData[]; // flat array, stride = 19 floats (76 bytes)
};
layout(std430, set = 1, binding = 9) readonly buffer GlobalIndices {
    uint indexData[];
};

// Must match cluster_cull.comp constants.
const uint CLUSTER_TILES_X = 16;
const uint CLUSTER_TILES_Y = 9;
const uint CLUSTER_SLICES_Z = 24;
const float CLUSTER_NEAR = 0.1;
const float CLUSTER_FAR = 10000.0;

const float PI = 3.14159265359;

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

    // Vertex stride: 19 floats (76 bytes). UV starts at float offset 9 (byte 36).
    const uint STRIDE = 19;
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
vec4 traceReflection(vec3 origin, vec3 direction, float maxDist) {
    rayQueryEXT rq;
    rayQueryInitializeEXT(
        rq, topLevelAS,
        gl_RayFlagsOpaqueEXT, 0xFF,
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

    // Sample the hit surface's texture.
    vec3 hitColor = texture(textures[nonuniformEXT(hitTexIdx)], hitUV).rgb;

    // Apply a simple lighting approximation: darken based on the hit distance
    // (distant reflections are dimmer). No full shading on the reflected surface.
    float hitDist = rayQueryGetIntersectionTEXT(rq, true);
    float distFade = 1.0 / (1.0 + hitDist * 0.005);

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
    float logRatio = log(CLUSTER_FAR / CLUSTER_NEAR);
    uint sliceZ = uint(log(max(viewDepth, CLUSTER_NEAR) / CLUSTER_NEAR) / logRatio * float(CLUSTER_SLICES_Z));
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
    vec4 texColor = texture(textures[nonuniformEXT(fragTexIndex)], fragUV);

    // Read per-instance material data (needed by alpha test and lighting).
    GpuInstance inst = instances[fragInstanceIndex];

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
    float roughness = inst.roughness;
    float metalness = inst.metalness;
    float emissiveMult = inst.emissiveMult;
    vec3 emissiveColor = vec3(inst.emissiveR, inst.emissiveG, inst.emissiveB);
    float specStrength = inst.specularStrength;
    vec3 specColor = vec3(inst.specularR, inst.specularG, inst.specularB);
    uint normalMapIdx = inst.normalMapIndex;

    // Surface normal — perturbed by normal map if available.
    vec3 N = normalize(fragNormal);
    if (normalMapIdx != 0u) {
        N = perturbNormal(N, fragWorldPos, fragUV, normalMapIdx);
    }

    // ── G-buffer outputs (Phase 1) ────────────────────────────────────
    // Write these before any early return so SVGF has valid per-pixel
    // normal / motion / mesh_id regardless of which lighting path we take.
    outNormal = vec4(N, 0.0);

    // Screen-space motion vector: current-pixel UV → previous-pixel UV.
    // Perspective divide both clip-space positions to get NDC, halve to
    // go from NDC delta [-2,2] to UV delta [-1,1]. SVGF's temporal pass
    // reads it as: prev_uv = current_uv - motion.
    vec2 currNDC = fragCurrClipPos.xy / fragCurrClipPos.w;
    vec2 prevNDC = fragPrevClipPos.xy / fragPrevClipPos.w;
    outMotion = (currNDC - prevNDC) * 0.5;

    // Mesh ID: instance index + 1 so that "0" (clear value for background
    // pixels) is distinct from "instance 0". Caps at uint16 max.
    outMeshID = uint(fragInstanceIndex) + 1u;

    // View direction. NdotV is clamped to 0.05 (~87°) to prevent the
    // Cook-Torrance `D*G*F / (4*NdotV*NdotL)` specular term from blowing
    // up at grazing view angles — the microfacet model is not valid in
    // that regime anyway, and the unclamped version produced bright
    // triangular specular hotspots along wall surfaces when the camera
    // was looking along them.
    vec3 V = normalize(cameraPos.xyz - fragWorldPos);
    float NdotV = max(dot(N, V), 0.05);

    bool rtEnabled = sceneFlags.x > 0.5;

    // Base reflectance: dielectrics use 0.04, metals use albedo color.
    vec3 albedo = texColor.rgb * fragColor;

    // Dark / multiplicative lightmap (#264): baked shadow modulation.
    if (inst.darkMapIndex != 0u) {
        vec3 darkSample = texture(textures[nonuniformEXT(inst.darkMapIndex)], fragUV).rgb;
        albedo *= darkSample;
    }

    vec3 F0 = mix(vec3(0.04), albedo, metalness);

    // Emissive bypass: self-lit surfaces skip the light loop entirely.
    if (emissiveMult > 0.01) {
        vec3 emissive = emissiveColor * emissiveMult;
        vec3 ambient = sceneFlags.yzw * albedo * (1.0 - metalness);
        outColor = vec4(ambient + emissive, texColor.a);
        outRawIndirect = vec4(0.0);
        outAlbedo = vec4(albedo, 1.0);
        return;
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
    // Detection: alpha-blended fragments (alpha < 0.95) that are NOT
    // glass objects (roughness > 0.1 or metalness > 0). This catches
    // windows but excludes glass vases/bottles.
    bool isWindow = texColor.a < 0.95 && texColor.a > 0.05 && !(roughness <= 0.1 && metalness < 0.1);
    bool isGlass = roughness <= 0.1 && metalness < 0.1 && texColor.a < 0.7 && texColor.a > 0.2;

    if (isWindow && rtEnabled) {
        // Cast a ray through the window in the view direction.
        vec3 throughDir = -V; // continue along the camera's line of sight
        rayQueryEXT windowRQ;
        rayQueryInitializeEXT(
            windowRQ, topLevelAS,
            gl_RayFlagsTerminateOnFirstHitEXT | gl_RayFlagsOpaqueEXT,
            0xFF,
            fragWorldPos - N * 0.5, // start slightly behind the window surface
            0.1,
            throughDir,
            2000.0 // if nothing hit within 2000 units, it's "outside"
        );
        rayQueryProceedEXT(windowRQ);

        bool hitsInterior = (rayQueryGetIntersectionTypeEXT(windowRQ, true) != gl_RayQueryCommittedIntersectionNoneEXT);

        if (!hitsInterior) {
            // Ray escaped the cell — this window sees sky.
            // Output sky light directly: the window is a light portal,
            // not a shaded surface. Skip the entire PBR lighting loop.
            vec3 skyColor = vec3(0.6, 0.75, 1.0); // clear day sky
            // Tint by window texture: stained glass tints the light,
            // clear glass passes it through mostly unchanged.
            // The texture alpha controls how much glass vs sky we see.
            vec3 windowTint = mix(vec3(1.0), texColor.rgb, texColor.a * 0.5);
            vec3 transmitted = skyColor * windowTint * 1.2;
            outColor = vec4(transmitted, 1.0);
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

    // Ambient base from cell lighting.
    vec3 ambient = sceneFlags.yzw * albedo * (1.0 - metalness);
    vec3 Lo = vec3(0.0); // Accumulated outgoing radiance.

    // ── RT reflection for metallic/glossy surfaces ──────────────────
    //
    // Metals reflect their environment. Cast a reflection ray and blend
    // the result based on metalness and Fresnel. The reflected color
    // replaces the ambient term for metals (they have no diffuse).
    if (rtEnabled && metalness > 0.3 && roughness < 0.6) {
        vec3 R = reflect(-V, N);
        vec4 reflResult = traceReflection(fragWorldPos + N * 0.1, R, 5000.0);

        // Fresnel-weighted reflection: stronger at grazing angles.
        vec3 F = fresnelSchlick(NdotV, F0);

        // Roughness blurs the reflection: mix toward ambient for rough metals.
        float reflClarity = 1.0 - roughness;
        vec3 envColor = mix(ambient, reflResult.rgb * albedo, reflClarity * reflResult.a);

        // Metals: reflection replaces ambient entirely.
        // Glossy dielectrics: reflection adds on top of ambient.
        ambient = mix(ambient, envColor, metalness * F);
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
        // Clustered lighting: iterate only lights assigned to this fragment's cluster.
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
                // Point light — windowed 1/d falloff matching Gamebryo's
                // default D3D9 attenuation (C=0, L=1, Q=0 → 1/d).
                // The smooth window function fades to zero at the radius
                // boundary to avoid sharp cutoff artifacts.
                vec3 toLight = lightPos - fragWorldPos;
                dist = length(toLight);
                L = toLight / max(dist, 0.001);
                float ratio = dist / max(radius, 0.001);
                float window = clamp(1.0 - ratio, 0.0, 1.0);
                window *= window; // smooth fade at boundary
                atten = window / max(dist, 1.0);
            } else if (lightType < 1.5) {
                // Spot light — same windowed 1/d attenuation + cone factor.
                vec3 toLight = lightPos - fragWorldPos;
                dist = length(toLight);
                L = toLight / max(dist, 0.001);
                vec3 spotDir = normalize(lights[i].direction_angle.xyz);
                float spotAngle = lights[i].direction_angle.w;
                float ratio = dist / max(radius, 0.001);
                float window = clamp(1.0 - ratio, 0.0, 1.0);
                window *= window;
                atten = window / max(dist, 1.0);
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

            // RT soft shadow ray: stochastic 1-SPP with penumbra.
            //
            // Jitter the ray origin on a disk perpendicular to the light
            // direction, scaled by the light's angular size as seen from the
            // fragment. This produces contact-hardening shadows: close to
            // the occluder the penumbra is tight, far away it's wide.
            //
            // At 200+ FPS the temporal variation from frame-to-frame noise
            // naturally integrates into smooth soft shadows for the human eye.
            //
            // Unshadowed lights (radius < 0, e.g. interior fill directional)
            // skip shadow rays entirely — they exist as ambient fill that
            // should not be blocked by sealed interior walls.
            float shadow = 1.0;
            bool unshadowed = radius < 0.0;
            if (rtEnabled && !unshadowed) {
                // Soft shadow via jittered ray direction. Point/spot lights
                // aim at a disk around their physical position; directional
                // lights aim along -sunDir (i.e., the L vector) with a small
                // angular cone for penumbra. The previous code shared the
                // point-light path for directional lights, which caused the
                // shadow ray to aim at the world origin (lightPos=(0,0,0)
                // for directional) instead of the sun — producing a
                // cone-shaped light leak on walls.

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
                    // Point / spot: trace toward a jittered point on the
                    // light's physical disk. The fixed disk radius produces
                    // correct contact-hardening naturally — nearby fragments
                    // see the disk at a larger angular subtense (soft
                    // penumbra), distant fragments at a smaller one (hard
                    // shadows). See #257.
                    float lightDiskRadius = max(radius * 0.025, 1.5);
                    vec3 jitteredTarget = lightPos + (T * diskSample.x + B * diskSample.y) * lightDiskRadius;
                    rayDir = normalize(jitteredTarget - rayOrigin);
                    rayDist = length(jitteredTarget - rayOrigin) - 0.1;
                } else {
                    // Directional: trace along the direction-to-light (L)
                    // with a small angular cone (~2.8°, matching the sun's
                    // real angular diameter of ~0.5° scaled up for softer
                    // penumbra). No physical light position.
                    const float sunAngularRadius = 0.05; // tan(~2.8°)
                    vec3 jitteredDir = L + (T * diskSample.x + B * diskSample.y) * sunAngularRadius;
                    rayDir = normalize(jitteredDir);
                    rayDist = 10000.0;
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
                    shadow = 0.0;
                }
            }

            // PBR: Cook-Torrance BRDF.
            vec3 H = normalize(V + L);
            float NdotH = max(dot(N, H), 0.0);
            float HdotV = max(dot(H, V), 0.0);

            float D = distributionGGX(NdotH, roughness);
            float G = geometrySmith(NdotV, NdotL, roughness);
            vec3 F = fresnelSchlick(HdotV, F0);

            vec3 kD = (1.0 - F) * (1.0 - metalness);
            vec3 specular = (D * G * F) / max(4.0 * NdotV * NdotL, 0.01);
            vec3 radiance = lightColor * atten * shadow;

            Lo += (kD * albedo / PI + specular * specStrength * specColor) * radiance * NdotL;

            // Per-light ambient fill: approximates Gamebryo's D3D9
            // Material.Ambient * Light.Ambient * dimmer / attenuation term.
            // Each nearby light contributes a small NdotL-independent fill
            // that lifts the base illumination in rooms with many lights.
            Lo += lightColor * atten * shadow * albedo * 0.25;
        }
    }

    // ── 1-bounce RT ambient GI ──────────────────────────────────────
    //
    // Cast a single cosine-weighted hemisphere ray per fragment. If it
    // hits geometry, sample the hit surface's texture color and multiply
    // by the ambient light level to approximate indirect illumination.
    // At 60+ FPS, temporal noise integration produces smooth color bleeding.
    vec3 indirect = vec3(0.0);
    if (rtEnabled && !isWindow && !isGlass && emissiveMult < 0.01) {
        float giDist = length(fragWorldPos - cameraPos.xyz);
        if (giDist < 1500.0) {
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
                gl_RayFlagsOpaqueEXT, 0xFF,
                giOrigin, 0.5, giDir, 500.0
            );
            rayQueryProceedEXT(giRQ);

            if (rayQueryGetIntersectionTypeEXT(giRQ, true) != gl_RayQueryCommittedIntersectionNoneEXT) {
                int hitIdx = rayQueryGetIntersectionInstanceCustomIndexEXT(giRQ, true);
                int hitPrim = rayQueryGetIntersectionPrimitiveIndexEXT(giRQ, true);
                vec2 hitBary = rayQueryGetIntersectionBarycentricsEXT(giRQ, true);
                float hitDist = rayQueryGetIntersectionTEXT(giRQ, true);

                GpuInstance hitInst = instances[hitIdx];
                vec2 hitUV = getHitUV(uint(hitIdx), uint(hitPrim), hitBary);
                vec3 hitAlbedo = texture(textures[nonuniformEXT(hitInst.textureIndex)], hitUV).rgb;

                // Ambient bounce: modulates hue from nearby surfaces.
                // Scale is moderate — the 4-frame noise hold smooths flicker.
                float giFade = 1.0 / (1.0 + hitDist * 0.005);
                indirect = max(sceneFlags.yzw, vec3(0.15)) * hitAlbedo * giFade * 0.3;
                // Soft clamp to tame outliers without killing the effect.
                indirect = min(indirect, vec3(0.4));
            } else {
                // Ray escaped — sky fill adds subtle blue to open areas.
                indirect = vec3(0.6, 0.75, 1.0) * 0.06;
            }
        }
    }

    // Sample ambient occlusion from the SSAO texture (computed last frame).
    // On the first frame before SSAO has run, the texture may read 0 —
    // clamp to a minimum to avoid killing all ambient light.
    vec2 aoUV = gl_FragCoord.xy / screen.xy;
    float ao = max(texture(aoTexture, aoUV).r, 0.45);

    // Phase 2: separate direct from indirect lighting.
    //
    // outColor       = direct lighting (Lo + glass tint) + fog
    // outRawIndirect = raw indirect light ((ambient + indirect) * ao)
    // outAlbedo      = surface albedo (written for future SVGF phases)
    //
    // The composite pass reassembles: final = direct + indirect
    //
    // NOTE: albedo demodulation is DEFERRED to Phase 3 where it pairs with
    // SVGF denoising. In Phase 2 we write raw indirect (not divided by
    // albedo) to avoid precision loss and dark-albedo amplification
    // artifacts that were visible during isolated testing.
    vec3 directLight = Lo;
    vec3 indirectLight = (ambient + indirect) * ao;

    // Glass compositing: Fresnel controls the output alpha.
    float finalAlpha = texColor.a;
    if (isGlass) {
        finalAlpha = mix(texColor.a, 1.0, glassFresnel * 0.7);
        // Glass tint adds to the direct-light output.
        directLight = directLight + albedo * 0.15;
    }

    // Distance fog — applied to direct lighting only. Indirect is assumed
    // to be local enough that fog attenuation is a minor visual artifact.
    // (A more correct approach would be to pass linear depth to the
    //  composite pass and fog the combined signal — deferred to later.)
    if (fog.w > 0.5) {
        float fogFactor = smoothstep(screen.z, screen.w, worldDist);
        directLight = mix(directLight, fog.xyz, fogFactor);
        // Also fade indirect toward zero in fog so distant bounces don't
        // weirdly show through — matches the spatial locality assumption.
        indirectLight *= (1.0 - fogFactor);
    }

    outColor = vec4(directLight, finalAlpha);
    outRawIndirect = vec4(indirectLight, 1.0);
    outAlbedo = vec4(albedo, 1.0);
}
