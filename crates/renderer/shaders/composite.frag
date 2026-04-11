#version 450

// Composite pass: samples the HDR color intermediate, applies ACES tone
// mapping, and writes the result to the sRGB swapchain image. This is the
// last pass of the frame and replaces the previous direct-to-swapchain path.
//
// ACES is the Narkowicz 2015 approximation — a 5-term polynomial fit to
// the full ACES filmic curve. Industry standard for real-time, ~2 MACs.

layout(set = 0, binding = 0) uniform sampler2D hdrTex;

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
    vec3 hdr = texture(hdrTex, fragUV).rgb;
    vec3 mapped = aces(hdr);
    // Swapchain is BGRA8_SRGB → hardware does linear→sRGB conversion on write.
    outColor = vec4(mapped, 1.0);
}
