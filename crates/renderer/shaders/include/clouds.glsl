// SKYAL — volumetric cloud layer.
//
// Raymarched through a spherical shell above the viewer, replacing the
// UV-projected cloud planes' inability to have depth, self-shadowing or
// parallax. Marched into the sky cubemap rather than per-pixel or per-ray:
// that is the whole reason the cubemap exists (docs/engine/skyal.md §2.2).
//
// Method follows Schneider & Vos, "The Real-Time Volumetric Cloudscapes of
// Horizon: Zero Dawn", SIGGRAPH 2015 Advances in Real-Time Rendering:
//   * a spherical-shell cloud layer, so clouds bend down to the horizon
//     instead of ending at a flat plane's edge;
//   * low-frequency Perlin-Worley for the base shape, remapped by a
//     coverage signal;
//   * high-frequency Worley erosion applied at the cloud's edges only;
//   * a height-gradient that shapes density vertically;
//   * Beer-Lambert transmittance with the powder (inverse-Beer) term for
//     the dark-edge / silver-lining behaviour Beer alone cannot produce;
//   * Henyey-Greenstein phase for forward scattering toward the sun.
//
// The noise volumes are NOT generated here. They are the two the froxel
// volumetrics pipeline already builds (`volumetrics/noise.rs`), which are
// exactly this pair: FBM Perlin blended with an inverted-Worley billow for
// the base, Worley-dominated for the detail. They arrive as function
// parameters rather than bindings so this file does not dictate a
// descriptor set — the sky-cube bake and the composite background bind
// them in different sets.
//
// Requires `include/sky.glsl` (for `SkyDome`).
#ifndef CLOUDS_GLSL
#define CLOUDS_GLSL

// CLOUD_LAYER_BOTTOM / _TOP / _PLANET_RADIUS / _VIEW_STEPS / _LIGHT_STEPS
// come from the `#include`d `shader_constants.glsl`, generated from
// `src/shader_constants_data.rs` — which is where their values and the
// reasoning behind them are documented. Do not redeclare them here.

// Henyey-Greenstein phase function. `g > 0` is forward-scattering, which is
// what puts the bright rim on a cloud between the viewer and the sun.
float cloud_henyey_greenstein(float cos_angle, float g) {
    float g2 = g * g;
    float denom = 1.0 + g2 - 2.0 * g * cos_angle;
    return (1.0 - g2) / (4.0 * 3.14159265 * pow(max(denom, 1.0e-4), 1.5));
}

// Remap `value` from [`from_min`, `from_max`] onto [`to_min`, `to_max`].
// The reference uses this to subtract the coverage signal out of the base
// noise rather than multiplying by it — multiplying thins clouds
// everywhere, remapping erodes them from the edges inward.
float cloud_remap(float value, float from_min, float from_max, float to_min, float to_max) {
    return to_min
        + (clamp(value, from_min, from_max) - from_min)
            / max(from_max - from_min, 1.0e-5)
            * (to_max - to_min);
}

// Vertical density profile. Dense in the middle of the layer, tapering to
// nothing at both boundaries so clouds have flat-ish bases and rounded
// tops rather than being cut off by the shell.
float cloud_height_gradient(float height_fraction) {
    float bottom = cloud_remap(height_fraction, 0.0, 0.15, 0.0, 1.0);
    float top = cloud_remap(height_fraction, 0.55, 1.0, 1.0, 0.0);
    return clamp(bottom, 0.0, 1.0) * clamp(top, 0.0, 1.0);
}

// Density at a point in the shell.
float cloud_density(
    vec3 position,
    float height_fraction,
    float coverage,
    vec2 wind_offset,
    sampler3D base_noise,
    sampler3D detail_noise
) {
    // Base shape. The volume is tileable, so a plain scaled world position
    // is a valid lookup; wind advects the whole field horizontally.
    vec3 base_uvw = vec3(position.xz * 0.00008 + wind_offset, position.y * 0.00008);
    float base = texture(base_noise, base_uvw).r;

    // Coverage is SUBTRACTED, not multiplied (see `cloud_remap`).
    float shaped = cloud_remap(base, 1.0 - coverage, 1.0, 0.0, 1.0);
    shaped *= cloud_height_gradient(height_fraction);
    if (shaped <= 0.0) {
        return 0.0;
    }

    // Erode the edges with the high-frequency volume. The erosion strength
    // falls off as the base density rises, so it carves wispy boundaries
    // without punching holes through cloud cores.
    vec3 detail_uvw = vec3(position.xz * 0.0009 + wind_offset * 3.0, position.y * 0.0009);
    float detail = texture(detail_noise, detail_uvw).r;
    float erosion = mix(detail, 1.0 - detail, clamp(height_fraction * 5.0, 0.0, 1.0));
    return clamp(cloud_remap(shaped, erosion * 0.45, 1.0, 0.0, 1.0), 0.0, 1.0);
}

// Distance along `dir` from a viewer `height` metres above the planet
// surface to a shell of radius `planet + shell`. Returns a negative value
// when the ray never reaches it.
float cloud_shell_distance(vec3 dir, float height, float shell) {
    float r = CLOUD_PLANET_RADIUS + height;
    float target = CLOUD_PLANET_RADIUS + shell;
    float b = r * dir.y;
    float c = r * r - target * target;
    float disc = b * b - c;
    if (disc < 0.0) {
        return -1.0;
    }
    return -b + sqrt(disc);
}

// March the cloud layer along `dir` and return premultiplied
// (scattered radiance, coverage alpha).
//
// `coverage` is the canonical procedural-cloud coverage EXAL already
// derives per weather (`SkyDome::weather_aurora.z`) — the same signal the
// authored cloud-plane path uses. Nothing here invents a WTHR mapping.
vec4 cloud_march(
    SkyDome dome,
    vec3 dir,
    vec3 sky_behind,
    sampler3D base_noise,
    sampler3D detail_noise
) {
    float coverage = clamp(dome.weather_aurora.z, 0.0, 1.0);
    if (coverage <= 0.001 || dir.y <= 0.0) {
        return vec4(0.0);
    }

    // Viewer at the layer's own reference height (0): the cubemap is a
    // distant environment, so there is no camera parallax to preserve.
    float start = cloud_shell_distance(dir, 0.0, CLOUD_LAYER_BOTTOM);
    float end = cloud_shell_distance(dir, 0.0, CLOUD_LAYER_TOP);
    if (start < 0.0 || end <= start) {
        return vec4(0.0);
    }

    vec3 sun_dir = dome.sun_dir.xyz;
    float sun_intensity = dome.sun_dir.w;
    vec3 sun_color = dome.sun_color.xyz;
    float cos_angle = dot(dir, sun_dir);
    // Dual-lobe: a strong forward lobe for the silver lining plus a weak
    // backward one so clouds away from the sun are not black.
    float phase = max(
        cloud_henyey_greenstein(cos_angle, 0.8),
        cloud_henyey_greenstein(cos_angle, -0.15) * 0.7
    );

    float time = dome.weather_params.w;
    // The host packs `[dir.x, speed, dir.z, 0]` (`build_composite_params`),
    // so direction is `.xz` and speed is `.y` — the same read
    // `weather_procedural_cloud` makes. An earlier `.xy * .z` here folded
    // the speed into the direction and used `dir.z` as the speed, so the
    // layer drifted off-axis and stood still under a pure X wind.
    vec2 wind = dome.weather_wind.xz * dome.weather_wind.y * time * 0.00002;

    float step_size = (end - start) / float(CLOUD_VIEW_STEPS);
    float transmittance = 1.0;
    vec3 scattered = vec3(0.0);

    for (int i = 0; i < CLOUD_VIEW_STEPS; ++i) {
        if (transmittance < 0.01) {
            break;
        }
        float t = start + (float(i) + 0.5) * step_size;
        vec3 position = dir * t;
        float height_fraction = clamp(
            (position.y - CLOUD_LAYER_BOTTOM) / (CLOUD_LAYER_TOP - CLOUD_LAYER_BOTTOM),
            0.0,
            1.0
        );
        float density =
            cloud_density(position, height_fraction, coverage, wind, base_noise, detail_noise);
        if (density <= 0.0) {
            continue;
        }

        // Short march toward the sun for self-shadowing.
        float light_optical_depth = 0.0;
        float light_step = (CLOUD_LAYER_TOP - CLOUD_LAYER_BOTTOM) / float(CLOUD_LIGHT_STEPS);
        for (int j = 0; j < CLOUD_LIGHT_STEPS; ++j) {
            vec3 light_pos = position + sun_dir * (float(j) + 0.5) * light_step;
            float lh = clamp(
                (light_pos.y - CLOUD_LAYER_BOTTOM) / (CLOUD_LAYER_TOP - CLOUD_LAYER_BOTTOM),
                0.0,
                1.0
            );
            light_optical_depth +=
                cloud_density(light_pos, lh, coverage, wind, base_noise, detail_noise) * light_step;
        }

        // Beer-Lambert, plus the powder term. Beer alone makes a cloud's
        // lit edge as dark as its core; powder (1 - exp(-2*d)) restores
        // the darkening-toward-the-edge that real clouds show when lit
        // from behind the viewer.
        float beer = exp(-light_optical_depth * 0.0012);
        float powder = 1.0 - exp(-density * step_size * 0.0024);
        vec3 sun_radiance = sun_color * sun_intensity * beer * powder * phase;

        // Ambient from the sky the cloud sits in front of, so an overcast
        // deck does not go black where the sun cannot reach it.
        vec3 ambient = sky_behind * mix(0.35, 0.85, height_fraction);

        float sample_extinction = density * step_size * 0.0016;
        float sample_transmittance = exp(-sample_extinction);
        // Energy-conserving integration of the slab (Hillaire 2015): the
        // analytic integral of in-scatter over the slab, not `S * dt`,
        // which over-brightens at large step sizes.
        vec3 slab = (sun_radiance + ambient) * (1.0 - sample_transmittance);
        scattered += transmittance * slab;
        transmittance *= sample_transmittance;
    }

    return vec4(scattered, 1.0 - transmittance);
}

#endif // CLOUDS_GLSL
