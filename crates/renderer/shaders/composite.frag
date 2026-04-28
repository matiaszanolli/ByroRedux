#version 450
#extension GL_EXT_nonuniform_qualifier : require

// Composite pass: combines the main render pass outputs into the final
// tone-mapped swapchain image, with sky rendering for background pixels.
//
//   For geometry pixels (depth < 1.0):
//     final = direct + indirect
//     output = aces(final * exposure)
//
//   For sky pixels (depth == 1.0, exterior only):
//     Reconstruct world-space view direction from screen UV + inv_view_proj.
//     Compute sky gradient (horizon → zenith) + cloud layer + sun disc.
//     output = aces(sky * exposure)
//
// Direct and indirect are separated so SVGF can filter only the noisy
// indirect signal without blurring crisp direct-light shadows. The
// albedo attachment lets us re-multiply after demodulation.

layout(set = 0, binding = 0) uniform sampler2D hdrTex;       // direct light
layout(set = 0, binding = 1) uniform sampler2D indirectTex;  // demodulated indirect
layout(set = 0, binding = 2) uniform sampler2D albedoTex;    // surface albedo (multiplies demodulated indirect)
layout(set = 0, binding = 3) uniform CompositeParams {
    vec4 fog_color;      // xyz = RGB, w = enabled (1.0 = yes)
    vec4 fog_params;     // x = near, y = far, z/w = unused
    vec4 depth_params;   // x = is_exterior (1.0 = sky enabled), y/z/w = unused
    vec4 sky_zenith;     // xyz = zenith color (linear RGB), w = sun_size (cos threshold)
    vec4 sky_horizon;    // xyz = horizon color (linear RGB), w = unused
    vec4 sky_lower;      // xyz = below-horizon ground tint (WTHR SKY_LOWER), w = unused (#541)
    vec4 sun_dir;        // xyz = sun direction (world-space, normalized), w = sun_intensity
    vec4 sun_color;      // xyz = sun disc color (linear RGB), w = CLMT FNAM sun sprite idx (floatBitsToUint; 0 = procedural disc)
    vec4 cloud_params;   // x=scroll_u, y=scroll_v, z=tile_scale (0=disabled), w=texture_idx(uintBits)
    vec4 cloud_params_1; // cloud layer 1 (WTHR CNAM) — same packing as cloud_params
    vec4 cloud_params_2; // cloud layer 2 (WTHR ANAM) — same packing (M33.1)
    vec4 cloud_params_3; // cloud layer 3 (WTHR BNAM) — same packing (M33.1)
    vec4 camera_pos;     // xyz = world position, w = unused. Fog distance origin (#428).
    mat4 inv_view_proj;  // inverse view-projection for ray reconstruction
} params;
layout(set = 0, binding = 4) uniform sampler2D depthTex;     // depth buffer
layout(set = 0, binding = 5) uniform usampler2D causticTex;  // R32_UINT caustic accumulator (#321)

// Set 1: bindless texture array from TextureRegistry — shared with the
// main geometry pipeline. Used here to sample WTHR cloud textures by index.
layout(set = 1, binding = 0) uniform sampler2D textures[];
// CAUSTIC_FIXED_SCALE — divide the uint accumulator by this to
// recover luminance. The compute side (`caustic_splat.comp`) reads
// the same value via the `causticTune.x` UBO uploaded by
// `caustic.rs` every frame, so the splat→accumulator path is
// auto-synced. Composite reads from the storage image directly
// without going through that UBO, so the literal is duplicated
// here. Pinned in lockstep with `caustic.rs::CAUSTIC_FIXED_SCALE`
// by the unit test
// `caustic_fixed_scale_sync_tests::composite_frag_caustic_fixed_scale_matches_rust_const`
// — bumping the Rust const fails the test until this literal is
// updated. See #667 / SH-12.
const float CAUSTIC_FIXED_SCALE = 65536.0;

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

    // Below-horizon darkening: ground approximation (not a ground
    // plane, just a colour fade toward the WTHR-authored ground
    // tint). #541 — pre-fix this branch faked the ground colour as
    // `horizon * 0.3` and dropped the authored `SKY_LOWER` group
    // entirely. The real Sky-Lower colour ships per-TOD on every
    // exterior WTHR, so the night ground is appropriately dark and
    // the sunrise / sunset fringe inherits the warm authored tint
    // without compositor-side tweaking.
    if (elevation < 0.0) {
        float below = clamp(-elevation * 3.0, 0.0, 1.0);
        sky = mix(horizon, params.sky_lower.xyz, below);
    }

    // Cloud layer 0 (from WTHR cloud_textures[0]).
    //
    // Project the upper hemisphere onto an infinite horizontal plane
    // overhead: uv = (dir.xz / dir.y) × tile_scale. This gives perspective-
    // correct foreshortening at low elevations (clouds at the horizon look
    // stretched and tile densely, directly overhead they look large and
    // slow-moving) without needing a real dome mesh.
    //
    // cloud_params.z == 0 disables the sample so the checkerboard fallback
    // handle is never read on cells without WTHR cloud data.
    float tile_scale = params.cloud_params.z;
    if (tile_scale > 0.0 && elevation > 0.0) {
        uint cloud_idx = floatBitsToUint(params.cloud_params.w);
        // max() floor guards against the overhead singularity (dir.y → 0)
        // producing NaN UVs. 0.05 matches ~3° of remaining foreshortening.
        vec2 uv = dir.xz / max(elevation, 0.05) * tile_scale
                + params.cloud_params.xy;
        vec4 cloud = texture(textures[nonuniformEXT(cloud_idx)], uv);
        // Fade clouds out at the horizon so the projection singularity
        // doesn't produce an ugly stretched band right at elevation=0.
        float horizon_fade = smoothstep(0.0, 0.12, elevation);
        sky = mix(sky, cloud.rgb, cloud.a * horizon_fade);
    }

    // Cloud layer 1 (WTHR CNAM — higher-altitude deck, opposite drift direction).
    // tile_scale_1 == 0.0 when no CNAM texture was loaded; the branch is
    // skipped entirely so the bindless array is never sampled with index 0.
    float tile_scale_1 = params.cloud_params_1.z;
    if (tile_scale_1 > 0.0 && elevation > 0.0) {
        uint cloud_idx_1 = floatBitsToUint(params.cloud_params_1.w);
        vec2 uv_1 = dir.xz / max(elevation, 0.05) * tile_scale_1
                  + params.cloud_params_1.xy;
        vec4 cloud_1 = texture(textures[nonuniformEXT(cloud_idx_1)], uv_1);
        float horizon_fade_1 = smoothstep(0.0, 0.12, elevation);
        sky = mix(sky, cloud_1.rgb, cloud_1.a * horizon_fade_1);
    }

    // Cloud layer 2 (WTHR ANAM, M33.1) — same projection / fade as layer 1.
    float tile_scale_2 = params.cloud_params_2.z;
    if (tile_scale_2 > 0.0 && elevation > 0.0) {
        uint cloud_idx_2 = floatBitsToUint(params.cloud_params_2.w);
        vec2 uv_2 = dir.xz / max(elevation, 0.05) * tile_scale_2
                  + params.cloud_params_2.xy;
        vec4 cloud_2 = texture(textures[nonuniformEXT(cloud_idx_2)], uv_2);
        float horizon_fade_2 = smoothstep(0.0, 0.12, elevation);
        sky = mix(sky, cloud_2.rgb, cloud_2.a * horizon_fade_2);
    }

    // Cloud layer 3 (WTHR BNAM, M33.1) — same projection / fade as layer 1.
    float tile_scale_3 = params.cloud_params_3.z;
    if (tile_scale_3 > 0.0 && elevation > 0.0) {
        uint cloud_idx_3 = floatBitsToUint(params.cloud_params_3.w);
        vec2 uv_3 = dir.xz / max(elevation, 0.05) * tile_scale_3
                  + params.cloud_params_3.xy;
        vec4 cloud_3 = texture(textures[nonuniformEXT(cloud_idx_3)], uv_3);
        float horizon_fade_3 = smoothstep(0.0, 0.12, elevation);
        sky = mix(sky, cloud_3.rgb, cloud_3.a * horizon_fade_3);
    }

    // Sun disc: bright circular spot with a soft edge.
    // sun_size is cos(half-angle) of the disc — lower = wider.
    // Use a smooth transition band outside the core to avoid hard edges
    // from screen-space direction reconstruction precision.
    float cos_angle = dot(dir, sun_direction);
    float sun_edge_start = sun_size - 0.002; // soft outer fringe
    if (cos_angle > sun_edge_start) {
        float t = (cos_angle - sun_edge_start) / (1.0 - sun_edge_start);
        t = smoothstep(0.0, 1.0, t);
        // Core is bright, edge fades smoothly.
        float core = smoothstep(sun_size, 1.0, cos_angle);
        float disc = mix(t * 0.5, 1.0, core);

        // #478 — when CLMT FNAM ships a sun sprite (non-zero index),
        // sample it within the disc and multiply by sun_col; otherwise
        // fall back to the flat sun_col (pre-fix behaviour). The UV
        // comes from projecting `dir` onto a tangent plane
        // perpendicular to `sun_direction` and scaling so the texture
        // fills the disc radius.
        uint sun_tex_idx = floatBitsToUint(params.sun_color.w);
        vec3 disc_color = sun_col;
        if (sun_tex_idx != 0u) {
            // Local 2D basis on the plane perpendicular to sun_direction.
            vec3 up_world = abs(sun_direction.y) < 0.99
                ? vec3(0.0, 1.0, 0.0)
                : vec3(0.0, 0.0, 1.0);
            vec3 tangent = normalize(cross(up_world, sun_direction));
            vec3 bitangent = cross(sun_direction, tangent);

            // Disc radius in tangent-plane units: `sqrt(1 - sun_size^2)`
            // matches the angular half-width on the unit sphere. We
            // normalise by this so the sprite fills the disc exactly.
            float disc_r = sqrt(max(0.0, 1.0 - sun_size * sun_size));
            vec2 uv = vec2(dot(dir, tangent), dot(dir, bitangent)) / max(disc_r, 1e-4);
            uv = uv * 0.5 + 0.5;
            vec4 sprite = texture(textures[nonuniformEXT(sun_tex_idx)], uv);
            disc_color = sun_col * sprite.rgb;
        }

        sky += disc_color * sun_intensity * disc;
    }

    // Sun glow: soft radial halo around the sun.
    float glow = max(cos_angle, 0.0);
    glow = pow(glow, 4.0);
    sky += sun_col * glow * 0.15;

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
        // Pass `direct4.a` through (mirroring the geometry branch at
        // line 279) so the alpha-blend marker bit `DEN-6 / #676`
        // preserves through TAA stays consistent across both sky and
        // geometry pixels. Sky pixels by construction don't have a
        // glass surface in front of them today, so today's
        // `direct4.a` on a sky-branch fragment is zero — but a future
        // decal pass / transparent UI / lens-flare feature that asks
        // "is this swapchain pixel sky?" via swapchain alpha would
        // see an asymmetric "1.0 = sky, anything else = geometry"
        // contract that's harder to reason about than the symmetric
        // "alpha is the marker bit, branch on it the same way."
        // DEN-11.
        outColor = vec4(aces(sky * exposure), direct4.a);
    } else {
        // Geometry pixel: combine direct + (indirect × albedo) and tone map.
        // The shader wrote lighting-only indirect (no local albedo) so
        // SVGF operates on a texture-free signal; multiply here to
        // re-apply surface color. See #268.
        vec3 indirect = texture(indirectTex, fragUV).rgb;
        vec3 albedo = texture(albedoTex, fragUV).rgb;

        // Caustic (#321): refracted-light scatter from the caustic_splat
        // pass. Stored as fixed-point luminance in a R32_UINT accumulator;
        // decode here and add a warm-white contribution scaled by the
        // receiver's own albedo so colored surfaces pick up the caustic
        // with their own tint.
        uint causticRaw = texelFetch(causticTex, ivec2(gl_FragCoord.xy), 0).r;
        float causticLum = float(causticRaw) / CAUSTIC_FIXED_SCALE;
        vec3 caustic = albedo * causticLum;

        vec3 combined = direct + indirect * albedo + caustic;

        // Distance fog — applied here rather than in the geometry pass
        // so SVGF history carries un-fogged indirect. Pre-#428 fog was
        // baked into both direct and indirect attachments in
        // triangle.frag, which meant the SVGF history retained the
        // previous frame's fog values after transitions (cell load,
        // weather change) and produced multi-frame ghosting until the
        // α=0.2 accumulation washed it out.
        //
        // Reconstruct world-space fragment position from depth +
        // inv_view_proj, then fog-blend the combined signal by the
        // camera-to-fragment distance.
        //
        // Skip fog on empty/far-plane pixels (depth >= 0.9999). Pre-fix
        // those pixels had no world position and the reconstructed
        // `worldDist` was effectively infinite, saturating the smoothstep
        // to 1.0 and flooding every gap in interior geometry with
        // fog_color — visible as a bright blue-tinted "doorway" through
        // any wall seam on interior cells. The sky branch above
        // already handles exterior depth-far pixels; interior empty
        // pixels just pass through the clear color untouched.
        if (depth < 0.9999
            && params.fog_color.w > 0.5
            && params.fog_params.y > params.fog_params.x)
        {
            vec2 ndc_xy = fragUV * 2.0 - 1.0;
            vec4 clip = vec4(ndc_xy, depth, 1.0);
            vec4 world = params.inv_view_proj * clip;
            vec3 worldPos = world.xyz / world.w;
            float worldDist = length(worldPos - params.camera_pos.xyz);

            // Cap at 70% opacity — same rationale as the pre-#428
            // triangle.frag code: D3D9 rendered fog in sRGB where the
            // blend appeared softer; linear-space fog is perceptually
            // stronger, so the cap keeps distant geometry readable.
            float fogFactor = smoothstep(params.fog_params.x, params.fog_params.y, worldDist) * 0.7;
            combined = mix(combined, params.fog_color.xyz, fogFactor);
        }

        const float exposure = 0.85;
        outColor = vec4(aces(combined * exposure), direct4.a);
    }
}
