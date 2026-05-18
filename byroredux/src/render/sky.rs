//! Sky parameter assembly — extracted from `build_render_data` per #1115.
//!
//! Pure read-only over `SkyParamsRes` + `CloudSimState`; returns a
//! `SkyParams` struct that flows into the camera/scene UBO. No mutation
//! of any of `build_render_data`'s output Vecs.

use byroredux_core::ecs::World;
use byroredux_renderer::SkyParams;

use crate::components::{CloudSimState, SkyParamsRes};

/// Assemble per-frame `SkyParams` from world resources.
///
/// Sourced from:
///   * `SkyParamsRes` — rebuilt per exterior load (zenith / horizon /
///     sun / cloud-layer tunables, optional DALC cube).
///   * `CloudSimState` — survives cell transitions (per-layer scroll
///     offsets accumulated by `weather_system`).
///
/// When `SkyParamsRes` is absent (interior cell, or no exterior load
/// yet this session) returns `SkyParams::default()`. When
/// `CloudSimState` is absent but `SkyParamsRes` is present (first
/// exterior frame), cloud scrolls default to zero.
pub(super) fn build_sky_params(world: &World) -> SkyParams {
    let Some(sky_res) = world.try_resource::<SkyParamsRes>() else {
        return SkyParams::default();
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
        dalc_cube: sky_res.current_dalc_cube.map(|c| {
            byroredux_renderer::vulkan::context::SkyDalcCube {
                pos_x: c.pos_x,
                neg_x: c.neg_x,
                pos_y: c.pos_y,
                neg_y: c.neg_y,
                pos_z: c.pos_z,
                neg_z: c.neg_z,
                specular: c.specular,
                fresnel_power: c.fresnel_power,
            }
        }),
    }
}
