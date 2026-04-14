#version 450

// Composite pass: combines the main render pass outputs into the final
// tone-mapped swapchain image, with sky rendering for background pixels.
//
//   For geometry pixels (depth < 1.0):
//     final = direct + indirect
//     output = aces(final * exposure)
//
//   For sky pixels (depth == 1.0, exterior only):
//     Reconstruct world-space view direction from screen UV + inv_view_proj.
//     Compute sky gradient (horizon → zenith) + sun disc.
//     output = aces(sky * exposure)
//
// Direct and indirect are separated so SVGF can filter only the noisy
// indirect signal without blurring crisp direct-light shadows. The
// albedo attachment lets us re-multiply after demodulation.

layout(set = 0, binding = 0) uniform sampler2D hdrTex;       // direct light
layout(set = 0, binding = 1) uniform sampler2D indirectTex;  // demodulated indirect
layout(set = 0, binding = 2) uniform sampler2D albedoTex;    // surface albedo (reserved for Phase 3 demodulated-indirect × albedo re-multiplication)
layout(set = 0, binding = 3) uniform CompositeParams {
    vec4 fog_color;      // xyz = RGB, w = enabled (1.0 = yes)
    vec4 fog_params;     // x = near, y = far, z/w = unused
    vec4 depth_params;   // x = is_exterior (1.0 = sky enabled), y/z/w = unused
    vec4 sky_zenith;     // xyz = zenith color (linear RGB), w = sun_size (cos threshold)
    vec4 sky_horizon;    // xyz = horizon color (linear RGB), w = unused
    vec4 sun_dir;        // xyz = sun direction (world-space, normalized), w = sun_intensity
    vec4 sun_color;      // xyz = sun disc color (linear RGB), w = unused
    mat4 inv_view_proj;  // inverse view-projection for ray reconstruction
} params;
layout(set = 0, binding = 4) uniform sampler2D depthTex;     // depth buffer

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

// Reconstruct world-space view direction from screen UV and inverse VP.
vec3 screen_to_world_dir(vec2 uv) {
    // NDC: x [-1,1], y [-1,1], z = 1.0 (far plane in Vulkan reversed-Z? no,
    // we use standard depth where 1.0 = far).
    vec2 ndc = uv * 2.0 - 1.0;
    vec4 clip = vec4(ndc, 1.0, 1.0);
    vec4 world = params.inv_view_proj * clip;
    return normalize(world.xyz / world.w);
}

// Compute sky color from view direction.
vec3 compute_sky(vec3 dir) {
    vec3 zenith = params.sky_zenith.xyz;
    vec3 horizon = params.sky_horizon.xyz;
    float sun_size = params.sky_zenith.w;
    float sun_intensity = params.sun_dir.w;
    vec3 sun_direction = normalize(params.sun_dir.xyz);
    vec3 sun_col = params.sun_color.xyz;

    // Elevation: 0 at horizon, 1 at zenith. Clamp negative (below horizon)
    // to a slightly darkened horizon for a ground-plane approximation.
    float elevation = dir.y;

    // Sky gradient: smooth blend from horizon to zenith.
    // Use a non-linear curve so the horizon band is wider (more natural).
    float t = clamp(elevation, 0.0, 1.0);
    t = sqrt(t); // widen horizon band
    vec3 sky = mix(horizon, zenith, t);

    // Below-horizon darkening: ground approximation (not a ground plane,
    // just a color fade toward a darker horizon).
    if (elevation < 0.0) {
        float below = clamp(-elevation * 3.0, 0.0, 1.0);
        vec3 ground = horizon * 0.3;
        sky = mix(horizon, ground, below);
    }

    // Sun disc: sharp bright spot in the sky.
    float cos_angle = dot(dir, sun_direction);
    if (cos_angle > sun_size) {
        // Smooth edge: lerp from sun_size to 1.0
        float edge = (cos_angle - sun_size) / (1.0 - sun_size);
        edge = smoothstep(0.0, 1.0, edge);
        sky += sun_col * sun_intensity * edge;
    }

    // Sun glow: soft halo around the sun.
    float glow = max(cos_angle, 0.0);
    glow = pow(glow, 8.0);
    sky += sun_col * glow * 0.3;

    return sky;
}

void main() {
    vec4 direct4 = texture(hdrTex, fragUV);
    vec3 direct = direct4.rgb;

    float depth = texture(depthTex, fragUV).r;
    bool is_sky = (depth >= 0.9999) && (params.depth_params.x > 0.5);

    if (is_sky) {
        // Sky pixel: reconstruct view direction and compute sky color.
        vec3 dir = screen_to_world_dir(fragUV);
        vec3 sky = compute_sky(dir);

        const float exposure = 0.85;
        outColor = vec4(aces(sky * exposure), 1.0);
    } else {
        // Geometry pixel: combine direct + indirect and tone map.
        vec3 indirect = texture(indirectTex, fragUV).rgb;
        vec3 combined = direct + indirect;

        const float exposure = 0.85;
        outColor = vec4(aces(combined * exposure), direct4.a);
    }
}
