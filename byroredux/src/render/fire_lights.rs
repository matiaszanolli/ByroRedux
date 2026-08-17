//! Derived light sources for emissive participating media.
//!
//! The froxel emission term (M55 / `volumetrics_inject.comp`) makes a flame
//! *visible*, but a visible flame still leaves the room dark: the froxel grid
//! contributes radiance along the view ray, not irradiance onto surfaces.
//! Bethesda papered over this by hand-placing a LIGH record beside every
//! torch. This module closes the loop physically instead — a medium that
//! emits light also *casts* it.
//!
//! # Why this operates on GPU volumes rather than the ECS
//!
//! By the time `collect_lights` runs, [`GpuFogVolume`] already holds every
//! quantity this derivation needs — absolute world-space center, half extents,
//! extinction, albedo and emitted radiance — after frustum culling and
//! transform composition. Re-querying the ECS would duplicate that work, take
//! another set of component locks, and risk disagreeing with what the froxel
//! pass will actually inject. Deriving from the same struct the GPU reads
//! keeps the light and the medium exactly consistent by construction.
//!
//! # Authored-light suppression
//!
//! Vanilla content already places a LIGH beside most fires. Emitting a derived
//! light there would double-count the illumination, so a fire whose extent
//! already contains an authored light is treated as *already lit* and
//! suppressed. The derived light is therefore additive only where the original
//! engine had nothing — fires without a companion LIGH and transient
//! fireballs, which by their nature cannot have hand-placed lights.

#[cfg(test)]
use byroredux_core::lighting::AttenuationModel;
use byroredux_core::lighting::{Emitter, Meters, RadiantIntensityRgb, VisibilityMask};
use byroredux_core::radiometry::linear_srgb_luminance;
use byroredux_renderer::{GpuFogVolume, GpuLight};

use super::lights::gpu_light_from_emitter;
#[cfg(test)]
use super::lights::LIGHT_RANGE_EXTENSION;

/// Cutoff used to size a derived light's reach.
///
/// The canonical inverse-square shader treats `GpuLight.color_type.rgb` as
/// radiant intensity (W·sr⁻¹) and physical distance in metres. This threshold
/// therefore has the matching scene-irradiance unit and determines only the
/// finite cluster/cull reach; it does not alter illumination inside the range.
const CUTOFF_IRRADIANCE: f32 = 1.0e-3;

/// Upper bound on a derived light's influence range, metres.
///
/// A large, hot volume can otherwise derive a reach that spans an entire
/// worldspace, which would defeat the clustered-light binning the froxel and
/// fragment passes both rely on. Matches the scale of the largest radii
/// vanilla content authors.
const MAX_DERIVED_RANGE_METERS: f32 = 4096.0 / byroredux_core::lighting::BETHESDA_UNITS_PER_METER;

/// Minimum useful range, metres. Below this a light influences nothing
/// but its own froxel and is not worth a cluster slot.
const MIN_DERIVED_RANGE_METERS: f32 = 16.0 / byroredux_core::lighting::BETHESDA_UNITS_PER_METER;

/// Append one derived point light per emissive volume that is not already
/// served by an authored light.
///
/// Must run BEFORE `collect_lights`' GI-priority sort: fires are bright and
/// belong in the shader's `GI_HIT_LIGHT_CAP` prefix on merit, and appending
/// after the sort would strand them at the tail of the array regardless of how
/// much they matter.
pub(super) fn append_fire_lights(fog_volumes: &[GpuFogVolume], gpu_lights: &mut Vec<GpuLight>) {
    append_derived_lights(fog_volumes, gpu_lights);
}

fn append_derived_lights(fog_volumes: &[GpuFogVolume], gpu_lights: &mut Vec<GpuLight>) {
    let authored_count = gpu_lights.len();
    for volume in fog_volumes {
        let Some(light) = derive_fire_light(volume) else {
            continue;
        };
        if is_already_lit(volume, &gpu_lights[..authored_count]) {
            continue;
        }
        gpu_lights.push(light);
    }
}

/// Whether an authored light already sits inside this volume's extent.
///
/// The test is containment in the volume's own bounding sphere rather than a
/// fixed distance: a candle and a bonfire have very different ideas of "beside
/// the flame", and the volume's extent is exactly that scale.
fn is_already_lit(volume: &GpuFogVolume, authored: &[GpuLight]) -> bool {
    let center = center_of(volume);
    let radius = bounding_radius(volume);
    authored.iter().any(|light| {
        // Directional lights (type 2) are infinite and have no position, so
        // they can never be "the light belonging to this fire".
        if light.color_type[3] > 1.5 {
            return false;
        }
        let delta = [
            light.position_radius[0] - center[0],
            light.position_radius[1] - center[1],
            light.position_radius[2] - center[2],
        ];
        let distance_squared = delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2];
        distance_squared <= radius * radius
    })
}

fn center_of(volume: &GpuFogVolume) -> [f32; 3] {
    [
        volume.center_shape[0],
        volume.center_shape[1],
        volume.center_shape[2],
    ]
}

fn bounding_radius(volume: &GpuFogVolume) -> f32 {
    let [x, y, z] = [
        volume.half_extents_extinction[0],
        volume.half_extents_extinction[1],
        volume.half_extents_extinction[2],
    ];
    (x * x + y * y + z * z).sqrt()
}

/// Derive a point light from one emissive volume, or `None` if the volume is
/// passive, degenerate, or too dim to matter.
///
/// The chain is:
///
/// 1. **Emergent radiance.** A homogeneous slab of absorption optical depth
///    `tau_a` with internal source `L_e` emits `L_e * (1 - exp(-tau_a))`. This
///    is the exact solution of the radiative transfer equation for such a
///    slab, and it saturates correctly: a very thick flame approaches `L_e`
///    rather than growing without bound as a naive `sigma_a * L_e * d` would.
/// 2. **Radiant intensity.** `I = L * A`, using projected area in square
///    metres (never Bethesda units squared) so the result matches the
///    canonical [`RadiantIntensityRgb`] contract. An ellipsoid's area is
///    view-dependent; the mean is the isotropic proxy, which is appropriate
///    because a point light has no orientation to carry the anisotropy anyway.
/// 3. **Reach.** Inverse-square down to [`CUTOFF_IRRADIANCE`]. The engine then
///    applies its own `pointSpotAtten` curve within that radius — this step
///    sizes the influence sphere, it does not replace the falloff model.
fn derive_fire_light(volume: &GpuFogVolume) -> Option<GpuLight> {
    let (center, emitter) = derive_fire_emitter(volume)?;
    Some(gpu_light_from_emitter(center, emitter, 1.0))
}

fn derive_fire_emitter(volume: &GpuFogVolume) -> Option<([f32; 3], Emitter)> {
    let emission = [
        volume.emission_temperature[0],
        volume.emission_temperature[1],
        volume.emission_temperature[2],
    ];
    if !emission.iter().all(|c| c.is_finite()) || emission.iter().all(|c| *c <= 0.0) {
        return None;
    }

    let half_extents = [
        volume.half_extents_extinction[0],
        volume.half_extents_extinction[1],
        volume.half_extents_extinction[2],
    ];
    if !half_extents.iter().all(|e| e.is_finite() && *e > 0.0) {
        return None;
    }
    let mean_half_extent = (half_extents[0] + half_extents[1] + half_extents[2]) / 3.0;

    let sigma_t = volume.half_extents_extinction[3];
    if !sigma_t.is_finite() || sigma_t <= 0.0 {
        return None;
    }

    // Absorption fraction per channel; emission is driven by sigma_a, not
    // sigma_t, exactly as in `volumetrics_inject.comp`.
    let albedo = [
        volume.albedo_edge[0].clamp(0.0, 1.0),
        volume.albedo_edge[1].clamp(0.0, 1.0),
        volume.albedo_edge[2].clamp(0.0, 1.0),
    ];
    // Chord through the volume, world units.
    let path_length = 2.0 * mean_half_extent;
    let mut emergent = [0.0f32; 3];
    for channel in 0..3 {
        let optical_depth = sigma_t * (1.0 - albedo[channel]) * path_length;
        emergent[channel] = emission[channel] * (1.0 - (-optical_depth).exp());
    }

    let luminance = linear_srgb_luminance(emergent);
    if !luminance.is_finite() || luminance <= 0.0 {
        return None;
    }

    // Radiant intensity of a sphere of this size at this surface radiance.
    // The GPU attenuation law converts its world-space distance to metres, so
    // this area must be m² and the uploaded RGB must be I = L*A, not bare L.
    let source_radius = Meters::from_bethesda_units(mean_half_extent);
    let projected_area_m2 = std::f32::consts::PI * source_radius.get().powi(2);
    let radiant_intensity = emergent.map(|channel| channel * projected_area_m2);
    let intensity_luminance = linear_srgb_luminance(radiant_intensity);
    let range_meters = (intensity_luminance / CUTOFF_IRRADIANCE).sqrt();
    if !range_meters.is_finite() || range_meters < MIN_DERIVED_RANGE_METERS {
        return None;
    }
    let range = Meters::new(range_meters.min(MAX_DERIVED_RANGE_METERS));

    let center = center_of(volume);
    let emitter = Emitter::inverse_square_point(
        RadiantIntensityRgb::new(radiant_intensity),
        range,
        source_radius,
        VisibilityMask::FULL,
    );
    Some((center, emitter))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A torch-scale flame: ~5-unit half extents, optical depth 0.4 across
    /// its width, soot albedo 0.25, emitted radiance from the 1850 K anchor.
    fn torch_volume() -> GpuFogVolume {
        let half_extent = 5.0_f32;
        // Mirrors `fog::fire_volume_from_particle`: sigma_t is chosen to give
        // FLAME_OPTICAL_DEPTH across the primitive width, expressed per world
        // unit as the GPU struct stores it.
        let sigma_t = 0.4 / (2.0 * half_extent);
        GpuFogVolume {
            center_shape: [0.0, 0.0, 0.0, 1.0],
            half_extents_extinction: [half_extent, half_extent, half_extent, sigma_t],
            inverse_rotation: [0.0, 0.0, 0.0, 1.0],
            albedo_edge: [0.25, 0.25, 0.25, 0.45],
            emission_temperature: [12.0, 6.5, 2.0, 1850.0],
            profile_params: [2.0, 0.0, 0.0, 0.0],
        }
    }

    fn passive_volume() -> GpuFogVolume {
        GpuFogVolume {
            emission_temperature: [0.0; 4],
            ..torch_volume()
        }
    }

    #[test]
    fn passive_media_derive_no_light() {
        assert!(derive_fire_light(&passive_volume()).is_none());
        let mut lights = Vec::new();
        append_derived_lights(&[passive_volume()], &mut lights);
        assert!(
            lights.is_empty(),
            "ordinary fog and smoke must not become light sources"
        );
    }

    #[test]
    fn derived_light_uses_metre_area_and_radiant_intensity() {
        let volume = torch_volume();
        let (_, emitter) = derive_fire_emitter(&volume).expect("torch flame is emissive");
        let intensity = emitter.radiant_intensity.get();

        // A 5-world-unit source has a 7.14 cm physical radius. Its projected
        // area is ~0.016 m², so the uploaded intensity must be much smaller
        // than the source radiance. Treating 5 as metres (or uploading bare
        // radiance) fails this by orders of magnitude.
        let source_radius_m = 5.0 / byroredux_core::lighting::BETHESDA_UNITS_PER_METER;
        let area_m2 = std::f32::consts::PI * source_radius_m.powi(2);
        assert!((emitter.source_radius.get() - source_radius_m).abs() < 1.0e-6);
        for (channel, source_radiance) in intensity
            .iter()
            .zip(volume.emission_temperature[..3].iter())
        {
            assert!(*channel > 0.0);
            assert!(*channel < *source_radiance * area_m2);
        }

        // The finite range is physical metres, converted back to world units
        // only by the shared renderer boundary.
        assert!(emitter.range.get() > 5.0 && emitter.range.get() < 6.0);
        let light = derive_fire_light(&volume).expect("emissive");
        let range_world = light.position_radius[3] / LIGHT_RANGE_EXTENSION;
        assert!(
            (range_world - emitter.range.to_bethesda_units()).abs() < 1.0e-4,
            "GPU packing must perform exactly one metres -> world conversion"
        );
        assert_eq!(&light.color_type[..3], &intensity);
        assert_eq!(
            light.params[3],
            AttenuationModel::InverseSquare as u8 as f32
        );
    }

    #[test]
    fn derived_lights_are_enabled_without_a_runtime_feature_gate() {
        let mut lights = Vec::new();
        append_fire_lights(&[torch_volume()], &mut lights);
        assert_eq!(lights.len(), 1, "emissive media must light nearby surfaces");
    }

    /// A fire is warm-coloured; the derived light must carry that through
    /// rather than washing out to white.
    #[test]
    fn derived_light_keeps_blackbody_chromaticity() {
        let light = derive_fire_light(&torch_volume()).expect("emissive");
        assert!(
            light.color_type[0] > light.color_type[1] && light.color_type[1] > light.color_type[2],
            "derived flame light must stay R > G > B, got {:?}",
            &light.color_type[..3]
        );
        assert_eq!(
            light.color_type[3], 0.0,
            "derived fire lights are point lights"
        );
    }

    /// Emergent radiance must saturate at `L_e`, not grow without bound with
    /// size. A naive `sigma_a * L_e * path` model would fail this.
    #[test]
    fn emergent_radiance_saturates_for_optically_thick_media() {
        let mut thick = torch_volume();
        thick.half_extents_extinction[3] = 100.0; // absurdly dense
        let (_, emitter) = derive_fire_emitter(&thick).expect("emissive");
        let intensity = emitter.radiant_intensity.get();
        let area_m2 = std::f32::consts::PI * emitter.source_radius.get().powi(2);
        for channel in 0..3 {
            assert!(
                intensity[channel] / area_m2 <= thick.emission_temperature[channel] + 1.0e-3,
                "channel {channel} exceeded the source radiance after removing area: {} > {}",
                intensity[channel] / area_m2,
                thick.emission_temperature[channel]
            );
        }
    }

    /// The double-count guard: a fire with an authored light inside its own
    /// extent must not add a second one.
    #[test]
    fn authored_light_inside_the_flame_suppresses_the_derived_one() {
        let volume = torch_volume();
        let authored = GpuLight {
            position_radius: [1.0, 1.0, 1.0, 512.0],
            color_type: [1.0, 0.8, 0.6, 0.0],
            direction_angle: [0.0; 4],
            params: [
                1.0,
                1.0,
                VisibilityMask::FULL.bits() as f32,
                AttenuationModel::LegacySoftRange as u8 as f32,
            ],
        };
        let mut lights = vec![authored];
        append_derived_lights(&[volume], &mut lights);
        assert_eq!(
            lights.len(),
            1,
            "an authored LIGH inside the flame already represents it"
        );
    }

    /// ...but a light across the room does not suppress it.
    #[test]
    fn distant_authored_light_does_not_suppress_the_derived_one() {
        let volume = torch_volume();
        let distant = GpuLight {
            position_radius: [500.0, 0.0, 0.0, 512.0],
            color_type: [1.0, 0.8, 0.6, 0.0],
            direction_angle: [0.0; 4],
            params: [
                1.0,
                1.0,
                VisibilityMask::FULL.bits() as f32,
                AttenuationModel::LegacySoftRange as u8 as f32,
            ],
        };
        let mut lights = vec![distant];
        append_derived_lights(&[volume], &mut lights);
        assert_eq!(
            lights.len(),
            2,
            "a fire with no companion LIGH must light itself"
        );
    }

    /// A directional light has no position and can never be the fire's own
    /// companion light, however close its zero-origin happens to fall.
    #[test]
    fn directional_light_never_suppresses_a_fire() {
        let volume = torch_volume();
        let sun = GpuLight {
            position_radius: [0.0, 0.0, 0.0, -1.0],
            color_type: [1.0, 1.0, 1.0, 2.0],
            direction_angle: [0.0, -1.0, 0.0, 0.0],
            params: [
                0.0,
                0.0,
                VisibilityMask::FULL.bits() as f32,
                AttenuationModel::LegacySoftRange as u8 as f32,
            ],
        };
        let mut lights = vec![sun];
        append_derived_lights(&[volume], &mut lights);
        assert_eq!(
            lights.len(),
            2,
            "the sun sits at the origin in light space and must not be mistaken \
             for a torch's companion light"
        );
    }

    /// Derived lights must not suppress each other — only authored ones
    /// participate in the containment test.
    #[test]
    fn co_located_fires_do_not_suppress_each_other() {
        let volume = torch_volume();
        let mut lights = Vec::new();
        append_derived_lights(&[volume, volume], &mut lights);
        assert_eq!(
            lights.len(),
            2,
            "the suppression test must only consider lights present before the pass"
        );
    }

    /// Degenerate input must be rejected rather than uploaded as a NaN light.
    #[test]
    fn degenerate_volumes_are_rejected() {
        let mut nan_emission = torch_volume();
        nan_emission.emission_temperature[0] = f32::NAN;
        assert!(derive_fire_light(&nan_emission).is_none());

        let mut zero_extent = torch_volume();
        zero_extent.half_extents_extinction[1] = 0.0;
        assert!(derive_fire_light(&zero_extent).is_none());

        let mut zero_sigma = torch_volume();
        zero_sigma.half_extents_extinction[3] = 0.0;
        assert!(derive_fire_light(&zero_sigma).is_none());

        // Emissive but far too dim to reach anything.
        let mut faint = torch_volume();
        faint.emission_temperature = [1.0e-6, 1.0e-6, 1.0e-6, 1850.0];
        assert!(derive_fire_light(&faint).is_none());
    }
}
