#version 450

layout(set = 0, binding = 0) uniform sampler2D upscaledScene;

layout(push_constant) uniform PresentationParams {
    vec4 underwater;
    float exposure;
} params;

layout(location = 0) in vec2 fragUV;
layout(location = 0) out vec4 outColor;

vec3 aces(vec3 x) {
    const float a = 2.51;
    const float b = 0.03;
    const float c = 2.43;
    const float d = 0.59;
    const float e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0, 1.0);
}

void main() {
    vec4 scene = texture(upscaledScene, fragUV);
    vec3 presented = aces(scene.rgb * params.exposure);

    if (params.underwater.w > 0.0) {
        float extinction = clamp(
            1.0 - exp(-params.underwater.w / 120.0),
            0.0,
            0.85
        );
        vec3 underwaterTone = aces(params.underwater.xyz * params.exposure);
        presented = mix(presented, underwaterTone, extinction);
    }

    outColor = vec4(presented, scene.a);
}
