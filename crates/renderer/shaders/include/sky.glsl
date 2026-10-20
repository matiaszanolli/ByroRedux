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
    vec4 weather_aurora;    // intensity, follows-sun, procedural coverage, column-coverage threshold
    // x = is_exterior — the only lane the dome reads, but carried as the
    // full `vec4` its source field is so each builder stays a plain copy.
    vec4 depth_params;
    // xyz = the directional light above the clouds, w unused. The cloud body
    // is lit by this; surfaces receive it through the cloud-shadow map.
    vec4 sun_illuminance;
    // xyz = the camera's world position in metres (renderer Y-up), w unused.
    // The cloud field and the upper plane are anchored to the world through
    // it, so they parallax as the viewer moves and the shadows they cast
    // (`cloud_shadow.comp`) stay where they fall.
    vec4 cloud_origin;
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

// Stars, moon and aurora. They sit behind every cloud layer, so they are
// added to the clear sky and the cloud composites in `sky_radiance` occlude
// them by their own transmittance.
vec3 weather_sky_details(SkyDome dome, vec3 sky, vec3 dir, float elevation) {
    // #4908 — gate on "an outdoor palette is drawn", not on the room's
    // weather flag: an interior composite keeps `depth_params.x = 0` so rain
    // and height fog stay off, but paints the outdoor sky through Show Sky
    // (`sky_lower.w == 1`) and bounded apertures (`== 2`). Gating on
    // `depth_params.x` alone left those skies starless and moonless while the
    // same room's window-portal cube (packed `depth_params.x = 1`) had both.
    if (dome.depth_params.x <= 0.5 && dome.sky_lower.w <= 0.5) {
        return sky;
    }
    // sun_dir.y is positive while the sun is above the horizon and is the
    // canonical day/night signal already used by the TOD system.
    float night = 1.0 - smoothstep(-0.08, 0.20, dome.sun_dir.y);
    float stars = weather_star_field(dir) * night;
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
            * dome.weather_lightning.w * night * 0.65;
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
        sky += aurora_color * aurora * bands * curtains * 0.38;
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

// #4230 — the mip for one WTHR cloud layer, from the shared elevation term.
//
// A texture LOD is log2 of the footprint in the texture's own mip-0 texels,
// and for a cloud layer that footprint scales with `tile_scale * width`, not
// with `tile_scale` alone: the host derives `tile_scale` as
// `baseline * CLOUD_REF_WIDTH / width` precisely so a 1024^2 sprite tiles half
// as often as a 512^2 one. Offsetting by raw `tile_scale` would therefore
// sample a high-resolution sprite one mip *sharper* than a reference one with
// the same on-screen density — trading the aliasing this fixes for another.
//
// The offset is a whole log2, not halved like the elevation term. That 0.5 is
// a softening of an approximate geometric slope (#730); this is an exact
// frequency ratio at every elevation, and a layer twice as fine needs exactly
// one more mip to be filtered like layer 0.
//
// Relative to layer 0's baseline at the reference width, so layer 0 — the
// layer `base_lod` was tuned on — resolves to an offset of exactly zero at
// any sprite resolution, and keeps its look.
float cloud_layer_lod(float base_lod, float tile_scale, uint texture_index) {
    float width = float(textureSize(textures[nonuniformEXT(texture_index)], 0).x);
    return base_lod
        + log2(tile_scale * width / (CLOUD_TILE_SCALE_LAYER_0 * CLOUD_REF_WIDTH));
}

// ── Aerial perspective ─────────────────────────────────────────────────────
//
// The air between the viewer and a cloud layer both dims the cloud and adds
// in-scattered sky light in front of it. Extinction comes from the Earth
// atmosphere (`ATMOSPHERE_*`, Hillaire 2020 Table 1); the in-scattered colour
// stays the authored WTHR sky's, so the games' palettes are kept.

// Scaled complementary error function erfcx(x) = exp(x^2) erfc(x), x >= 0.
// Below 2: Abramowitz & Stegun 7.1.26, whose erfc is a polynomial times
// exp(-x^2), so the polynomial alone is erfcx. Above: their continued
// fraction 7.1.14 to six levels, where 7.1.26's absolute error would be
// amplified by exp(x^2). Relative error <= 5e-5 over [0, 50] against the
// library erfc (`chapman_column_matches_numerical_integration`).
float sky_erfcx(float x) {
    if (x < 2.0) {
        float t = 1.0 / (1.0 + 0.3275911 * x);
        return t * (0.254829592 + t * (-0.284496736 + t * (1.421413741
            + t * (-1.453152027 + t * 1.061405429))));
    }
    float f = x + 3.0 / x;
    f = x + 2.5 / f;
    f = x + 2.0 / f;
    f = x + 1.5 / f;
    f = x + 1.0 / f;
    f = x + 0.5 / f;
    return 0.5641895835477563 / f; // 1 / (sqrt(pi) * f)
}

// Sea-level-equivalent path length (metres) through one exponential
// constituent of scale height `scale_height`, along a ray from the ground at
// zenith cosine `mu >= 0`, out to `distance` (infinity when negative).
//
// Over a curved ground the ray's altitude is mu s + s^2 / 2R to second order,
// so the column integral of exp(-z / H) has the closed form
//   sqrt(pi R H / 2) * [erfcx(x0) - exp(x0^2 - x1^2) erfcx(x1)],
//   x0 = mu sqrt(R / 2H),  x1 = x0 + s / sqrt(2 R H)
// — the Chapman integral, exact in both limits (H / mu overhead, the grazing
// sqrt(pi R H / 2) at the horizon). Written with erfcx so neither exponential
// can overflow. Within 0.02% of a numerical integral through the true sphere
// at every elevation and cloud distance this sky uses.
float sky_air_column(float mu, float distance, float scale_height) {
    float R = CLOUD_PLANET_RADIUS;
    float x0 = max(mu, 0.0) * sqrt(R / (2.0 * scale_height));
    float column = sky_erfcx(x0);
    if (distance >= 0.0) {
        float x1 = x0 + distance / sqrt(2.0 * R * scale_height);
        column -= exp((x0 - x1) * (x0 + x1)) * sky_erfcx(x1);
    }
    return sqrt(1.5707963267948966 * R * scale_height) * max(column, 0.0);
}

// Optical depth of clear air along a ray of zenith cosine `mu` from the
// viewer to `distance` (to space when negative).
vec3 sky_air_optical_depth(float mu, float distance) {
    return ATMOSPHERE_RAYLEIGH_EXTINCTION
            * sky_air_column(mu, distance, ATMOSPHERE_RAYLEIGH_SCALE_HEIGHT)
        + vec3(ATMOSPHERE_MIE_EXTINCTION
            * sky_air_column(mu, distance, ATMOSPHERE_MIE_SCALE_HEIGHT));
}

// Air between the viewer and a point `distance` along a ray of zenith cosine
// `mu`: its transmittance, and the share of the ray's in-scattered sky light
// that originates in front of the point.
//
// The sky colour is the in-scatter of the WHOLE ray to space. Under single
// scattering with a uniform source, the share scattered in front of the point
// is the share of the ray's extinction that lies in front of it:
// (1 - T(d)) / (1 - T(inf)).
//
// Chromatically NEUTRAL: the extinction is the atmosphere's, but averaged over
// the three channels. The in-scattered colour is the game's authored sky, which
// already carries its own temperature and mood; a per-channel Rayleigh share
// (blue scatters ~6x red) would tilt every world's distant clouds toward blue —
// the Pitt's sepia deck turned lavender at low elevations — by re-colouring a
// palette that was authored, not simulated.
void sky_air_terms(float mu, float distance, out vec3 transmittance, out vec3 in_front) {
    float depth = dot(sky_air_optical_depth(mu, distance), vec3(1.0 / 3.0));
    float total = dot(sky_air_optical_depth(mu, -1.0), vec3(1.0 / 3.0));
    float neutral_transmittance = exp(-depth);
    transmittance = vec3(neutral_transmittance);
    in_front = vec3(clamp(
        (1.0 - neutral_transmittance) / max(1.0 - exp(-total), 1.0e-5), 0.0, 1.0));
}

// Fade a premultiplied cloud layer at `distance` along a view ray of zenith
// cosine `mu` into the clear sky `haze` in front of it: dimmed by the air's
// transmittance, plus the sky light scattered in front of it.
vec4 sky_aerial_perspective(vec4 layer, vec3 haze, float mu, float distance) {
    vec3 transmittance;
    vec3 in_front;
    sky_air_terms(mu, distance, transmittance, in_front);
    return vec4(layer.rgb * transmittance + haze * layer.a * in_front, layer.a);
}

// ── Upper cloud plane ──────────────────────────────────────────────────────
//
// The authored WTHR cloud textures, drawn on a spherical shell at
// CLOUD_UPPER_PLANE_ALTITUDE (WMO high étage) behind the volumetric deck.
// They used to be painted on an infinite flat plane in FRONT of the deck,
// which put the games' high cirrus layers nearer than 1.5 km cumulus and
// needed a 0.05 elevation floor plus a 0.12 horizon fade to hide the plane's
// projection singularity. The shell has none: toward the horizon its UVs
// converge to sqrt(2 R h / h^2) ~ 38 tile units instead of diverging, and the
// aerial perspective fades it into the horizon haze.
//
// UV convention: the horizontal world position of the shell point, in units
// of the shell altitude. Relative to the viewer that is `dir.xz / dir.y` at
// the zenith, so `tile_scale` keeps its authored meaning (#529) and overhead
// clouds keep their size.

// One authored layer, premultiplied. `tile_scale == 0` is the host's
// "no texture" sentinel, so the bindless array is never sampled at index 0.
vec4 sky_upper_cloud_layer(vec4 params, vec4 tint, vec2 shell_uv, float base_lod) {
    float tile_scale = params.z;
    if (tile_scale <= 0.0) {
        return vec4(0.0);
    }
    uint texture_index = floatBitsToUint(params.w);
    vec2 uv = shell_uv * tile_scale + params.xy;
    // `textureLod` with an analytic LOD: the driver's derivative estimate
    // sees a huge UV discontinuity across the horizon quads and snaps to
    // mip 0 (per-texel aliasing, #730 / SH-13).
    vec4 texel = textureLod(textures[nonuniformEXT(texture_index)], uv,
        cloud_layer_lod(base_lod, tile_scale, texture_index));
    float alpha = texel.a * tint.a;
    return vec4(texel.rgb * tint.rgb * alpha, alpha);
}

// The four authored layers over one another (layer 0 lowest, layer 3 drawn
// last, the order the 2D path used), faded into `haze` toward the horizon.
// Premultiplied (radiance, opacity).
vec4 sky_upper_cloud_plane(SkyDome dome, vec3 dir, vec3 haze) {
    if (dir.y <= 0.0) {
        return vec4(0.0);
    }
    float distance = cloud_shell_distance(dir, 0.0, CLOUD_UPPER_PLANE_ALTITUDE);
    if (distance <= 0.0) {
        return vec4(0.0);
    }
    float stretch = distance / CLOUD_UPPER_PLANE_ALTITUDE;
    // World-anchored like the deck: the shell point's horizontal world
    // position, in units of the shell altitude.
    vec2 shell_uv = dir.xz * stretch + dome.cloud_origin.xz / CLOUD_UPPER_PLANE_ALTITUDE;
    // Analytic mip: the UV footprint grows with the stretch toward the
    // horizon; 0.5 keeps the softened slope the flat plane was tuned with
    // (`log2(1 / elevation) * 0.5`, which `stretch` equals above ~3 degrees).
    float base_lod = log2(max(stretch, 1.0)) * 0.5;

    vec4 plane = vec4(0.0);
    vec4 layers[4] = vec4[4](
        sky_upper_cloud_layer(dome.cloud_params, dome.cloud_tint_0, shell_uv, base_lod),
        sky_upper_cloud_layer(dome.cloud_params_1, dome.cloud_tint_1, shell_uv, base_lod),
        sky_upper_cloud_layer(dome.cloud_params_2, dome.cloud_tint_2, shell_uv, base_lod),
        sky_upper_cloud_layer(dome.cloud_params_3, dome.cloud_tint_3, shell_uv, base_lod)
    );
    for (int i = 0; i < 4; ++i) {
        plane = layers[i] + plane * (1.0 - layers[i].a);
    }
    if (plane.a <= 0.0) {
        return vec4(0.0);
    }
    // The authored textures and PNAM tints were made to be composited
    // straight onto the authored sky, so they already carry the air above an
    // overhead view; full aerial perspective would haze the games' cloud art
    // a second time. Apply only the air the path holds BEYOND the zenith
    // path to the same shell: none overhead, so the authored look is exact
    // there, rising toward the horizon, where it fades the plane into the
    // haze — the job the flat plane's `smoothstep(0, 0.12)` horizon fade did.
    vec3 transmittance;
    vec3 in_front;
    sky_air_terms(dir.y, distance, transmittance, in_front);
    vec3 zenith_transmittance;
    vec3 zenith_in_front;
    sky_air_terms(1.0, CLOUD_UPPER_PLANE_ALTITUDE, zenith_transmittance, zenith_in_front);
    vec3 extra_transmittance = min(transmittance / zenith_transmittance, vec3(1.0));
    vec3 extra_in_front = clamp(
        (in_front - zenith_in_front) / max(1.0 - zenith_in_front, vec3(1.0e-5)),
        vec3(0.0), vec3(1.0));
    return vec4(plane.rgb * extra_transmittance + haze * plane.a * extra_in_front, plane.a);
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

    float cos_angle = dot(dir, sun_direction);

    // Sun glow: soft radial halo around the sun. It is forward aerosol
    // scattering in the air, so it belongs to the clear-sky radiance the
    // clouds' aerial perspective fades toward (`clear_sky` below), not to
    // the celestial layer behind them.
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
    sky += sun_col * glow * 0.10 * sun_intensity * dome.weather_sky.w;

    // The haze colour for the aerial perspective: the clear sky in this
    // direction, without the disc, stars, moon or aurora, which are sources
    // behind the air rather than light scattered by it.
    vec3 clear_sky = sky;

    // Sun disc: bright circular spot with a soft edge.
    // sun_size is cos(half-angle) of the disc — lower = wider.
    // Use a smooth transition band outside the core to avoid hard edges
    // from screen-space direction reconstruction precision.
    //
    // `elevation > 0.0` stops the disc painting over the below-horizon
    // ground tint at sunset/sunrise (the `mix(horizon, dome.sky_lower.xyz,
    // below)` above produces a "ground" colour that the disc would
    // otherwise overwrite). #800.
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

        // #sunset-disc — the disc keeps a fixed luminance contrast against
        // the sky behind it: extinction reddens the sun but never dims it
        // below its surroundings until it sets, and a physical disc is
        // orders of magnitude brighter than the sky. The authored term
        // (`disc_color × sun_intensity × sun_glare`) reaches that on its
        // own only for bright-authored suns (FNV's [1.0, 0.49, 0.16] at
        // ×3.5 intensity); Skyrim's dim [0.33, 0.10, 0.0] slid under the
        // horizon glow and rendered no disc at all — the sunset was a
        // diffuse bright band with no sun in it. The floor lifts the
        // authored term by a luminance ratio only (hue preserved), and
        // never attenuates: noon discs already dominate.
        vec3 disc_base = disc_color * sun_intensity * dome.weather_sky.w;
        float disc_lum = dot(disc_base, LUMA_REC709);
        float local_sky_lum = dot(sky, LUMA_REC709);
        vec3 disc_rgb =
            disc_base * max(1.0, local_sky_lum * 1.8 / max(disc_lum, 1.0e-5));
        // No visibility factor: the disc is behind both cloud layers, which
        // composite over it by their own transmittance below.
        sky += disc_rgb * disc;
    }

    sky = weather_sky_details(dome, sky, dir, elevation);

    // SKYAL — the clouds composite over everything above, far to near:
    //
    //   1. the authored WTHR cloud textures on a shell at
    //      CLOUD_UPPER_PLANE_ALTITUDE (9 km), above the volumetric deck;
    //   2. the volumetric deck (`include/clouds.glsl`, 1.5-5 km) in front.
    //
    // Each is premultiplied (radiance, opacity), faded toward `clear_sky` by
    // the air between the viewer and the layer, and composited by its own
    // transmittance, so the sun disc, stars and the upper plane are occluded
    // physically rather than by fixed visibility factors.
    vec4 upper_plane = sky_upper_cloud_plane(dome, dir, clear_sky);
    sky = sky * (1.0 - upper_plane.a) + upper_plane.rgb;

    // The march returns premultiplied radiance, hence `sky * (1 - a) + rgb`.
    float deck_distance;
    vec4 volumetric_cloud = cloud_march(
        dome, dir, cloud_base_noise, cloud_detail_noise, cloud_jitter, deck_distance);
    if (volumetric_cloud.a > 0.0) {
        volumetric_cloud = sky_aerial_perspective(
            volumetric_cloud, clear_sky, dir.y, deck_distance);
    }
    sky = sky * (1.0 - volumetric_cloud.a) + volumetric_cloud.rgb;

    return sky;
}

#endif // SKY_GLSL
