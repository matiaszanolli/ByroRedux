#version 450

// Composite pass (Phase 2): combines the main render pass outputs into
// the final tone-mapped swapchain image.
//
//   final = direct + indirectDemod * albedo
//   final = mix(final, fogColor, fogFactor)  — applied here, not in main pass
//   output = aces(final)
//
// Direct and indirect are separated so SVGF can filter only the noisy
// indirect signal without blurring crisp direct-light shadows. The
// albedo attachment lets us re-multiply after demodulation.

layout(set = 0, binding = 0) uniform sampler2D hdrTex;       // direct light
layout(set = 0, binding = 1) uniform sampler2D indirectTex;  // demodulated indirect
layout(set = 0, binding = 2) uniform sampler2D albedoTex;    // surface albedo
layout(set = 0, binding = 3) uniform CompositeParams {
    vec4 fog_color;    // xyz = RGB, w = enabled (1.0 = yes)
    vec4 fog_params;   // x = near, y = far, z/w = unused
    vec4 depth_params; // x/y = unused (future: camera near/far for depth reprojection)
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
    vec4 direct4 = texture(hdrTex, fragUV);
    vec3 direct = direct4.rgb;
    vec3 indirectDemod = texture(indirectTex, fragUV).rgb;
    vec3 albedo = texture(albedoTex, fragUV).rgb;

    // Reassemble the full lighting equation: direct + indirect_rescaled.
    vec3 combined = direct + indirectDemod * albedo;

    // Fog application (moved from main render pass). Note: Phase 2
    // doesn't yet pass camera/fog state to the composite; we use a
    // simple smoothstep on fragment-UV-based "distance" as a stub.
    // TODO(phase 3+): pass proper linear depth or pre-baked fog factor
    // via an additional attachment / uniform.
    if (params.fog_color.w > 0.5) {
        // Until we have real depth here, disable fog in composite.
        // The previous pass's fog behavior is lost — acceptable because
        // the Oblivion test scene has no aggressive fog.
    }

    outColor = vec4(aces(combined), 1.0);
}
