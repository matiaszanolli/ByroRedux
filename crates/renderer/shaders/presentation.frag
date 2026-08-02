#version 450

layout(set = 0, binding = 0) uniform sampler2D upscaledScene;

layout(push_constant) uniform PresentationParams {
    vec4 underwater;
    float exposure;
    float padding0;
    float padding1;
    float padding2;
    vec4 lens;
    vec4 radialCurve;
    vec4 grade;
    vec4 radialCenter;
    vec4 tintColor;
    vec4 fadeColor;
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

vec4 sampleImageSpace(vec2 uv) {
    vec2 texel = 1.0 / vec2(textureSize(upscaledScene, 0));
    vec4 scene = texture(upscaledScene, uv);

    float blurRadius = max(params.lens.x, 0.0);
    if (blurRadius > 0.001) {
        vec2 d = texel * blurRadius;
        vec4 blurred = scene * 4.0;
        blurred += texture(upscaledScene, uv + vec2( d.x, 0.0));
        blurred += texture(upscaledScene, uv + vec2(-d.x, 0.0));
        blurred += texture(upscaledScene, uv + vec2(0.0,  d.y));
        blurred += texture(upscaledScene, uv + vec2(0.0, -d.y));
        blurred += texture(upscaledScene, uv + d);
        blurred += texture(upscaledScene, uv - d);
        blurred += texture(upscaledScene, uv + vec2(d.x, -d.y));
        blurred += texture(upscaledScene, uv + vec2(-d.x, d.y));
        scene = blurred / 12.0;
    }

    float doubleVision = abs(params.lens.y);
    if (doubleVision > 0.001) {
        vec2 offset = vec2(doubleVision * 4.0 * texel.x, 0.0);
        vec4 ghost = 0.5 * (
            texture(upscaledScene, uv + offset) +
            texture(upscaledScene, uv - offset)
        );
        scene = mix(scene, ghost, clamp(doubleVision * 0.35, 0.0, 0.75));
    }

    float motionBlur = abs(params.lens.z);
    if (motionBlur > 0.001) {
        vec2 offset = vec2(motionBlur * 6.0 * texel.x, 0.0);
        vec4 streak = (
            texture(upscaledScene, uv - offset) + scene +
            texture(upscaledScene, uv + offset)
        ) / 3.0;
        scene = mix(scene, streak, clamp(motionBlur * 0.25, 0.0, 0.8));
    }

    float radialStrength = params.lens.w;
    if (abs(radialStrength) > 0.001) {
        vec2 radial = uv - params.radialCenter.xy;
        float radius = length(radial);
        float rampUp = max(params.radialCurve.x, 0.001);
        float envelope = smoothstep(
            params.radialCurve.y,
            params.radialCurve.y + rampUp,
            radius
        );
        float downStart = params.radialCurve.w;
        float rampDown = params.radialCurve.z;
        if (rampDown > 0.001 && downStart > params.radialCurve.y) {
            envelope *= 1.0 - smoothstep(downStart, downStart + rampDown, radius);
        }
        float amount = clamp(radialStrength * 0.02 * envelope, -0.25, 0.25);
        vec4 radialSum = scene;
        for (int i = 1; i <= 6; ++i) {
            float stepAmount = amount * (float(i) / 6.0);
            radialSum += texture(upscaledScene, uv - radial * stepAmount);
        }
        scene = radialSum / 7.0;
    }
    return scene;
}

vec3 normalizedLegacyColor(vec3 color) {
    return max(max(color.r, color.g), color.b) > 1.0 ? color / 255.0 : color;
}

void main() {
    vec4 scene = sampleImageSpace(fragUV);
    float luminance = dot(scene.rgb, vec3(0.2126, 0.7152, 0.0722));
    vec3 graded = mix(vec3(luminance), scene.rgb, max(params.grade.x, 0.0));
    graded = (graded - vec3(0.18)) * max(params.grade.z, 0.0) + vec3(0.18);
    graded *= max(params.grade.y, 0.0);
    graded = mix(
        graded,
        graded * normalizedLegacyColor(params.tintColor.rgb),
        clamp(params.tintColor.a, 0.0, 1.0)
    );
    vec3 presented = aces(graded * params.exposure);

    if (params.underwater.w > 0.0) {
        float extinction = clamp(
            1.0 - exp(-params.underwater.w / 120.0),
            0.0,
            0.85
        );
        vec3 underwaterTone = aces(params.underwater.xyz * params.exposure);
        presented = mix(presented, underwaterTone, extinction);
    }

    presented = mix(
        presented,
        normalizedLegacyColor(params.fadeColor.rgb),
        clamp(params.fadeColor.a, 0.0, 1.0)
    );

    outColor = vec4(presented, scene.a);
}
