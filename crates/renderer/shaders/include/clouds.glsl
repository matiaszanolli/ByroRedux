// SKYAL — volumetric cloud layer.
//
// Raymarched through a spherical shell above the viewer. It is the cloud
// body `sky_radiance` paints, replacing a 2D FBM approximation that could
// fake neither depth, self-shadowing nor parallax. It runs in both sky
// consumers: once per cube texel in the bake, and once per clear-depth pixel
// in the composite background. Ray-traced rays never march it; they sample
// the baked cube (docs/engine/skyal.md §2.2).
//
// Method follows Schneider & Vos, "The Real-Time Volumetric Cloudscapes of
// Horizon: Zero Dawn", SIGGRAPH 2015 Advances in Real-Time Rendering:
//   * a spherical-shell cloud layer, so clouds bend down to the horizon
//     instead of ending at a flat plane's edge;
//   * low-frequency Perlin-Worley for the base shape, remapped by a
//     coverage signal;
//   * high-frequency Worley erosion applied at the cloud's edges only;
//   * a height-gradient that shapes density vertically;
//   * Beer-Lambert transmittance.
//
// Lighting follows Hillaire 2016, "Physically Based Sky, Atmosphere and Cloud
// Rendering in Frostbite" (SIGGRAPH 2016 PBS course notes §5.5-5.8):
//   * energy-conserving analytic slab integration (§5.6.3);
//   * a two-lobe Henyey-Greenstein phase (§5.7);
//   * ambient from the sky over both hemispheres (§5.5.1);
//   * multiple scattering as summed single-scattering octaves (§5.8), after
//     Wrenninge et al. 2013;
//   * albedo ~= 1 ("cloud albedo is very close to 1", §5.8).
// Schneider's powder term is not used: its view-dependent gradient is left
// unspecified in the source, and mixing it into this model is part of how an
// earlier version rendered clouds darker than the sky.
//
// Units. The sun term is the directional light surfaces receive
// (`SkyDome::sun_illuminance`), in the engine's surface-lighting units, where
// a white diffuse surface facing the sun has radiance E (the diffuse BRDF's
// 1/PI is compensated in `lighting.glsl`). Hillaire notes that a very dense
// cloud "should converge to what an opaque diffuse surface would look like".
// A dense albedo-1 sample's source is K * E * p * sum(a^n); any normalised
// phase averages to 1/(4 PI) over the sphere, so matching the white surface
// on average fixes K = 4 PI / sum(a^n). The phase keeps its angular shape, so
// the forward silver lining exceeds E and the sun-away side falls below it.
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

// Two-lobe cloud phase function, Hillaire 2016 ("Physically Based Sky,
// Atmosphere and Cloud Rendering in Frostbite", SIGGRAPH 2016 PBS course,
// slides 34-38): a forward and a backward Henyey-Greenstein lobe mixed by a
// blend weight. Each lobe is normalised, so the convex mix is too — which is
// the energy-conservation property the notes ask of the combination.
//
// `eccentricity_scale` multiplies both lobes' g. It is 1 for single
// scattering; the multiple-scattering octaves pass a shrinking scale so
// later bounces scatter more uniformly (Wrenninge's approximation, which
// Hillaire uses for clouds).
float cloud_phase(float cos_angle, float eccentricity_scale) {
    return mix(
        cloud_henyey_greenstein(cos_angle, CLOUD_PHASE_G0 * eccentricity_scale),
        cloud_henyey_greenstein(cos_angle, CLOUD_PHASE_G1 * eccentricity_scale),
        CLOUD_PHASE_BLEND
    );
}

// Mean radiance of the analytic sky dome over the whole sphere, without the
// sun disk — the cloud ambient term. Hillaire 2016 (notes, slide 60) computes
// cloud ambient as the sky luminance integrated over uniform sphere
// directions with the sun disk excluded, scattered with a uniform 1/4PI
// phase, so the in-scattered ambient radiance is exactly that mean.
//
// Frostbite estimates the integral from a 64x64 buffer. This engine's sky
// gradient is closed-form, so the integral is taken exactly instead. Under
// uniform sphere measure `dir.y` is uniform on [-1, 1] (Archimedes), so each
// hemisphere contributes half:
//   upper: sky = mix(horizon, zenith, sqrt(y)),  y in [0, 1]
//          mean = horizon + (zenith - horizon) * integral(sqrt(y)) = + 2/3
//   lower: sky = mix(horizon, lower, min(3u, 1)), u = -y in [0, 1]
//          mean = horizon + (lower - horizon) * integral(min(3u, 1)) = + 5/6
// Must track `sky_radiance`'s gradient: if that shape changes, these two
// fractions change with it.
vec3 cloud_ambient_radiance(SkyDome dome) {
    vec3 zenith = dome.sky_zenith.xyz;
    vec3 horizon = dome.sky_horizon.xyz;
    vec3 lower = dome.sky_lower.xyz;
    vec3 upper_mean = horizon + (zenith - horizon) * (2.0 / 3.0);
    vec3 lower_mean = horizon + (lower - horizon) * (5.0 / 6.0);
    return 0.5 * (upper_mean + lower_mean);
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
// `coverage` is `SkyDome::weather_aurora.z`, which EXAL derives from the WTHR
// classification flags in `env_translate::fog_coverage_from_weather`. This
// file adds no mapping of its own — but that one is uncited (a fixed
// 0.86 / 0.80 / 0.70 / 0.40 / 0.55 per flag), see docs/engine/skyal.md.
//
// `jitter` in [0, 1) offsets every view sample within its step. The bake
// passes 0.5 (step-centred, deterministic): the cube is re-baked each frame
// and reflections have no temporal filter of their own, so a varying offset
// would flicker there. The composite background passes a per-pixel,
// per-frame blue-noise rank, which TAA / FSR integrate, so 48 steps do not
// band.
vec4 cloud_march(
    SkyDome dome,
    vec3 dir,
    sampler3D base_noise,
    sampler3D detail_noise,
    float jitter
) {
    float coverage = clamp(dome.weather_aurora.z, 0.0, 1.0);
    // The horizon fade the 2D body this march replaced used, carried over
    // verbatim rather than re-tuned. Near the horizon the shell is ~100 km
    // away and one pixel spans kilometres of it, so the march aliases into
    // noise; the fade is what kept that off-screen before, and still does.
    float horizon_fade = smoothstep(0.015, 0.16, dir.y);
    if (coverage <= 0.001 || horizon_fade <= 0.0) {
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
    vec3 sun_illuminance = dome.sun_illuminance.xyz;
    float cos_angle = dot(dir, sun_dir);

    // Diffuse-surface calibration K = 4 PI / sum_{n<N} a^n — see the header.
    float octave_scattering_sum = 0.0;
    {
        float scale = 1.0;
        for (int n = 0; n < int(CLOUD_MS_OCTAVES); ++n) {
            octave_scattering_sum += scale;
            scale *= CLOUD_MS_SCATTERING_FALLOFF;
        }
    }
    float diffuse_calibration = 12.566370614359172 / octave_scattering_sum;
    // Constant along the ray, so evaluated once rather than per step.
    vec3 ambient = cloud_ambient_radiance(dome);

    float time = dome.weather_params.w;
    // The host packs `[dir.x, speed, dir.z, 0]` (`build_composite_params`),
    // so direction is `.xz` and speed is `.y`. An earlier `.xy * .z` here folded
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
        float t = start + (float(i) + jitter) * step_size;
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

        // `light_optical_depth` is still the density integral toward the sun;
        // scale it to extinction once, here.
        light_optical_depth *= CLOUD_EXTINCTION_PER_METER;

        // Multiple scattering: N single-scattering octaves, octave n using
        // scattering * a^n, extinction * b^n and phase eccentricity * c^n
        // (Hillaire 2016 Eq. 19-20). Later octaves reach further into the
        // shadowed interior and scatter more uniformly, which is what lets
        // light "punch through the medium in order to reveal inner details
        // on the shadowed sides" (§5.8).
        float octave_sum = 0.0;
        float scattering_scale = 1.0;
        float extinction_scale = 1.0;
        float eccentricity_scale = 1.0;
        for (int n = 0; n < int(CLOUD_MS_OCTAVES); ++n) {
            octave_sum += scattering_scale
                * exp(-light_optical_depth * extinction_scale)
                * cloud_phase(cos_angle, eccentricity_scale);
            scattering_scale *= CLOUD_MS_SCATTERING_FALLOFF;
            extinction_scale *= CLOUD_MS_EXTINCTION_FALLOFF;
            eccentricity_scale *= CLOUD_MS_ECCENTRICITY_FALLOFF;
        }
        vec3 sun_radiance = sun_illuminance * diffuse_calibration * octave_sum;

        float sample_extinction = density * CLOUD_EXTINCTION_PER_METER * step_size;
        float sample_transmittance = exp(-sample_extinction);
        // Energy-conserving integration of the slab (Hillaire 2015): the
        // analytic integral of in-scatter over the slab, not `S * dt`,
        // which over-brightens at large step sizes.
        vec3 slab = (sun_radiance + ambient) * (1.0 - sample_transmittance);
        scattered += transmittance * slab;
        transmittance *= sample_transmittance;
    }

    // WTHR stays the artistic colour authority, exactly as it was for the 2D
    // body this replaced: bias the lit colour toward the mean authored layer
    // tint (floored so a subdued tint cannot collapse the cloud into the
    // sky), and scale opacity by the authored layer alpha. Same factors, so
    // per-weather cloud colour does not regress with the swap.
    vec4 tint = (dome.cloud_tint_0 + dome.cloud_tint_1
        + dome.cloud_tint_2 + dome.cloud_tint_3) * 0.25;
    float authored_alpha = clamp(tint.a, 0.0, 1.0);
    vec3 tinted = scattered * mix(vec3(1.0), max(tint.rgb, vec3(0.08)), 0.45);

    // Premultiplied throughout, so the colour must be scaled by exactly
    // what the alpha is scaled by — including the 0.96 cap the 2D body used
    // to keep a trace of sky visible through the densest deck.
    float raw_alpha = (1.0 - transmittance) * horizon_fade * mix(0.78, 1.0, authored_alpha);
    float alpha = min(raw_alpha, 0.96);
    float colour_scale = raw_alpha > 1.0e-5
        ? alpha / (1.0 - transmittance)
        : 0.0;
    return vec4(tinted * colour_scale, alpha);
}

#endif // CLOUDS_GLSL
