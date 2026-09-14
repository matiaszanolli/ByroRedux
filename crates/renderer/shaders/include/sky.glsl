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
    vec4 weather_wind;      // x = wind dir x, y = normalized speed, z = wind dir z, w reserved
    vec4 weather_lightning; // lightning RGB, moon glare
    vec4 weather_sky;       // stars RGB, sun glare
    vec4 weather_aurora;    // intensity, follows-sun, procedural coverage, reserved
    // x = is_exterior — the only lane the dome reads, but carried as the
    // full `vec4` its source field is so each builder stays a plain copy.
    vec4 depth_params;
    // xyz = the directional light surfaces receive, w unused. The cloud body
    // is lit by this so clouds and terrain share one sun.
    vec4 sun_illuminance;
};

// The volumetric cloud body. Included here, after `SkyDome`, because every
// function in it takes one; its own guard makes a consumer's second
// `#include` a no-op.
#include "include/clouds.glsl"

float weather_hash21(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
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

// Palette-preserving atmosphere approximation. The WTHR horizon/zenith
// colours remain the climate authority; these terms only supply the angular
// structure a vertical lerp cannot represent: broad Rayleigh sky light and
// forward Mie haze toward the sun. A full atmosphere LUT will eventually
// replace this, but it must not discard the authored Fallout/Skyrim palettes
// merely to get an Earth-default blue sky.
vec3 sky_atmospheric_inscatter(SkyDome dome, vec3 dir, float elevation) {
    float up = clamp(elevation, 0.0, 1.0);
    float daylight = clamp(dome.sun_dir.w * 0.25, 0.0, 1.0);
    float atmosphere_enabled = step(1.0e-5, up) * step(1.0e-5, daylight);

    float cos_theta = clamp(dot(dir, dome.sun_dir.xyz), -1.0, 1.0);
    // Rayleigh's normalized angular shape is proportional to 1 + cos²θ.
    float rayleigh_phase = 0.75 * (1.0 + cos_theta * cos_theta);
    // A normalized Henyey-Greenstein lobe (g = 0.76) captures the broad
    // aerosol-forward haze without competing with the separately rendered
    // sharp solar disc. Multiplying by 4π makes its sphere-average one.
    float g = 0.76;
    float denominator = max(1.0 + g * g - 2.0 * g * cos_theta, 1.0e-4);
    float mie_phase = (1.0 - g * g) / pow(denominator, 1.5);
    float horizon_mass = pow(1.0 - up, 1.35);
    vec3 rayleigh_tint = mix(dome.sky_horizon.xyz, dome.sky_zenith.xyz, 0.72);
    vec3 mie_tint = mix(dome.sky_horizon.xyz, dome.sun_color.xyz, 0.55);
    // Clouds composite their own physical transmittance over this result;
    // weather coverage stays exclusively in include/clouds.glsl so there is
    // one source of truth for cloud occupancy.
    return atmosphere_enabled * daylight * (
        rayleigh_tint * (0.018 * rayleigh_phase * (0.35 + 0.65 * up))
        + mie_tint * (0.012 * mie_phase * horizon_mass)
    );
}

// Compute sky color from view direction.
//
// `cloud_base_noise` / `cloud_detail_noise` are the shared density volumes
// (`CloudNoiseVolumes`); they are parameters rather than bindings because
// the two consumers bind them in different descriptor sets. `cloud_jitter`
// is the cloud march's step offset — see `cloud_march`.
vec3 sky_radiance(
    SkyDome dome,
    vec3 dir,
    sampler3D cloud_base_noise,
    sampler3D cloud_detail_noise,
    float cloud_jitter
) {
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

    sky += sky_atmospheric_inscatter(dome, dir, elevation);

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
    //
    // SKYAL — the body is the volumetric march (`include/clouds.glsl`). It
    // replaced a 2D FBM field that faked self-shadowing with one offset
    // density tap. Same slot, same coverage lane, same tint authority; the
    // march returns premultiplied radiance, hence `sky * (1 - a) + rgb` in
    // place of the old `mix`.
    vec4 volumetric_cloud =
        cloud_march(dome, dir, cloud_base_noise, cloud_detail_noise, cloud_jitter);
    sky = sky * (1.0 - volumetric_cloud.a) + volumetric_cloud.rgb;
    float cloud_occlusion = volumetric_cloud.a;

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
