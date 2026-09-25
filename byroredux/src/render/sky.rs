//! Sky parameter assembly — extracted from `build_render_data` per #1115.
//!
//! Pure read-only over `SkyParamsRes`, `CellLightingRes`, and
//! `CloudSimState`; returns a `SkyParams` struct that flows into the
//! camera/scene UBO. No mutation of any of `build_render_data`'s output
//! Vecs.

use byroredux_core::ecs::{TotalTime, World};
use byroredux_renderer::vulkan::context::SkyDalcCube;
use byroredux_renderer::{SkyParams, SkyWeatherParams};

use crate::components::{
    CellLightingRes, CloudSimState, DalcCubeYup, GameTimeRes, InteriorSkyExposureRes, SkyParamsRes,
    WeatherDataRes, WeatherSurfaceState,
};

fn renderer_dalc_cube(cube: DalcCubeYup) -> SkyDalcCube {
    SkyDalcCube {
        pos_x: cube.pos_x,
        neg_x: cube.neg_x,
        pos_y: cube.pos_y,
        neg_y: cube.neg_y,
        pos_z: cube.pos_z,
        neg_z: cube.neg_z,
        specular: cube.specular,
        fresnel_power: cube.fresnel_power,
    }
}

fn interior_dalc_cube(world: &World) -> Option<SkyDalcCube> {
    let cell = world.try_resource::<CellLightingRes>()?;
    if !cell.is_interior {
        return None;
    }
    let faces = cell.directional_ambient.as_ref()?;
    Some(renderer_dalc_cube(DalcCubeYup::from_xcll_zup(
        faces,
        cell.specular_color.unwrap_or([0.0; 3]),
        cell.fresnel_power.unwrap_or(1.0),
    )))
}

/// Assemble per-frame `SkyParams` from world resources.
///
/// Sourced from:
///   * `SkyParamsRes` — rebuilt per exterior load (zenith / horizon /
///     sun / cloud-layer tunables, optional DALC cube).
///   * `CellLightingRes` — supplies an interior XCLL ambient cube when
///     present.
///   * `CloudSimState` — survives cell transitions (per-layer scroll
///     offsets accumulated by `weather_system`).
///
/// When `SkyParamsRes` is absent on an interior-only boot, the room's
/// ordinary lighting fields retain `SkyParams::default()` and a separate
/// outdoor portal palette comes from the procedural fallback. When `CloudSimState` is absent but
/// `SkyParamsRes` is present (first exterior frame), cloud scrolls
/// default to zero.
/// The live exterior TOD/weather zenith colour, readable from inside an
/// interior cell (#3323).
///
/// `SkyParamsRes` is worldspace-scoped with World lifetime (#1199), so it
/// survives the transition into an interior and the weather sim keeps
/// updating it. An interior-only boot (`--cell ...`) uses the same procedural
/// outdoor fallback as a plugin-less exterior, advanced by the live clock.
///
/// This is the exterior colour lane used by the window-portal shader. The
/// separate outdoor sun lane below is consumed only by volumetrics after a
/// geometry visibility test. Composite uses the outdoor palette only at
/// Show Sky depth misses or bounded, modeled openings.
/// Outdoor sun for rays that prove they cross an interior aperture. A direct
/// `--cell` boot has no SkyParamsRes, so derive the same procedural solar arc
/// used by the exterior fallback from the live game clock and climate hours.
fn portal_sun(world: &World) -> ([f32; 3], [f32; 3]) {
    if let Some(sky) = world.try_resource::<SkyParamsRes>() {
        let intensity = if sky.sun_direction[1] > 0.0 {
            sky.sun_intensity.max(0.0)
        } else {
            0.0
        };
        return (
            sky.sun_direction,
            sky.sun_color.map(|channel| channel * intensity),
        );
    }
    let hour = world
        .try_resource::<GameTimeRes>()
        .map_or_else(|| GameTimeRes::default().hour, |time| time.hour);
    let tod_hours = world
        .try_resource::<WeatherDataRes>()
        .map_or(crate::systems::weather::DEFAULT_TOD_HOURS, |weather| {
            weather.tod_hours
        });
    let (direction, intensity) = crate::systems::weather::compute_sun_arc(hour, tod_hours);
    (
        direction,
        crate::env_translate::FB_SUN_COLOR.map(|channel| channel * intensity),
    )
}

pub(super) fn build_sky_params(world: &World) -> SkyParams {
    let interior_cube = interior_dalc_cube(world);

    // #1199 — `SkyParamsRes` is worldspace-scoped and survives cell
    // transitions. Interior surface lighting must retain its own values
    // (#2226). The composite uses the outdoor palette only on clear-depth
    // aperture pixels; the portal sun reaches volumetrics only after a
    // geometry ray proves an opening.
    // SKYAL — snapshot the cell's directional inputs in the same short borrow
    // that decides interiority, so `CellLightingRes` is never held while
    // `SkyParamsRes` is acquired below. That Cell->Sky nesting is the pair
    // #1410's lock-order detector flags; `render::lights::collect_lights`
    // avoids it the same way.
    let cell_directional = world.try_resource::<CellLightingRes>().map(|cell| {
        (
            cell.is_interior,
            cell.directional_color,
            cell.directional_fade,
        )
    });
    let is_interior = cell_directional.is_some_and(|(interior, _, _)| interior);
    if is_interior {
        let (portal_sun_direction, portal_sun_radiance) = portal_sun(world);
        let mut outdoor = if let Some(sky_res) = world.try_resource::<SkyParamsRes>() {
            outdoor_sky_params(world, &sky_res, None)
        } else {
            // Direct `--cell` boot has no surviving worldspace sky. Bake the
            // same procedural outdoor fallback used by an exterior boot,
            // with its sun driven by the live clock rather than a frozen noon.
            let hour = world
                .try_resource::<GameTimeRes>()
                .map_or_else(|| GameTimeRes::default().hour, |time| time.hour);
            let tod_hours = world
                .try_resource::<WeatherDataRes>()
                .map_or(crate::systems::weather::DEFAULT_TOD_HOURS, |weather| {
                    weather.tod_hours
                });
            let (direction, intensity) = crate::systems::weather::compute_sun_arc(hour, tod_hours);
            let mut fallback = crate::env_translate::procedural_fallback_sky(direction);
            fallback.sun_intensity = intensity;
            outdoor_sky_params(world, &fallback, None)
        };
        outdoor.is_exterior = true;
        return SkyParams {
            dalc_cube: interior_cube,
            portal_sun_direction,
            portal_sun_radiance,
            interior_show_sky: world
                .try_resource::<InteriorSkyExposureRes>()
                .is_some_and(|exposure| exposure.0),
            // #3323 — the exterior colour lane for `triangle.frag`'s
            // window-portal escape. Pinning it to `SkyParams::default()` made
            // every FNV interior window transmit clear-noon blue at 03:00,
            // which is the exact symptom #925 claimed to have fixed on the
            // exact cells it named (Vault 21/34/22, the Novac motel rooms).
            //
            // `SkyParamsRes` is worldspace-scoped and survives the transition
            // into an interior. On a direct `--cell` boot the same exterior
            // procedural fallback is baked for the portal instead.
            exterior_zenith_color: outdoor.zenith_color,
            portal_outdoor_sky: Some(outdoor.into()),
            ..SkyParams::default()
        };
    }

    let Some(sky_res) = world.try_resource::<SkyParamsRes>() else {
        return SkyParams {
            dalc_cube: interior_cube,
            ..SkyParams::default()
        };
    };
    outdoor_sky_params(world, &sky_res, cell_directional)
}

/// `cell_directional` is the active cell's `(is_interior, directional colour,
/// directional fade)`. `Some` lights the clouds with the sun the cell's
/// surfaces receive; `None` (the interior portal palette, where the cell's
/// directional is the room's XCLL key and not the sky's) lights them with the
/// exterior's own sunlight colour instead, so a cloud seen through a window
/// matches the same weather outdoors (#4839).
fn outdoor_sky_params(
    world: &World,
    sky_res: &SkyParamsRes,
    cell_directional: Option<(bool, [f32; 3], Option<f32>)>,
) -> SkyParams {
    let clouds = world.try_resource::<CloudSimState>();
    let scroll = clouds
        .as_ref()
        .map(|c| {
            (
                c.cloud_scroll,
                c.cloud_scroll_1,
                c.cloud_scroll_2,
                c.cloud_scroll_3,
            )
        })
        .unwrap_or_default();
    let weather = SkyWeatherParams {
        cloud_coverage: sky_res.weather.cloud_coverage,
        cloud_tints: sky_res.weather.cloud_tints,
        precipitation: sky_res.weather.precipitation,
        thunder_frequency: sky_res.weather.thunder_frequency,
        lightning_color: sky_res.weather.lightning_color,
        stars_color: sky_res.weather.stars_color,
        sun_glare: sky_res.weather.sun_glare,
        moon_glare: sky_res.weather.moon_glare,
        aurora_intensity: sky_res.weather.aurora_intensity,
        aurora_follows_sun: sky_res.weather.aurora_follows_sun,
        wind_direction: sky_res.weather.wind_direction,
        wind_speed: sky_res.weather.wind_speed,
        surface_wetness: world
            .try_resource::<WeatherSurfaceState>()
            .map_or(0.0, |surface| surface.wetness),
        surface_snow: world
            .try_resource::<WeatherSurfaceState>()
            .map_or(0.0, |surface| surface.snow),
    };
    SkyParams {
        zenith_color: sky_res.zenith_color,
        // On an exterior the two are the same sky by definition; the lane
        // only diverges on interiors (#3323).
        exterior_zenith_color: sky_res.zenith_color,
        portal_outdoor_sky: None,
        portal_sun_radiance: [0.0; 3],
        portal_sun_direction: [0.0, -1.0, 0.0],
        interior_show_sky: false,
        horizon_color: sky_res.horizon_color,
        lower_color: sky_res.lower_color,
        sun_direction: sky_res.sun_direction,
        sun_color: sky_res.sun_color,
        sun_size: sky_res.sun_size,
        sun_intensity: sky_res.sun_intensity,
        // The same call `collect_lights` makes for the surfaces' directional
        // light, so the volumetric clouds are lit by the sun the terrain is.
        sun_illuminance: {
            let (interior, color, fade) =
                cell_directional.unwrap_or((false, sky_res.weather.sunlight_color, None));
            super::compute_directional_upload(
                &color,
                interior,
                sky_res.sun_intensity,
                fade,
                weather.cloud_coverage,
            )
        },
        // Tangent-plane disk approximation valid only for α < ~0.05 rad
        // (derivation documented at the directional-shadow-jitter block in
        // triangle.frag's legacy-WRS arm, next to `sunAngularRadius`; the
        // ReSTIR arm's sampler carries a one-line back-reference to the same
        // spot). Debug-mode guard so a per-cell override above 0.1 rad fails
        // loudly instead of silently producing biased penumbras.
        // (#1109 / REN-D20-002)
        sun_angular_radius: {
            debug_assert!(
                sky_res.sun_angular_radius < 0.10,
                "sun_angular_radius {:.4} rad exceeds tangent-plane approximation \
                 threshold (~0.05 rad); penumbra sampling will be visibly biased.",
                sky_res.sun_angular_radius,
            );
            sky_res.sun_angular_radius
        },
        is_exterior: sky_res.is_exterior,
        cloud_scroll: scroll.0,
        cloud_tile_scale: sky_res.cloud_tile_scale,
        cloud_texture_index: sky_res.cloud_texture_index,
        sun_texture_index: sky_res.sun_texture_index,
        cloud_scroll_1: scroll.1,
        cloud_tile_scale_1: sky_res.cloud_tile_scale_1,
        cloud_texture_index_1: sky_res.cloud_texture_index_1,
        cloud_scroll_2: scroll.2,
        cloud_tile_scale_2: sky_res.cloud_tile_scale_2,
        cloud_texture_index_2: sky_res.cloud_texture_index_2,
        cloud_scroll_3: scroll.3,
        cloud_tile_scale_3: sky_res.cloud_tile_scale_3,
        cloud_texture_index_3: sky_res.cloud_texture_index_3,
        // #993 — pass the per-TOD-lerped 6-axis ambient cube
        // through to the renderer. Engine-Y-up axes (the
        // Zup → Yup swap lives in DalcCubeYup::from_skyrim_zup).
        dalc_cube: sky_res.current_dalc_cube.map(renderer_dalc_cube),
        weather,
        weather_time_seconds: world.try_resource::<TotalTime>().map_or(0.0, |time| time.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::WeatherSkyState;

    fn interior_lighting(directional_ambient: Option<[[f32; 3]; 6]>) -> CellLightingRes {
        CellLightingRes {
            ambient: [0.0; 3],
            directional_color: [0.0; 3],
            directional_dir: [0.0, -1.0, 0.0],
            is_interior: true,
            fog_color: [0.0; 3],
            fog_near: 0.0,
            fog_far: 0.0,
            fog_medium: crate::fog::FogMedium::DISABLED,
            directional_fade: None,
            fog_clip: None,
            fog_power: None,
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient,
            specular_color: Some([0.2, 0.4, 0.6]),
            specular_alpha: Some(0.0),
            fresnel_power: Some(1.5),
            inheritance_flags: None,
        }
    }

    #[test]
    fn interior_xcll_cube_reaches_sky_params_without_exterior_resource() {
        let mut world = World::new();
        let faces = [
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [6.0, 0.0, 0.0],
        ];
        world.insert_resource(interior_lighting(Some(faces)));

        let cube = build_sky_params(&world)
            .dalc_cube
            .expect("interior XCLL ambient cube must reach the renderer");
        assert_eq!(cube.pos_x, faces[0]);
        assert_eq!(cube.neg_x, faces[1]);
        assert_eq!(cube.neg_z, faces[2]);
        assert_eq!(cube.pos_z, faces[3]);
        assert_eq!(cube.pos_y, faces[4]);
        assert_eq!(cube.neg_y, faces[5]);
        assert_eq!(cube.specular, [0.2, 0.4, 0.6]);
        assert_eq!(cube.fresnel_power, 1.5);
    }

    #[test]
    fn missing_xcll_cube_preserves_default_sky_params() {
        let mut world = World::new();
        world.insert_resource(interior_lighting(None));

        assert!(build_sky_params(&world).dalc_cube.is_none());
    }

    fn stale_exterior_daytime_sky() -> SkyParamsRes {
        // Simulates a `SkyParamsRes` left over from a prior exterior
        // worldspace load (#1199 — worldspace-scoped, survives cell
        // unload/transition by design, always constructed with
        // `is_exterior: true`). Every field is deliberately non-default
        // so the regression test below can prove none of them leak.
        SkyParamsRes {
            zenith_color: [0.3, 0.5, 0.9],
            horizon_color: [0.8, 0.8, 0.9],
            lower_color: [0.4, 0.4, 0.45],
            sun_direction: [0.5, 0.8, 0.3],
            sun_color: [1.0, 0.95, 0.85],
            sun_size: 0.02,
            sun_intensity: 4.0,
            sun_angular_radius: 0.020,
            is_exterior: true,
            cloud_tile_scale: 0.6,
            cloud_texture_index: 7,
            sun_texture_index: 3,
            cloud_tile_scale_1: 0.5,
            cloud_texture_index_1: 8,
            cloud_tile_scale_2: 0.4,
            cloud_texture_index_2: 9,
            cloud_tile_scale_3: 0.3,
            cloud_texture_index_3: 10,
            current_dalc_cube: None,
            weather: crate::components::WeatherSkyState::default(),
        }
    }

    /// REN-D18-01 / #2226 — a stale exterior `SkyParamsRes` surviving a
    /// transition into an interior cell must not leak *any* field, not
    /// just `dalc_cube`. Every non-cube field must fall back to
    /// `SkyParams::default()` (in particular `is_exterior: false` and
    /// `cloud_tile_scale: 0.0`, which gate the TOD sky/sun/cloud terms
    /// off in the shader) even though `SkyParamsRes` is present.
    #[test]
    fn stale_exterior_sky_params_res_does_not_leak_into_interior() {
        let mut world = World::new();
        world.insert_resource(interior_lighting(None));
        world.insert_resource(stale_exterior_daytime_sky());
        world.insert_resource(WeatherSurfaceState {
            wetness: 0.8,
            snow: 0.6,
        });

        let params = build_sky_params(&world);
        let default = SkyParams::default();

        assert!(!params.is_exterior, "must not report exterior");
        assert_eq!(params.zenith_color, default.zenith_color);
        assert_eq!(params.horizon_color, default.horizon_color);
        assert_eq!(params.sun_direction, default.sun_direction);
        assert_eq!(params.sun_intensity, default.sun_intensity);
        assert_eq!(params.portal_sun_direction, [0.5, 0.8, 0.3]);
        assert_eq!(params.portal_sun_radiance, [4.0, 3.8, 3.4]);
        assert_eq!(params.cloud_tile_scale, default.cloud_tile_scale);
        assert_eq!(params.cloud_texture_index, default.cloud_texture_index);
        assert!(params.dalc_cube.is_none());
        assert_eq!(params.weather.surface_wetness, 0.0);
        assert_eq!(params.weather.surface_snow, 0.0);
        assert_eq!(
            params.sun_illuminance, default.sun_illuminance,
            "the room's own sky lane must stay unlit by the exterior sun — \
             only the portal palette carries it (#4839)"
        );
        // #3323 — the one deliberate exception, added *after* #2226 and
        // narrower than what #2226 removed. `exterior_zenith_color` is a
        // separate lane read by exactly one shader branch (the window-portal
        // escape, where the ray provably left the cell), so it carries the
        // live sky while every field asserted above stays defaulted.
        assert_eq!(
            params.exterior_zenith_color,
            [0.3, 0.5, 0.9],
            "the exterior sky lane must survive into an interior — that is \
             the whole point of #3323"
        );
        assert_ne!(
            params.exterior_zenith_color, params.zenith_color,
            "the two lanes must not collapse into one: `zenith_color` also \
             drives CompositeParams::sky_zenith, which is the interior sky \
             leak #2226 removed"
        );
        let outside = params.portal_outdoor_sky.as_ref().unwrap();
        assert!(outside.is_exterior);
        assert_eq!(outside.zenith_color, [0.3, 0.5, 0.9]);
        assert_eq!(outside.sun_direction, [0.5, 0.8, 0.3]);
    }

    /// #4839 — clouds seen through an aperture are lit by the exterior's own
    /// sunlight. Not the room's XCLL directional (a different quantity), and
    /// not `portal_sun_radiance` (sun-disc colour × 0–4, different units).
    #[test]
    fn interior_portal_sky_clouds_are_lit_by_the_exterior_sunlight() {
        let mut world = World::new();
        let mut room = interior_lighting(None);
        room.directional_color = [0.2, 0.2, 0.2];
        world.insert_resource(room);
        let mut sky = stale_exterior_daytime_sky();
        // Clear sky at full daylight, so the illuminance is exactly the colour.
        sky.weather = WeatherSkyState {
            cloud_coverage: 0.0,
            sunlight_color: [0.9, 0.6, 0.3],
            ..WeatherSkyState::default()
        };
        world.insert_resource(sky);

        let outside = build_sky_params(&world).portal_outdoor_sky.unwrap();
        assert_eq!(outside.sun_illuminance, [0.9, 0.6, 0.3]);

        // A heavier cloud column between the sun and the cloud body dims it,
        // exactly as it does outdoors.
        world
            .try_resource_mut::<SkyParamsRes>()
            .unwrap()
            .weather
            .cloud_coverage = 1.0;
        let overcast = build_sky_params(&world).portal_outdoor_sky.unwrap();
        assert!(
            overcast.sun_illuminance[0] < outside.sun_illuminance[0]
                && overcast.sun_illuminance[0] > 0.0
        );
    }

    /// Direct `--cell` boot still has a procedural outdoor sky for the
    /// window-escape cube, even though no exterior has been loaded.
    #[test]
    fn interior_only_session_bakes_procedural_outdoor_sky() {
        let mut world = World::new();
        world.insert_resource(interior_lighting(None));

        let params = build_sky_params(&world);
        assert_eq!(
            params.exterior_zenith_color,
            crate::env_translate::procedural_fallback_sky([0.0, 1.0, 0.0]).zenith_color,
            "direct interior boot should use the procedural exterior palette"
        );
        assert!(params.portal_outdoor_sky.unwrap().is_exterior);
    }

    #[test]
    fn interior_only_boot_uses_live_clock_for_portal_sun() {
        let mut world = World::new();
        world.insert_resource(interior_lighting(None));
        world.insert_resource(GameTimeRes::frozen_at(12.0));

        let noon = build_sky_params(&world);
        assert!(noon.portal_sun_direction[1] > 0.0);
        assert!(
            noon.portal_sun_radiance
                .iter()
                .all(|channel| *channel > 0.0)
        );
        let noon_outside = noon.portal_outdoor_sky.as_ref().unwrap();
        assert!(noon_outside.is_exterior);
        assert_eq!(noon_outside.sun_direction, noon.portal_sun_direction);
        assert!(noon_outside.sun_intensity > 0.0);
        assert!(
            noon_outside.sun_illuminance.iter().all(|c| *c > 0.0),
            "the procedural exterior's sun must light its clouds through an \
             interior aperture too (#4839)"
        );
        assert_eq!(noon.sun_direction, SkyParams::default().sun_direction);
        assert!(!noon.is_exterior);

        world
            .try_resource_mut::<GameTimeRes>()
            .unwrap()
            .set_hour(0.0);
        let midnight = build_sky_params(&world);
        assert_eq!(midnight.portal_sun_radiance, [0.0; 3]);
        let midnight_outside = midnight.portal_outdoor_sky.unwrap();
        assert_eq!(midnight_outside.sun_intensity, 0.0);
        assert_eq!(midnight_outside.sun_illuminance, [0.0; 3]);
    }

    #[test]
    fn interior_portal_sun_stays_dark_below_horizon() {
        let mut world = World::new();
        world.insert_resource(interior_lighting(None));
        let mut sky = stale_exterior_daytime_sky();
        sky.sun_direction = [0.0, -1.0, 0.0];
        world.insert_resource(sky);

        let params = build_sky_params(&world);
        assert_eq!(params.portal_sun_radiance, [0.0; 3]);
    }

    #[test]
    fn show_sky_interior_allows_open_sky_visibility_without_exterior_lighting() {
        let mut world = World::new();
        world.insert_resource(interior_lighting(None));
        world.insert_resource(InteriorSkyExposureRes(true));

        let params = build_sky_params(&world);
        assert!(params.interior_show_sky);
        assert!(!params.is_exterior);
        assert_eq!(params.sun_direction, SkyParams::default().sun_direction);
    }

    /// On an exterior the two lanes describe the same sky, so they must
    /// agree — a divergence there would mean the portal and the composite
    /// pass paint different skies through the same window.
    #[test]
    fn exterior_cell_reports_the_same_sky_on_both_lanes() {
        let mut world = World::new();
        world.insert_resource(stale_exterior_daytime_sky());
        world.insert_resource(WeatherSurfaceState {
            wetness: 0.8,
            snow: 0.6,
        });

        let params = build_sky_params(&world);
        assert!(params.is_exterior);
        assert_eq!(params.exterior_zenith_color, params.zenith_color);
        assert_eq!(params.portal_sun_radiance, [0.0; 3]);
        assert_eq!(params.exterior_zenith_color, [0.3, 0.5, 0.9]);
        assert_eq!(params.weather.surface_wetness, 0.8);
        assert_eq!(params.weather.surface_snow, 0.6);
    }

    /// Sibling of the above with an XCLL cube present: the interior cube
    /// must still win even though a stale exterior `SkyParamsRes` (with
    /// its own `current_dalc_cube`) is also present.
    #[test]
    fn stale_exterior_sky_params_res_does_not_override_interior_xcll_cube() {
        let mut world = World::new();
        let faces = [
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [6.0, 0.0, 0.0],
        ];
        world.insert_resource(interior_lighting(Some(faces)));
        world.insert_resource(stale_exterior_daytime_sky());

        let params = build_sky_params(&world);
        assert!(!params.is_exterior);
        let cube = params
            .dalc_cube
            .expect("interior XCLL ambient cube must reach the renderer");
        assert_eq!(cube.pos_x, faces[0]);
    }
}
