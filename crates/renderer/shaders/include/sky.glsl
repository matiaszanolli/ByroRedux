// Shared sky dome — the single implementation of the analytic sky.
//
// Extracted from `composite.frag` so the background pass and the sky
// cubemap bake run the SAME code. Before this split there was exactly one
// consumer and the sky read `CompositeParams` directly; a second consumer
// would have had to copy ~400 lines of gradient / cloud / celestial logic
// and then drift from it.
//
// Parameters arrive as an explicit `SkyDome` value rather than through a
// shared uniform block, because the two consumers do not share a
// descriptor set: the composite pass reads them out of its own
// `CompositeParams`, the cubemap bake out of its own UBO. Field names are
// deliberately identical to `CompositeParams`' so each builder is a
// field-for-field copy a reader can check by eye.
//
// Requires `include/shader_constants.glsl`, which every consumer already
// includes.
#ifndef SKY_GLSL
#define SKY_GLSL

// Set 1: bindless texture array from TextureRegistry — shared with the
// main geometry pipeline. Used to sample WTHR cloud textures and the CLMT
// FNAM sun sprite by index. Declared here rather than per-consumer because
// the sky is the only thing in either consumer that reads it.
layout(set = 1, binding = 0) uniform sampler2D textures[];

// Everything the sky dome needs, and nothing else.
//
// One `vec4` per source field, names matching `CompositeParams` 1:1.
struct SkyDome {
    vec4 sky_zenith;        // xyz = zenith colour, w = sun_size (cos threshold)
    vec4 sky_horizon;       // xyz = horizon colour, w = unused by the dome
    vec4 sky_lower;         // xyz = below-horizon ground tint (WTHR SKY_LOWER)
    vec4 sun_dir;           // xyz = normalized world-space sun dir, w = sun_intensity
    vec4 sun_color;         // xyz = disc colour, w = sun-sprite index (floatBitsToUint)
    vec4 cloud_params;      // x=scroll_u y=scroll_v z=tile_scale(0=off) w=tex idx
    vec4 cloud_params_1;
    vec4 cloud_params_2;
    vec4 cloud_params_3;
    vec4 cloud_tint_0;      // PNAM/JNAM tint + alpha
    vec4 cloud_tint_1;
    vec4 cloud_tint_2;
    vec4 cloud_tint_3;
    vec4 weather_params;    // rain, snow, thunder frequency, session seconds
    vec4 weather_wind;      // wind dir x/z, normalized speed, reserved
    vec4 weather_lightning; // lightning RGB, moon glare
    vec4 weather_sky;       // stars RGB, sun glare
    vec4 weather_aurora;    // intensity, follows-sun, procedural coverage, reserved
    // x = is_exterior — the only lane the dome reads, but carried as the
    // full `vec4` its source field is so each builder stays a plain copy.
    vec4 depth_params;
};

float weather_hash21(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

// Smooth value noise for the broad cloud body. Authored WTHR cloud DDS
// layers are composited later as game-specific detail; this field prevents
// gaps between those finite sprites from exposing an implausibly empty sky.
float weather_cloud_noise(vec2 p) {
    vec2 cell = floor(p);
    vec2 local = fract(p);
    vec2 smooth_local = local * local * (3.0 - 2.0 * local);
    float a = weather_hash21(cell);
    float b = weather_hash21(cell + vec2(1.0, 0.0));
    float c = weather_hash21(cell + vec2(0.0, 1.0));
    float d = weather_hash21(cell + vec2(1.0, 1.0));
    return mix(mix(a, b, smooth_local.x), mix(c, d, smooth_local.x), smooth_local.y);
}

float weather_cloud_fbm(vec2 p) {
    // Explicit octaves let glslang fully unroll this hot sky-only path.
    float value = weather_cloud_noise(p) * 0.5333333;
    p = p * 2.03 + vec2(17.13, 9.71);
    value += weather_cloud_noise(p) * 0.2666667;
    p = p * 2.01 + vec2(8.37, 19.19);
    value += weather_cloud_noise(p) * 0.1333333;
    p = p * 2.04 + vec2(13.91, 3.17);
    value += weather_cloud_noise(p) * 0.0666667;
    return value;
}

// Continuous procedural cloud body. The return value is premultiplication-
// ready RGB + opacity. WTHR classification supplies broad coverage, its wind
// advects the field, and its PNAM/JNAM tables tint the result. Using the same
// infinite overhead plane as the authored layers keeps both representations
// locked together during camera motion and weather transitions.
vec4 weather_procedural_cloud(SkyDome dome, vec3 sky, vec3 dir, float elevation, vec3 sun_direction) {
    float coverage = clamp(dome.weather_aurora.z, 0.0, 1.0);
    float horizon_fade = smoothstep(0.015, 0.16, elevation);
    vec2 wind = dome.weather_wind.xz;
    float wind_speed = dome.weather_wind.y;
    float time = dome.weather_params.w;
    vec2 plane = dir.xz / max(elevation, 0.065);
    vec2 drift = wind * time * (0.0012 + wind_speed * 0.0065);

    // Frequencies are chosen in view-plane units rather than texture UVs:
    // the lowest octave must still cross several cells in a near-zenith
    // view, where `dir.xz / dir.y` spans only a small interval.
    vec2 broad_coord = plane * 4.75 + drift;
    float warp = weather_cloud_fbm(plane * 2.10 + drift * 0.37 + vec2(7.1, 19.3));
    broad_coord += vec2(warp - 0.5, 0.5 - warp) * 0.72;
    float broad = weather_cloud_fbm(broad_coord);
    float detail = weather_cloud_fbm(plane * 16.0 - drift * 0.61 + vec2(31.7, 11.3));
    float field = broad * 0.80 + detail * 0.20;
    float threshold = mix(0.62, 0.34, coverage);
    float density = smoothstep(threshold, threshold + 0.10, field);

    vec4 tint = (dome.cloud_tint_0 + dome.cloud_tint_1
        + dome.cloud_tint_2 + dome.cloud_tint_3) * 0.25;
    float day = smoothstep(-0.08, 0.22, sun_direction.y);
    float sun_facing = max(dot(dir, sun_direction), 0.0);
    float edge = smoothstep(0.08, 0.72, 1.0 - density)
        * pow(sun_facing, 10.0) * day;
    // One offset density tap approximates optical depth toward the sun. It
    // gives the body a darker underside and bright windward crown without a
    // full ray march, which is important because this runs for every clear-
    // depth pixel in the composition pass.
    float light_field = weather_cloud_fbm(
        broad_coord + sun_direction.xz * mix(0.18, 0.52, day)
    );
    float self_shadow = clamp((field - light_field) * 2.4 + 0.42, 0.0, 1.0);
    vec3 daylight_cloud = mix(
        vec3(0.43, 0.48, 0.56),
        vec3(0.98, 0.96, 0.91),
        1.0 - self_shadow
    );
    vec3 cloud_lit = mix(sky * 0.34, daylight_cloud, day);
    // WTHR remains the artistic colour authority, but treating its RGB as a
    // pure multiplier can collapse a bright cloud into the sky on records
    // with subdued tints (notably FNV's clear weather). Preserve enough
    // neutral daylight contrast for the body to read, then bias it toward
    // the authored tint.
    cloud_lit *= mix(vec3(1.0), max(tint.rgb, vec3(0.08)), 0.45);
    cloud_lit += dome.sun_color.rgb * edge * dome.sun_dir.w * 0.055;

    float authored_alpha = clamp(tint.a, 0.0, 1.0);
    float opacity = density * horizon_fade
        * mix(0.58, 0.95, coverage) * mix(0.78, 1.0, authored_alpha);
    return vec4(cloud_lit, clamp(opacity, 0.0, 0.96));
}

float weather_star_field(vec3 dir) {
    if (dir.y <= 0.02) {
        return 0.0;
    }
    // A stable spherical-ish hash grid. It is deliberately sparse and
    // high-frequency: stars are points of light, not another cloud texture.
    vec2 projected = dir.xz / max(dir.y, 0.08) * 95.0;
    vec2 cell = floor(projected);
    vec2 local = fract(projected) - 0.5;
    float seed = weather_hash21(cell);
    float point = 1.0 - smoothstep(0.0, 0.035, length(local));
    float rare = smoothstep(0.992, 1.0, seed);
    float bright = mix(0.45, 1.0, weather_hash21(cell + 17.0));
    return rare * point * bright;
}

vec3 weather_sky_details(SkyDome dome, vec3 sky, vec3 dir, float elevation, float cloud_occlusion) {
    if (dome.depth_params.x <= 0.5) {
        return sky;
    }
    // sun_dir.y is positive while the sun is above the horizon and is the
    // canonical day/night signal already used by the TOD system.
    float night = 1.0 - smoothstep(-0.08, 0.20, dome.sun_dir.y);
    float sky_visibility = 1.0 - clamp(cloud_occlusion, 0.0, 1.0);
    float stars = weather_star_field(dir) * night * sky_visibility;
    sky += dome.weather_sky.rgb * stars;

    // At night WTHR's SKY_SUN colour is the authored moon tint in the legacy
    // weather table. Keep the moon direction deterministic when the engine's
    // below-horizon sun sentinel has no horizontal component.
    vec3 moon_direction = vec3(-dome.sun_dir.x, 0.18, -dome.sun_dir.z);
    if (length(moon_direction.xz) < 0.05) {
        moon_direction = vec3(0.46, 0.28, 0.82);
    }
    moon_direction = normalize(moon_direction);
    float moon_cos = dot(dir, moon_direction);
    float moon_edge = 0.9992;
    if (moon_cos > moon_edge && elevation > 0.0) {
        float moon_disc = smoothstep(moon_edge, 1.0, moon_cos);
        sky += dome.sun_color.rgb * moon_disc
            * dome.weather_lightning.w * night * 0.65 * sky_visibility;
    }

    float aurora = dome.weather_aurora.x * night
        * smoothstep(0.12, 0.48, elevation);
    if (aurora > 0.0) {
        float travel = dome.weather_aurora.y > 0.5
            ? dome.weather_params.w * 0.018
            : 0.0;
        float bands = 0.5 + 0.5 * sin(dir.x * 17.0 + dir.z * 5.0 + travel);
        float curtains = 0.5 + 0.5 * sin(dir.z * 31.0 - dir.x * 7.0 + bands * 2.0);
        vec3 aurora_color = mix(vec3(0.08, 0.42, 0.28), vec3(0.18, 0.35, 0.75), curtains);
        sky += aurora_color * aurora * bands * curtains * 0.38 * sky_visibility;
    }

    return sky;
}

// Compute sky color from view direction.
vec3 sky_radiance(SkyDome dome, vec3 dir) {
    vec3 zenith = dome.sky_zenith.xyz;
    vec3 horizon = dome.sky_horizon.xyz;
    float sun_size = dome.sky_zenith.w;
    float sun_intensity = dome.sun_dir.w;
    // Host promises `dome.sun_dir.xyz` is already normalised
    // (per `SkyParams::sun_direction` doc — "normalized, world-space
    // Y-up"). The pre-fix `normalize(...)` per fragment was wasted
    // compute on the composite fullscreen draw. See REN-D10-NEW-07
    // (audit 2026-05-09). If the host contract ever weakens, the
    // dot-product / sun-disc maths below silently degrade — re-add
    // the normalize at that point and fix the SkyParams comment.
    vec3 sun_direction = dome.sun_dir.xyz;
    vec3 sun_col = dome.sun_color.xyz;

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
        sky = mix(horizon, dome.sky_lower.xyz, below);
    }

    // Broad cloud body first; authored WTHR layers below add their original
    // silhouettes and colour variation without being solely responsible for
    // sky occupancy. Accumulate opacity so celestial objects remain behind
    // both representations instead of painting over cloud cover.
    vec4 procedural_cloud = weather_procedural_cloud(dome, sky, dir, elevation, sun_direction);
    sky = mix(sky, procedural_cloud.rgb, procedural_cloud.a);
    float cloud_occlusion = procedural_cloud.a;

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
    // Analytic mip LOD: as elevation → 0 the UV magnitude grows as 1/elevation,
    // so the mip should rise by log2(1/elevation). Factor 0.5 keeps the transition
    // gentler than the raw log2 slope. textureLod bypasses the driver's dFdx/dFdy
    // estimate, which would see a 100x–500x UV discontinuity across horizon-fade
    // quads and snap to mip-0 (per-texel aliasing visible in #730). SH-13.
    float cloud_lod = log2(1.0 / max(elevation, 0.05)) * 0.5;

    float tile_scale = dome.cloud_params.z;
    if (tile_scale > 0.0 && elevation > 0.0) {
        uint cloud_idx = floatBitsToUint(dome.cloud_params.w);
        // max() floor guards against the overhead singularity (dir.y → 0)
        // producing NaN UVs. 0.05 matches ~3° of remaining foreshortening.
        vec2 uv = dir.xz / max(elevation, 0.05) * tile_scale
                + dome.cloud_params.xy;
        vec4 cloud = textureLod(textures[nonuniformEXT(cloud_idx)], uv, cloud_lod);
        // Fade clouds out at the horizon so the projection singularity
        // doesn't produce an ugly stretched band right at elevation=0.
        float horizon_fade = smoothstep(0.0, 0.12, elevation);
        vec4 tint = dome.cloud_tint_0;
        cloud.rgb *= tint.rgb;
        float layer_alpha = cloud.a * tint.a * horizon_fade;
        sky = mix(sky, cloud.rgb, layer_alpha);
        cloud_occlusion = 1.0 - (1.0 - cloud_occlusion) * (1.0 - layer_alpha);
    }

    // Cloud layer 1 (WTHR CNAM — higher-altitude deck, opposite drift direction).
    // tile_scale_1 == 0.0 when no CNAM texture was loaded; the branch is
    // skipped entirely so the bindless array is never sampled with index 0.
    float tile_scale_1 = dome.cloud_params_1.z;
    if (tile_scale_1 > 0.0 && elevation > 0.0) {
        uint cloud_idx_1 = floatBitsToUint(dome.cloud_params_1.w);
        vec2 uv_1 = dir.xz / max(elevation, 0.05) * tile_scale_1
                  + dome.cloud_params_1.xy;
        vec4 cloud_1 = textureLod(textures[nonuniformEXT(cloud_idx_1)], uv_1, cloud_lod);
        float horizon_fade_1 = smoothstep(0.0, 0.12, elevation);
        vec4 tint_1 = dome.cloud_tint_1;
        cloud_1.rgb *= tint_1.rgb;
        float layer_alpha_1 = cloud_1.a * tint_1.a * horizon_fade_1;
        sky = mix(sky, cloud_1.rgb, layer_alpha_1);
        cloud_occlusion = 1.0 - (1.0 - cloud_occlusion) * (1.0 - layer_alpha_1);
    }

    // Cloud layer 2 (WTHR ANAM, M33.1) — same projection / fade as layer 1.
    float tile_scale_2 = dome.cloud_params_2.z;
    if (tile_scale_2 > 0.0 && elevation > 0.0) {
        uint cloud_idx_2 = floatBitsToUint(dome.cloud_params_2.w);
        vec2 uv_2 = dir.xz / max(elevation, 0.05) * tile_scale_2
                  + dome.cloud_params_2.xy;
        vec4 cloud_2 = textureLod(textures[nonuniformEXT(cloud_idx_2)], uv_2, cloud_lod);
        float horizon_fade_2 = smoothstep(0.0, 0.12, elevation);
        vec4 tint_2 = dome.cloud_tint_2;
        cloud_2.rgb *= tint_2.rgb;
        float layer_alpha_2 = cloud_2.a * tint_2.a * horizon_fade_2;
        sky = mix(sky, cloud_2.rgb, layer_alpha_2);
        cloud_occlusion = 1.0 - (1.0 - cloud_occlusion) * (1.0 - layer_alpha_2);
    }

    // Cloud layer 3 (WTHR BNAM, M33.1) — same projection / fade as layer 1.
    float tile_scale_3 = dome.cloud_params_3.z;
    if (tile_scale_3 > 0.0 && elevation > 0.0) {
        uint cloud_idx_3 = floatBitsToUint(dome.cloud_params_3.w);
        vec2 uv_3 = dir.xz / max(elevation, 0.05) * tile_scale_3
                  + dome.cloud_params_3.xy;
        vec4 cloud_3 = textureLod(textures[nonuniformEXT(cloud_idx_3)], uv_3, cloud_lod);
        float horizon_fade_3 = smoothstep(0.0, 0.12, elevation);
        vec4 tint_3 = dome.cloud_tint_3;
        cloud_3.rgb *= tint_3.rgb;
        float layer_alpha_3 = cloud_3.a * tint_3.a * horizon_fade_3;
        sky = mix(sky, cloud_3.rgb, layer_alpha_3);
        cloud_occlusion = 1.0 - (1.0 - cloud_occlusion) * (1.0 - layer_alpha_3);
    }

    // Sun disc: bright circular spot with a soft edge.
    // sun_size is cos(half-angle) of the disc — lower = wider.
    // Use a smooth transition band outside the core to avoid hard edges
    // from screen-space direction reconstruction precision.
    //
    // `elevation > 0.0` matches the cloud-layer gate convention above
    // and stops the disc painting over the below-horizon ground tint
    // at sunset/sunrise (the `mix(horizon, dome.sky_lower.xyz, below)`
    // in `sky_radiance` produces a "ground" colour that the disc would
    // otherwise overwrite). #800.
    float cos_angle = dot(dir, sun_direction);
    float sun_edge_start = sun_size - 0.002; // soft outer fringe
    if (cos_angle > sun_edge_start && elevation > 0.0) {
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
        uint sun_tex_idx = floatBitsToUint(dome.sun_color.w);
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
            // Force mip 0. Default `texture()` uses derivative-based
            // mip selection which goes badly wrong here: the UV is
            // divided by `disc_r` (~0.017 for a ~1° wide sun) so
            // adjacent-pixel UV deltas are ~0.06 even though both
            // pixels sit inside a 100-pixel-wide disc. The sampler
            // then picks mip 4-5 of a 256×256 sprite and the sun
            // renders as ~8 visible texels (~16 pixels each — see
            // the user-reported screenshot, ByroRedux | 1 FPS frame
            // 2026-05-24). The sprite is supposed to look sharp
            // regardless of disc screen-size; mip 0 + bilinear gives
            // ~2.5 source texels per output pixel at typical disc
            // widths, which is supersampling (sharp, no aliasing).
            // Sub-pixel discs at very large render distances would
            // alias on mip 0, but the disc is bright enough that
            // small-disc aliasing is invisible against `sun_intensity`.
            vec4 sprite = textureLod(textures[nonuniformEXT(sun_tex_idx)], uv, 0.0);
            disc_color = sun_col * sprite.rgb;
        }

        float sun_visibility = 1.0 - clamp(cloud_occlusion, 0.0, 1.0) * 0.94;
        sky += disc_color * sun_intensity * dome.weather_sky.w * disc * sun_visibility;
    }

    // Sun glow: soft radial halo around the sun.
    //
    // #799 — multiply by `sun_intensity` so the halo fades with the
    // disc through the day/night ramp. Pre-fix the disc faded
    // correctly (its `sky += disc_color * sun_intensity * disc`
    // already carried the factor) but the halo stayed at constant 0.15 *
    // sun_col, so a WTHR with non-zero `SKY_SUN[NIGHT]` (e.g.
    // Skyrim's MoonShadow) painted a faint warm halo at midnight.
    //
    // Falloff tightened to `pow(., 8)` × 0.10 (was `pow(., 4)` × 0.15)
    // on the Markarth 2026-05-10 probe. The wider 4-power halo at
    // `sun_intensity = 4` was adding +0.28 RGB to the sky ~33° off the
    // sun direction; ACES tonemap then pushed the entire visible
    // upper hemisphere to pale-white in any view including the sun's
    // half of the sky. The `pow(., 8)` curve concentrates the halo
    // to ~15° around the disc — preserves the bright sun region
    // without washing the rest of the sky.
    float glow = max(cos_angle, 0.0);
    glow = pow(glow, 8.0);
    float glow_visibility = 1.0 - clamp(cloud_occlusion, 0.0, 1.0) * 0.82;
    sky += sun_col * glow * 0.10 * sun_intensity * dome.weather_sky.w * glow_visibility;

    return weather_sky_details(dome, sky, dir, elevation, cloud_occlusion);
}

#endif // SKY_GLSL
