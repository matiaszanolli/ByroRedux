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
    vec3 indirect = texture(indirectTex, fragUV).rgb;
    // albedo is written by the main pass but unused in Phase 2 — it'll
    // become load-bearing in Phase 3+ when SVGF demodulation kicks in.
    // vec3 albedo = texture(albedoTex, fragUV).rgb;

    // Reassemble direct + indirect. Fog already applied in the main pass.
    vec3 combined = direct + indirect;

    // Exposure: scale before tone mapping. The ACES curve is designed for
    // values around 0–2 HDR range. Legacy content without energy-conserving
    // BRDF (no 1/PI) can produce values > 2 in brightly lit areas. A
    // moderate exposure brings the working range into ACES's sweet spot.
    const float exposure = 0.7;
    outColor = vec4(aces(combined * exposure), direct4.a);
}
