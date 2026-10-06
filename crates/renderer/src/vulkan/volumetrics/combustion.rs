//! #5094 — combustion light moments, split out of the volumetrics driver
//! (regression of #2256): the constants, the moment decode and the
//! authored-source containment tests behind
//! `VolumetricsPipeline::append_combustion_surface_lights`.

use anyhow::Result;
use super::super::scene_buffer::GpuLight;
use crate::shader_constants::{
    ATTENUATION_MODEL_INVERSE_SQUARE, COMBUSTION_AEROSOL_LINGER_SECONDS, COMBUSTION_LIGHT_FIXED_SCALE,
    COMBUSTION_LIGHT_GRID_COUNT as GLSL_COMBUSTION_LIGHT_GRID_COUNT, COMBUSTION_LIGHT_GRID_X,
    COMBUSTION_LIGHT_GRID_Y, COMBUSTION_LIGHT_GRID_Z,
    COMBUSTION_LIGHT_HALF_EXTENT_XZ_METERS, COMBUSTION_LIGHT_HALF_EXTENT_Y_METERS,
    COMBUSTION_LIGHT_VOLUME_FIXED_SCALE,
    FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR, FOG_VOLUME_PROFILE_SMOKE, VISIBILITY_MASK_FULL,
};
use super::WORLD_UNITS_PER_METER;
use super::fog_clusters::GpuFogVolume;

/// Continue advancing transported smoke after the source entity disappears.
/// Core owns the matching aerosol decay/cutoff contract, avoiding a visible
/// tail cut when the renderer stops the otherwise-idle transport dispatch.
pub(crate) const COMBUSTION_HISTORY_LINGER_SECONDS: f32 = COMBUSTION_AEROSOL_LINGER_SECONDS;

pub(crate) const COMBUSTION_LIGHT_GRID_COUNT: usize = GLSL_COMBUSTION_LIGHT_GRID_COUNT as usize;

pub(crate) const MAX_COMBUSTION_SURFACE_LIGHTS: usize = 64;

pub(crate) const COMBUSTION_LIGHT_CUTOFF_IRRADIANCE: f32 = 1.0e-3;

pub(crate) const COMBUSTION_LIGHT_MIN_RANGE_METERS: f32 = 0.25;

pub(crate) const COMBUSTION_LIGHT_MAX_RANGE_METERS: f32 = 64.0;

/// `pointSpotAtten` treats half the uploaded cull radius as the physical
/// inverse-square reach and uses the second half as a smooth cull window.
pub(crate) const COMBUSTION_LIGHT_RANGE_EXTENSION: f32 = 2.0;

/// Art-directed gain for converting the transported fire field into a point
/// light. The moment is a froxel-integrated, escape-weighted source, while
/// the surface path evaluates it as one inverse-square emitter; without this
/// calibration the flame is visible but contributes almost no room light.
/// Keep this separate from the volumetric emission so fire visibility and
/// surface illumination can be tuned independently.
pub(crate) const COMBUSTION_SURFACE_LIGHT_BOOST: f32 = 4.0;

/// W2.13 (light & shadow campaign) — the restored oversized-reach canary.
/// The deleted `fire_lights.rs` canary gated a CPU-derived light; this one
/// guards the transported-field reduction. Vanilla torch LIGHs author a
/// ~512 BU radius; the derived reach is physics-computed
/// (`sqrt(luma / cutoff)` clamped to [`COMBUSTION_LIGHT_MAX_RANGE_METERS`],
/// times [`COMBUSTION_LIGHT_RANGE_EXTENSION`]), so an appended surface
/// light whose CULL radius exceeds 2× a vanilla torch is exactly the
/// "fires without a companion LIGH blowing out the room" failure the
/// ROADMAP's owed visual A/B exists to catch. Max possible:
/// 64 m × 2 × 70 BU/m = 8 960 BU, so the threshold is reachable.
pub(crate) const COMBUSTION_REACH_CANARY_CULL_RADIUS_BU: f32 = 1024.0;

/// A nuclear cloud deliberately creates more luminous bins than an oil
/// fireball. Keep each derived point-light contribution bounded while the
/// source is alive; the broad volume still lights the room through multiple
/// bins, but does not flatten every wall to white before the mushroom shape
/// can be read.
pub(crate) const NUCLEAR_COMBUSTION_SURFACE_LIGHT_SCALE: f32 = 0.22;

/// Fixed-point ABI mirrored by `CombustionLightMoment` in
/// `volumetrics_inject.comp` (std430: eight tightly packed uints).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuCombustionLightMoment {
    pub(crate) weight: u32,
    pub(crate) weighted_x: u32,
    pub(crate) weighted_y: u32,
    pub(crate) weighted_z: u32,
    pub(crate) radiant_r: u32,
    pub(crate) radiant_g: u32,
    pub(crate) radiant_b: u32,
    pub(crate) luminous_volume: u32,
}

pub(crate) fn decode_combustion_light_moment(bytes: &[u8]) -> GpuCombustionLightMoment {
    debug_assert!(bytes.len() >= std::mem::size_of::<GpuCombustionLightMoment>());
    let word = |index: usize| {
        let offset = index * std::mem::size_of::<u32>();
        u32::from_ne_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .expect("four-byte word"),
        )
    };
    GpuCombustionLightMoment {
        weight: word(0),
        weighted_x: word(1),
        weighted_y: word(2),
        weighted_z: word(3),
        radiant_r: word(4),
        radiant_g: word(5),
        radiant_b: word(6),
        luminous_volume: word(7),
    }
}

/// #4535 — run a latched drain step: the fallible body only executes while
/// the latch is set, and the latch is consumed only once the body returned
/// `Ok`. A failed drain must leave the slot latched (`combustion_moment_dirty`'s
/// documented invariant: "cleared only by a drain that actually zeroes") so
/// the next drain retries the zero instead of skipping it while the buffer
/// still holds un-drained moments — which the next inject pass would
/// `atomicAdd` on top of, surfacing one cycle of stale combustion light.
pub(crate) fn latched_drain<T>(latch: &mut bool, drain: impl FnOnce() -> Result<T>) -> Result<Option<T>> {
    if !*latch {
        return Ok(None);
    }
    let value = drain()?;
    *latch = false;
    Ok(Some(value))
}

pub(crate) fn combustion_light_from_moment(
    bin_index: usize,
    moment: GpuCombustionLightMoment,
    camera_world: [f32; 3],
) -> Option<GpuLight> {
    if moment.weight == 0 {
        return None;
    }
    let radiant = [
        moment.radiant_r as f32 / COMBUSTION_LIGHT_FIXED_SCALE,
        moment.radiant_g as f32 / COMBUSTION_LIGHT_FIXED_SCALE,
        moment.radiant_b as f32 / COMBUSTION_LIGHT_FIXED_SCALE,
    ]
    .map(|channel| channel * COMBUSTION_SURFACE_LIGHT_BOOST);
    let luma = byroredux_core::radiometry::linear_srgb_luminance(radiant);
    if !luma.is_finite() || luma < COMBUSTION_LIGHT_CUTOFF_IRRADIANCE {
        return None;
    }

    let grid_x = COMBUSTION_LIGHT_GRID_X as usize;
    let grid_y = COMBUSTION_LIGHT_GRID_Y as usize;
    let bin_x = bin_index % grid_x;
    let yz = bin_index / grid_x;
    let bin_y = yz % grid_y;
    let bin_z = yz / grid_y;
    if bin_z >= COMBUSTION_LIGHT_GRID_Z as usize {
        return None;
    }
    let inverse_weight = 1.0 / moment.weight as f32;
    let within = [
        (moment.weighted_x as f32 * inverse_weight).clamp(0.0, 1.0),
        (moment.weighted_y as f32 * inverse_weight).clamp(0.0, 1.0),
        (moment.weighted_z as f32 * inverse_weight).clamp(0.0, 1.0),
    ];
    let half = [
        COMBUSTION_LIGHT_HALF_EXTENT_XZ_METERS,
        COMBUSTION_LIGHT_HALF_EXTENT_Y_METERS,
        COMBUSTION_LIGHT_HALF_EXTENT_XZ_METERS,
    ];
    let dims = [
        COMBUSTION_LIGHT_GRID_X as f32,
        COMBUSTION_LIGHT_GRID_Y as f32,
        COMBUSTION_LIGHT_GRID_Z as f32,
    ];
    let bins = [bin_x as f32, bin_y as f32, bin_z as f32];
    let position: [f32; 3] = std::array::from_fn(|axis| {
        let offset_metres =
            -half[axis] + (bins[axis] + within[axis]) * 2.0 * half[axis] / dims[axis];
        camera_world[axis] + offset_metres * WORLD_UNITS_PER_METER
    });

    let luminous_volume = moment.luminous_volume as f32 / COMBUSTION_LIGHT_VOLUME_FIXED_SCALE;
    let source_radius_metres = if luminous_volume > 0.0 && luminous_volume.is_finite() {
        (3.0 * luminous_volume / (4.0 * std::f32::consts::PI))
            .cbrt()
            .clamp(0.02, 8.0)
    } else {
        0.02
    };
    let range_metres = (luma / COMBUSTION_LIGHT_CUTOFF_IRRADIANCE).sqrt().clamp(
        COMBUSTION_LIGHT_MIN_RANGE_METERS,
        COMBUSTION_LIGHT_MAX_RANGE_METERS,
    );

    Some(GpuLight {
        // Transported field samples have no persistent emitter identity, so
        // `append_combustion_surface_lights` pairs each appended light with
        // a [0; 4] entry in the collector's CPU-side identity vec (#5055).
        position_radius: [
            position[0],
            position[1],
            position[2],
            range_metres * WORLD_UNITS_PER_METER * COMBUSTION_LIGHT_RANGE_EXTENSION,
        ],
        color_type: [radiant[0], radiant[1], radiant[2], 0.0],
        direction_angle: [0.0; 4],
        params: [
            1.0,
            source_radius_metres * WORLD_UNITS_PER_METER,
            VISIBILITY_MASK_FULL as f32,
            ATTENUATION_MODEL_INVERSE_SQUARE as f32,
        ],
    })
}

pub(crate) fn volume_contains_position(volume: &GpuFogVolume, position: [f32; 3]) -> bool {
    let dx = position[0] - volume.center_shape[0];
    let dy = position[1] - volume.center_shape[1];
    let dz = position[2] - volume.center_shape[2];
    let radius_squared = volume.half_extents_extinction[0] * volume.half_extents_extinction[0]
        + volume.half_extents_extinction[1] * volume.half_extents_extinction[1]
        + volume.half_extents_extinction[2] * volume.half_extents_extinction[2];
    dx * dx + dy * dy + dz * dz <= radius_squared
}

pub(crate) fn combustion_light_is_inside_authored_source(
    light: &GpuLight,
    source_volumes: &[GpuFogVolume],
    authored_lights: &[GpuLight],
) -> bool {
    let light_position = [
        light.position_radius[0],
        light.position_radius[1],
        light.position_radius[2],
    ];
    source_volumes.iter().any(|volume| {
        let profile = volume.profile_params[0];
        let transported = profile.is_finite()
            && (FOG_VOLUME_PROFILE_SMOKE - 0.5..=FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR + 0.5)
                .contains(&profile);
        transported
            && volume_contains_position(volume, light_position)
            && authored_lights.iter().any(|candidate| {
                candidate.color_type[3] <= 1.5
                    && volume_contains_position(
                        volume,
                        [
                            candidate.position_radius[0],
                            candidate.position_radius[1],
                            candidate.position_radius[2],
                        ],
                    )
            })
    })
}
