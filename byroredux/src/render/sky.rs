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
pub(super) fn build_sky_params(world: &World) -> SkyParams {
    let interior_cube = interior_dalc_cube(world);
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
        horizon_color: sky_res.horizon_color,
        lower_color: sky_res.lower_color,
        sun_direction: sky_res.sun_direction,
        sun_color: sky_res.sun_color,
        sun_size: sky_res.sun_size,
        sun_intensity: sky_res.sun_intensity,
        // Tangent-plane disk approximation valid only for α < ~0.05 rad
        // (documented in triangle.frag:2418-2425). Debug-mode guard so a
        // per-cell override above 0.1 rad fails loudly instead of silently
        // producing biased penumbras. (#1109 / REN-D20-002)
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
        // Interior XCLL is authoritative if a stale exterior
        // SkyParamsRes survived a cell transition.
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
}
