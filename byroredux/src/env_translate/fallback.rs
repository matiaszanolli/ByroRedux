//! EXAL procedural fallback: the canonical no-climate / no-weather cell
//! lighting, sky and weather (a warm Mojave-style desert default).

use crate::components::{CellLightingRes, SkyParamsRes, WeatherDataRes, WeatherSkyState};
use super::sky_weather::{SUN_ANGULAR_RADIUS, SUN_INTENSITY_PEAK, SUN_SIZE_COS};

// ── Procedural fallback (no resolved climate / weather) ──
//
// Warm Mojave-style desert sky. Same values the bulk loader used pre-#M40;
// kept here as the canonical no-data default rather than an inline block in
// the render-setup path (EXAL §3: the fallback is an explicit canonical
// constructor, not a render-time heuristic).
const FB_AMBIENT: [f32; 3] = [0.15, 0.14, 0.12];
pub(crate) const FB_SUNLIGHT: [f32; 3] = [1.0, 0.95, 0.8];
const FB_FOG_COLOR: [f32; 3] = [0.65, 0.7, 0.8];
const FB_ZENITH: [f32; 3] = [0.15, 0.3, 0.65];
const FB_HORIZON: [f32; 3] = [0.55, 0.5, 0.42];
// Pre-#541 the `compute_sky` below-horizon branch faked the ground tint as
// `horizon * 0.3`; matching that keeps the procedural look unchanged.
const FB_LOWER: [f32; 3] = [
    FB_HORIZON[0] * 0.3,
    FB_HORIZON[1] * 0.3,
    FB_HORIZON[2] * 0.3,
];
// #4902 — private again: the render loop no longer re-derives the fallback sky.
const FB_SUN_COLOR: [f32; 3] = [1.0, 0.95, 0.8];
/// Star colour of the procedural fallback sky — and the one declaration
/// behind [`WeatherSkyState::default`]'s `stars_color` (#4494, same
/// anti-drift alias arrangement as [`FB_TOD_HOURS`]).
pub(crate) const FB_STARS_COLOR: [f32; 3] = [0.75, 0.8, 1.0];
/// Sunrise-begin / sunrise-end / sunset-begin / sunset-end breakpoints used
/// whenever no climate record drives them — the **one** declaration (#2812).
///
/// Same "exterior, no authored climate" state, three producers that must
/// agree: [`climate_tod_hours`]'s no-/invalid-climate return,
/// [`procedural_fallback_weather`]'s `tod_hours`, and the sun arc
/// `weather::apply_neutral_exterior_fallback` evaluates (via
/// `weather::DEFAULT_TOD_HOURS`, which aliases this).
///
/// It used to be written out independently at all three. That is the exact
/// shape `apply_neutral_exterior_fallback`'s own doc warns about — "the
/// **one** canonical EXAL boundary fallback … not a private set", the #1722
/// lesson — and `DEFAULT_TOD_HOURS`'s doc asserted the coupling while
/// referencing neither of the other two literals. A re-anchor applied to one
/// or two would silently split the fallback sun arc from the fallback palette
/// interpolation.
///
/// Value is the pre-#463 hardcoded default (`exal.md` boundary fallback).
pub(crate) const FB_TOD_HOURS: [f32; 4] = [6.0, 10.0, 18.0, 22.0];
const FB_FOG_NEAR: f32 = 15000.0;
const FB_FOG_FAR: f32 = 80000.0;

/// Procedural-fallback exterior lighting (no plugin data → engine defaults).
pub(crate) fn procedural_fallback_cell_lighting(sun_dir: [f32; 3]) -> CellLightingRes {
    CellLightingRes::bare(
        FB_AMBIENT,
        FB_SUNLIGHT,
        sun_dir,
        false,
        FB_FOG_COLOR,
        FB_FOG_NEAR,
        FB_FOG_FAR,
    )
}

/// Procedural-fallback sky (no clouds, procedural sun disc).
pub(crate) fn procedural_fallback_sky(sun_dir: [f32; 3]) -> SkyParamsRes {
    SkyParamsRes {
        zenith_color: FB_ZENITH,
        horizon_color: FB_HORIZON,
        lower_color: FB_LOWER,
        sun_direction: sun_dir,
        sun_color: FB_SUN_COLOR,
        sun_size: SUN_SIZE_COS,
        sun_intensity: SUN_INTENSITY_PEAK,
        sun_angular_radius: SUN_ANGULAR_RADIUS,
        is_exterior: true,
        cloud_tile_scale: 0.0,
        cloud_texture_index: 0,
        sun_texture_index: 0,
        cloud_tile_scale_1: 0.0,
        cloud_texture_index_1: 0,
        cloud_tile_scale_2: 0.0,
        cloud_texture_index_2: 0,
        cloud_tile_scale_3: 0.0,
        cloud_texture_index_3: 0,
        current_dalc_cube: None,
        // #5179 — a plugin-less world has no climate; the same fallback quad
        // `climate_tod_hours(None)` returns keeps the two seed paths aligned.
        tod_hours: FB_TOD_HOURS,
        weather: WeatherSkyState::default(),
    }
}

/// Synthetic [`WeatherDataRes`] for the fallback: every TOD slot of the six
/// groups `weather_system` reads carries the same procedural colour, so the
/// TOD lerp re-writes the same values each frame while still advancing
/// `sun_direction` / `sun_intensity`. See #542 / M33-10.
pub(crate) fn procedural_fallback_weather() -> WeatherDataRes {
    use byroredux_plugin::esm::records::weather::{
        SKY_AMBIENT, SKY_COLOR_GROUPS, SKY_FOG, SKY_HORIZON, SKY_LOWER, SKY_SUN, SKY_SUNLIGHT,
        SKY_TIME_SLOTS, SKY_UPPER,
    };
    let mut sky_colors = [[[0.0f32; 3]; SKY_TIME_SLOTS]; SKY_COLOR_GROUPS];
    let synthetic = [
        (SKY_UPPER, FB_ZENITH),
        (SKY_FOG, FB_FOG_COLOR),
        (SKY_AMBIENT, FB_AMBIENT),
        (SKY_SUNLIGHT, FB_SUNLIGHT),
        (SKY_SUN, FB_SUN_COLOR),
        (
            byroredux_plugin::esm::records::weather::SKY_STARS,
            FB_STARS_COLOR,
        ),
        // #541 — `weather_system` also reads SKY_LOWER for the below-horizon
        // branch; synthetic value matches the procedural `FB_LOWER`.
        (SKY_LOWER, FB_LOWER),
        (SKY_HORIZON, FB_HORIZON),
    ];
    for (group, color) in synthetic {
        sky_colors[group].fill(color);
    }
    WeatherDataRes {
        sky_colors,
        // Day/night fog distances kept identical — no authored night distance.
        fog: [FB_FOG_NEAR, FB_FOG_FAR, FB_FOG_NEAR, FB_FOG_FAR],
        fog_media: [
            crate::fog::FogMedium::from_legacy_ramp(FB_FOG_NEAR, FB_FOG_FAR, None),
            crate::fog::FogMedium::from_legacy_ramp(FB_FOG_NEAR, FB_FOG_FAR, None),
        ],
        // Pre-#463 hardcoded TOD breakpoints — shared with
        // `climate_tod_hours` and `weather::DEFAULT_TOD_HOURS` (#2812).
        tod_hours: FB_TOD_HOURS,
        skyrim_dalc_per_tod: None,
        wind_speed: 0,
        precipitation: 0.0,
        cloud_layer_velocities: [[0.0; 2]; 4],
        cloud_layer_velocities_authored: [false; 4],
        cloud_layer_colors: [[[1.0; 3]; 4]; 4],
        cloud_layer_alphas: [[1.0; 4]; 4],
        weather: WeatherSkyState::default(),
        grass_dimmer: 1.0,
        sunlight_dimmer: 1.0,
        image_space: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::weather::{SKY_AMBIENT, SKY_HORIZON, TOD_DAY};

    #[test]
    fn procedural_fallback_pins_mojave_defaults() {
        let sun_dir = [-0.4, 0.8, -0.45];
        let cl = procedural_fallback_cell_lighting(sun_dir);
        assert_eq!(cl.ambient, [0.15, 0.14, 0.12]);
        assert_eq!(cl.directional_dir, sun_dir);
        assert_eq!((cl.fog_near, cl.fog_far), (15000.0, 80000.0));
        assert!(cl.fog_medium.extinction_per_meter > 0.0);
        assert!(!cl.is_interior);

        let sky = procedural_fallback_sky(sun_dir);
        assert_eq!(sky.zenith_color, [0.15, 0.3, 0.65]);
        // Below-horizon ground tint matches the pre-#541 `horizon * 0.3`.
        assert_eq!(sky.lower_color, [0.55 * 0.3, 0.5 * 0.3, 0.42 * 0.3]);
        assert_eq!(sky.cloud_tile_scale, 0.0); // no clouds in the fallback
        assert_eq!(sky.sun_texture_index, 0); // procedural disc

        let wd = procedural_fallback_weather();
        assert_eq!(wd.tod_hours, [6.0, 10.0, 18.0, 22.0]);
        assert_eq!(wd.wind_speed, 0);
        assert_eq!(wd.fog_media, [cl.fog_medium; 2]);
        assert!(wd.skyrim_dalc_per_tod.is_none());
        // Synthetic table: the day slot of the read groups carries the
        // procedural colour, and the lerp endpoints are equal.
        assert_eq!(wd.sky_colors[SKY_AMBIENT][TOD_DAY], [0.15, 0.14, 0.12]);
        assert_eq!(
            wd.sky_colors[SKY_HORIZON][0],
            wd.sky_colors[SKY_HORIZON][TOD_DAY]
        );
    }
}
