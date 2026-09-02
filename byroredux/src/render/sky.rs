//! Sky parameter assembly — extracted from `build_render_data` per #1115.
//!
//! Pure read-only over `SkyParamsRes`, `CellLightingRes`, and
//! `CloudSimState`; returns a `SkyParams` struct that flows into the
//! camera/scene UBO. No mutation of any of `build_render_data`'s output
//! Vecs.

use byroredux_core::ecs::World;
use byroredux_renderer::vulkan::context::SkyDalcCube;
use byroredux_renderer::SkyParams;

use crate::components::{CellLightingRes, CloudSimState, DalcCubeYup, SkyParamsRes};

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
/// When `SkyParamsRes` is absent (interior cell, or no exterior load
/// yet this session), all fields except a present interior XCLL cube
/// retain `SkyParams::default()`. When `CloudSimState` is absent but
/// `SkyParamsRes` is present (first exterior frame), cloud scrolls
/// default to zero.
/// The live exterior TOD/weather zenith colour, readable from inside an
/// interior cell (#3323).
///
/// `SkyParamsRes` is worldspace-scoped with World lifetime (#1199), so it
/// survives the transition into an interior and the weather sim keeps
/// updating it. Returns the `SkyParams::default()` zenith when no exterior
/// has ever loaded this session — an interior-only boot (`--cell ...`) has
/// no outdoor sky to report, and the portal keeps its pre-#3323 constant.
///
/// This is deliberately the **only** exterior field an interior reads. Do
/// not grow it into a general "inherit the exterior sky" helper: that is
/// what #2226 removed, and a sealed roof only hides the resulting leak.
fn exterior_zenith_color(world: &World) -> [f32; 3] {
    world
        .try_resource::<SkyParamsRes>()
        .map_or_else(|| SkyParams::default().zenith_color, |sky| sky.zenith_color)
}

pub(super) fn build_sky_params(world: &World) -> SkyParams {
    let interior_cube = interior_dalc_cube(world);

    // #1199 — `SkyParamsRes` is worldspace-scoped and survives cell
    // unload/transition by design, and is *only ever* constructed with
    // `is_exterior: true`. An interior cell must therefore never read any
    // field from a stale exterior `SkyParamsRes` — not just `dalc_cube`
    // (REN-D18-01 / #2226: a partial fix once left `is_exterior` and the
    // whole TOD sky/sun/cloud set leaking through on any interior with an
    // unsealed roof gap or failed mesh, since a sealed interior only hides
    // the symptom by gating the sky term on `depth == 1.0`). Decide
    // interiority once, up front, from the same resource `interior_cube`
    // itself already consulted.
    let is_interior = world
        .try_resource::<CellLightingRes>()
        .is_some_and(|cell| cell.is_interior);
    if is_interior {
        return SkyParams {
            dalc_cube: interior_cube,
            // #3323 — the one exterior field an interior *does* carry.
            // Everything else stays at the default on purpose (see above):
            // an interior must not read a stale exterior sky, because the
            // TOD/sun/cloud set leaking into interior lighting is the bug
            // #2226 removed. But `triangle.frag`'s window-portal escape is
            // the one consumer where "this pixel sees the outdoors" is the
            // premise, not a leak — a ray that clears the cell genuinely
            // sees today's sky. Pinning it to `SkyParams::default()` made
            // every FNV interior window transmit clear-noon blue at 03:00,
            // which is the exact symptom #925 claimed to have fixed on the
            // exact cells it named (Vault 21/34/22, the Novac motel rooms).
            //
            // `SkyParamsRes` is worldspace-scoped and its lifetime matches
            // the World, not the cell (#1199 — see `cell_loader::unload`'s
            // note), so it survives the transition into an interior and the
            // weather sim keeps updating it. When no exterior has loaded
            // this session it is absent and the default stands, which is
            // the pre-#3323 behaviour.
            exterior_zenith_color: exterior_zenith_color(world),
            ..SkyParams::default()
        };
    }

    let Some(sky_res) = world.try_resource::<SkyParamsRes>() else {
        return SkyParams {
            dalc_cube: interior_cube,
            ..SkyParams::default()
        };
    };
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
    SkyParams {
        zenith_color: sky_res.zenith_color,
        // On an exterior the two are the same sky by definition; the lane
        // only diverges on interiors (#3323).
        exterior_zenith_color: sky_res.zenith_color,
        horizon_color: sky_res.horizon_color,
        lower_color: sky_res.lower_color,
        sun_direction: sky_res.sun_direction,
        sun_color: sky_res.sun_color,
        sun_size: sky_res.sun_size,
        sun_intensity: sky_res.sun_intensity,
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
        // `interior_cube` is always `None` on this path (the early
        // `is_interior` return above handles every interior case), kept
        // as a defensive `or_else` rather than removed outright.
        dalc_cube: interior_cube.or_else(|| sky_res.current_dalc_cube.map(renderer_dalc_cube)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            sun_direction: [0.5, -0.8, 0.3],
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

        let params = build_sky_params(&world);
        let default = SkyParams::default();

        assert!(!params.is_exterior, "must not report exterior");
        assert_eq!(params.zenith_color, default.zenith_color);
        assert_eq!(params.horizon_color, default.horizon_color);
        assert_eq!(params.sun_direction, default.sun_direction);
        assert_eq!(params.sun_intensity, default.sun_intensity);
        assert_eq!(params.cloud_tile_scale, default.cloud_tile_scale);
        assert_eq!(params.cloud_texture_index, default.cloud_texture_index);
        assert!(params.dalc_cube.is_none());
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
    }

    /// #3323 — with no exterior ever loaded this session (an interior-only
    /// `--cell` boot), there is no live sky to report and the portal must
    /// keep its pre-#3323 constant rather than transmitting black.
    #[test]
    fn interior_only_session_falls_back_to_the_default_exterior_zenith() {
        let mut world = World::new();
        world.insert_resource(interior_lighting(None));

        let params = build_sky_params(&world);
        assert_eq!(
            params.exterior_zenith_color,
            SkyParams::default().zenith_color,
            "no SkyParamsRes means no outdoor sky to report"
        );
    }

    /// On an exterior the two lanes describe the same sky, so they must
    /// agree — a divergence there would mean the portal and the composite
    /// pass paint different skies through the same window.
    #[test]
    fn exterior_cell_reports_the_same_sky_on_both_lanes() {
        let mut world = World::new();
        world.insert_resource(stale_exterior_daytime_sky());

        let params = build_sky_params(&world);
        assert!(params.is_exterior);
        assert_eq!(params.exterior_zenith_color, params.zenith_color);
        assert_eq!(params.exterior_zenith_color, [0.3, 0.5, 0.9]);
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
