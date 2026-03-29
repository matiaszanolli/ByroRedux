#version 450

layout(location = 0) in vec3 fragColor;
layout(location = 1) in vec2 fragUV;
layout(location = 2) in vec3 fragNormal;

layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform sampler2D texSampler;

void main() {
    vec4 texColor = texture(texSampler, fragUV);

    // Basic directional light
    vec3 lightDir = normalize(vec3(0.4, 0.8, 0.5));
    vec3 normal = normalize(fragNormal);
    float NdotL = max(dot(normal, lightDir), 0.0);

    // Ambient + diffuse
    float ambient = 0.2;
    float diffuse = 0.8 * NdotL;
    float lighting = ambient + diffuse;

    outColor = vec4(texColor.rgb * fragColor * lighting, texColor.a);
}
