#version 460
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec3 fragColor;
layout(location = 1) in vec2 fragUV;
layout(location = 2) in vec3 fragNormal;
layout(location = 3) in vec3 fragWorldPos;
layout(location = 4) flat in uint fragTexIndex;
layout(location = 5) flat in int fragInstanceIndex;

layout(location = 0) out vec4 outColor;

// Bindless texture array.
layout(set = 0, binding = 0) uniform sampler2D textures[];

// Per-instance material data from the instance SSBO.
struct GpuInstance {
    mat4 model;
    uint textureIndex;
    uint boneOffset;
    uint normalMapIndex;
    float roughness;
    float metalness;
    float emissiveMult;
    vec3 emissiveColor;
    float specularStrength;
    vec3 specularColor;
    uint _pad;
    uint _pad2[2];
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
    vec4 cameraPos;
    vec4 sceneFlags;  // x = RT enabled (1.0), yzw = ambient color (RGB)
};

layout(set = 1, binding = 2) uniform accelerationStructureEXT topLevelAS;

// ── PBR: GGX / Cook-Torrance BRDF ──────────────────────────────────

const float PI = 3.14159265359;

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

    // Alpha test.
    if (texColor.a < 0.1) {
        discard;
    }

    // Read per-instance material data.
    GpuInstance inst = instances[fragInstanceIndex];
    float roughness = inst.roughness;
    float metalness = inst.metalness;
    float emissiveMult = inst.emissiveMult;
    vec3 emissiveColor = inst.emissiveColor;
    float specStrength = inst.specularStrength;
    vec3 specColor = inst.specularColor;
    uint normalMapIdx = inst.normalMapIndex;

    // Surface normal — perturbed by normal map if available.
    vec3 N = normalize(fragNormal);
    if (normalMapIdx != 0u) {
        N = perturbNormal(N, fragWorldPos, fragUV, normalMapIdx);
    }

    // View direction.
    vec3 V = normalize(cameraPos.xyz - fragWorldPos);
    float NdotV = max(dot(N, V), 0.001);

    bool rtEnabled = sceneFlags.x > 0.5;

    // Base reflectance: dielectrics use 0.04, metals use albedo color.
    vec3 albedo = texColor.rgb * fragColor;
    vec3 F0 = mix(vec3(0.04), albedo, metalness);

    // Emissive bypass: self-lit surfaces skip the light loop entirely.
    if (emissiveMult > 0.01) {
        vec3 emissive = emissiveColor * emissiveMult;
        // Blend: emissive contributes alongside ambient-lit albedo.
        vec3 ambient = sceneFlags.yzw * albedo * (1.0 - metalness);
        outColor = vec4(ambient + emissive, texColor.a);
        return;
    }

    // Ambient base from cell lighting.
    vec3 ambient = sceneFlags.yzw * albedo * (1.0 - metalness);
    vec3 Lo = vec3(0.0); // Accumulated outgoing radiance.

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
        vec3 specular = (D * G * F) / max(4.0 * NdotV * NdotL, 0.001);
        Lo = (kD * albedo / PI + specular * specStrength * specColor) * vec3(0.8) * NdotL;
    } else {
        for (uint i = 0; i < lightCount; i++) {
            vec3 lightPos = lights[i].position_radius.xyz;
            float radius = lights[i].position_radius.w;
            vec3 lightColor = lights[i].color_type.rgb;
            float lightType = lights[i].color_type.w;

            vec3 L;
            float dist;
            float atten;

            if (lightType < 0.5) {
                // Point light.
                vec3 toLight = lightPos - fragWorldPos;
                dist = length(toLight);
                L = toLight / max(dist, 0.001);
                float ratio = dist / max(radius, 0.001);
                float ratio2 = ratio * ratio;
                float window = max(1.0 - ratio2 * ratio2, 0.0);
                atten = window * window / (1.0 + ratio2 * 4.0);
            } else if (lightType < 1.5) {
                // Spot light.
                vec3 toLight = lightPos - fragWorldPos;
                dist = length(toLight);
                L = toLight / max(dist, 0.001);
                vec3 spotDir = normalize(lights[i].direction_angle.xyz);
                float spotAngle = lights[i].direction_angle.w;
                float ratio = dist / max(radius, 0.001);
                float ratio2 = ratio * ratio;
                float window = max(1.0 - ratio2 * ratio2, 0.0);
                atten = window * window / (1.0 + ratio2 * 4.0);
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

            // RT shadow ray.
            float shadow = 1.0;
            if (rtEnabled) {
                rayQueryEXT rayQuery;
                rayQueryInitializeEXT(
                    rayQuery,
                    topLevelAS,
                    gl_RayFlagsTerminateOnFirstHitEXT | gl_RayFlagsOpaqueEXT,
                    0xFF,
                    fragWorldPos + N * 0.05,
                    0.001,
                    L,
                    dist - 0.1
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
            vec3 specular = (D * G * F) / max(4.0 * NdotV * NdotL, 0.001);
            vec3 radiance = lightColor * atten * shadow;

            Lo += (kD * albedo / PI + specular * specStrength * specColor) * radiance * NdotL;
        }
    }

    outColor = vec4(ambient + Lo, texColor.a);
}
