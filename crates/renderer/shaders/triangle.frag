#version 460
#extension GL_EXT_ray_query : enable
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec3 fragColor;
layout(location = 1) in vec2 fragUV;
layout(location = 2) in vec3 fragNormal;
layout(location = 3) in vec3 fragWorldPos;
layout(location = 4) flat in uint fragTexIndex;

layout(location = 0) out vec4 outColor;

// Bindless texture array: all textures in a single descriptor set.
// The per-instance texture index selects which texture to sample.
layout(set = 0, binding = 0) uniform sampler2D textures[];

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
    mat4 viewProj;    // combined view-projection (used by vertex shader)
    vec4 cameraPos;   // xyz = world position
    vec4 sceneFlags;  // x = RT enabled (1.0), yzw = ambient color (RGB)
};

layout(set = 1, binding = 2) uniform accelerationStructureEXT topLevelAS;

void main() {
    vec4 texColor = texture(textures[nonuniformEXT(fragTexIndex)], fragUV);

    // Alpha test: discard fully transparent fragments (alpha < threshold).
    // This handles glass, fences, foliage, and other alpha-tested geometry.
    if (texColor.a < 0.1) {
        discard;
    }

    vec3 normal = normalize(fragNormal);
    bool rtEnabled = sceneFlags.x > 0.5;

    // Ambient base from cell lighting (or default).
    vec3 totalLight = sceneFlags.yzw;

    if (lightCount == 0) {
        // Fallback: single directional light when no lights are placed.
        vec3 lightDir = normalize(vec3(0.4, 0.8, 0.5));
        float NdotL = max(dot(normal, lightDir), 0.0);
        totalLight = vec3(0.2) + vec3(0.8) * NdotL;
    } else {
        for (uint i = 0; i < lightCount; i++) {
            vec3 lightPos = lights[i].position_radius.xyz;
            float radius = lights[i].position_radius.w;
            vec3 lightColor = lights[i].color_type.rgb;
            float lightType = lights[i].color_type.w;

            vec3 toLight;
            float dist;
            vec3 lightDir;
            float atten;

            if (lightType < 0.5) {
                // Point light — smooth inverse-square falloff with radius cutoff.
                toLight = lightPos - fragWorldPos;
                dist = length(toLight);
                lightDir = toLight / max(dist, 0.001);
                // Windowed inverse-square: physically plausible, smooth cutoff at radius.
                // Based on UE4/Frostbite falloff: 1/(1 + (d/r)^2) * smooth_window
                float ratio = dist / max(radius, 0.001);
                float ratio2 = ratio * ratio;
                float window = max(1.0 - ratio2 * ratio2, 0.0);  // smooth cutoff
                atten = window * window / (1.0 + ratio2 * 4.0);  // inverse-square core
            } else if (lightType < 1.5) {
                // Spot light.
                toLight = lightPos - fragWorldPos;
                dist = length(toLight);
                lightDir = toLight / max(dist, 0.001);
                vec3 spotDir = normalize(lights[i].direction_angle.xyz);
                float spotAngle = lights[i].direction_angle.w;
                float ratio = dist / max(radius, 0.001);
                float ratio2 = ratio * ratio;
                float window = max(1.0 - ratio2 * ratio2, 0.0);
                atten = window * window / (1.0 + ratio2 * 4.0);
                float spotFactor = dot(-lightDir, spotDir);
                atten *= clamp((spotFactor - spotAngle) / (1.0 - spotAngle), 0.0, 1.0);
            } else {
                // Directional light.
                lightDir = normalize(lights[i].direction_angle.xyz);
                dist = 10000.0;
                atten = 1.0;
            }

            float NdotL = max(dot(normal, lightDir), 0.0);

            // Skip lights with zero contribution (back-facing or beyond radius).
            // Avoids expensive ray queries for lights that won't affect the result.
            float contribution = NdotL * atten;
            if (contribution < 0.001) {
                continue;
            }

            // RT shadow ray (when enabled).
            float shadow = 1.0;
            if (rtEnabled) {
                rayQueryEXT rayQuery;
                rayQueryInitializeEXT(
                    rayQuery,
                    topLevelAS,
                    gl_RayFlagsTerminateOnFirstHitEXT | gl_RayFlagsOpaqueEXT,
                    0xFF,
                    fragWorldPos + normal * 0.05,  // bias to avoid self-intersection
                    0.001,                          // tmin
                    lightDir,                       // direction
                    dist - 0.1                      // tmax
                );
                rayQueryProceedEXT(rayQuery);
                if (rayQueryGetIntersectionTypeEXT(rayQuery, true) != gl_RayQueryCommittedIntersectionNoneEXT) {
                    shadow = 0.0;
                }
            }

            totalLight += lightColor * NdotL * atten * shadow;
        }
    }

    outColor = vec4(texColor.rgb * fragColor * totalLight, texColor.a);
}
