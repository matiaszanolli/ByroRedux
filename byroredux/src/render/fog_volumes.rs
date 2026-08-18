//! ECS local fog-volume collection and GPU translation.

use byroredux_core::combustion::AEROSOL_LINGER_SECONDS;
use byroredux_core::ecs::{
    CombustionState, FogProfile, FogShape, FogVolume, GlobalTransform, TotalTime, World,
};
use byroredux_core::math::Vec3;
use byroredux_core::radiometry::{blackbody_chromaticity_srgb, linear_srgb_luminance};
use byroredux_renderer::vulkan::volumetrics::{
    GpuFogVolume, FOG_VOLUME_PROFILE_EXPLOSION, FOG_VOLUME_PROFILE_FLAME,
    FOG_VOLUME_PROFILE_HOMOGENEOUS, FOG_VOLUME_PROFILE_SMOKE, MAX_GPU_FOG_VOLUMES,
    WORLD_UNITS_PER_METER,
};

use super::camera::FrustumPlanes;

pub(super) fn collect_fog_volumes(
    world: &World,
    frustum: &FrustumPlanes,
    camera_pos: Vec3,
    out: &mut Vec<GpuFogVolume>,
) {
    out.clear();
    let Some(volume_query) = world.query::<FogVolume>() else {
        return;
    };
    let Some(transform_query) = world.query::<GlobalTransform>() else {
        return;
    };
    let combustion_query = world.query::<CombustionState>();
    let now_seconds = world.resource::<TotalTime>().0;

    for (entity, volume) in volume_query.iter() {
        let Some(transform) = transform_query.get(entity) else {
            continue;
        };
        let combustion = combustion_query
            .as_ref()
            .and_then(|query| query.get(entity))
            .copied();
        let mut render_volume = *volume;
        let explosion_age = if volume.profile == FogProfile::Explosion {
            match combustion {
                Some(state) => match state.normalized_age(now_seconds) {
                    Some(age) => Some((age, state.lifetime_seconds)),
                    None => {
                        if !explosion_smoke_linger_active(now_seconds, state) {
                            continue;
                        }
                        // The authored explosion has ended, but its soot
                        // remains a passive source boundary long enough for
                        // the canonical transported field to entrain and
                        // disperse it. No emission or hot temperature leaks
                        // into this phase.
                        render_volume.profile = FogProfile::Smoke;
                        render_volume.emissive_radiance = [0.0; 3];
                        render_volume.emission_temperature_k = 0.0;
                        None
                    }
                },
                // An explicitly authored Explosion without a timeline still
                // renders its initial state. All particle-conversion paths
                // attach `CombustionState`, so this is principally a safe
                // editor/debug default rather than a looping runtime effect.
                None => Some((0.0, 0.0)),
            }
        } else {
            None
        };
        let Some(gpu) =
            gpu_volume_from_ecs_with_explosion_age(render_volume, *transform, explosion_age)
        else {
            continue;
        };
        let center = Vec3::new(
            gpu.center_shape[0],
            gpu.center_shape[1],
            gpu.center_shape[2],
        );
        let extents = Vec3::new(
            gpu.half_extents_extinction[0],
            gpu.half_extents_extinction[1],
            gpu.half_extents_extinction[2],
        );
        if frustum.contains_sphere(center, extents.length()) {
            out.push(gpu);
        }
    }

    // Cluster overflow retains earlier entries. Near-to-far ordering therefore
    // makes the bounded list prefer media most likely to affect the camera.
    // #2680 / PERF-D1-02 — unstable sort: the stable one allocates a
    // volume-count-sized temporary on every frame that collects any media.
    out.sort_unstable_by(|a, b| {
        let distance_squared = |volume: &GpuFogVolume| {
            Vec3::new(
                volume.center_shape[0],
                volume.center_shape[1],
                volume.center_shape[2],
            )
            .distance_squared(camera_pos)
        };
        distance_squared(a).total_cmp(&distance_squared(b))
    });
    out.truncate(MAX_GPU_FOG_VOLUMES);
}

fn explosion_smoke_linger_active(now_seconds: f32, state: CombustionState) -> bool {
    let elapsed = now_seconds - state.start_time_seconds;
    elapsed.is_finite()
        && elapsed >= state.lifetime_seconds
        && elapsed < state.lifetime_seconds + AEROSOL_LINGER_SECONDS
}

#[cfg(test)]
fn gpu_volume_from_ecs(volume: FogVolume, transform: GlobalTransform) -> Option<GpuFogVolume> {
    gpu_volume_from_ecs_with_explosion_age(volume, transform, None)
}

fn gpu_volume_from_ecs_with_explosion_age(
    volume: FogVolume,
    transform: GlobalTransform,
    explosion_age: Option<(f32, f32)>,
) -> Option<GpuFogVolume> {
    if !volume.is_renderable() {
        return None;
    }
    let bounds = volume.bounds?;
    let scale = transform.scale.abs();
    if !scale.is_finite() || scale <= 1.0e-6 {
        return None;
    }
    let half_extents = bounds.half_extents.abs() * scale;
    if !half_extents.is_finite() || half_extents.min_element() <= 1.0e-6 {
        return None;
    }
    let rotation = transform.rotation * bounds.rotation;
    if !rotation.is_finite() || rotation.length_squared() <= 1.0e-8 {
        return None;
    }
    let rotation = rotation.normalize();
    let center = transform.translation + transform.rotation * (bounds.center * scale);
    if !center.is_finite() {
        return None;
    }
    // #2235 (REN-D10-01) — fog volumes are a new absolute-world-space GPU
    // consumer: `GpuFogVolume::center_shape` is authored/uploaded/sampled in
    // absolute coordinates (`volumetrics_inject.comp` subtracts it directly
    // from the absolute `world_pos`, unlike the render-origin-relative
    // raster path). It therefore inherits the same RT absolute-space f32
    // precision ceiling every other absolute-space consumer is guarded
    // against — see `RT_ABSOLUTE_PRECISION_CEILING` / #1495 and
    // docs/engine/shader-pipeline.md "Coordinate Spaces & Precision" ("Any
    // future absolute-space shader consumer inherits this same ceiling").
    // Never fires on vanilla content; catches a fog volume authored (or a
    // render_origin rebase bug) far enough from the origin to silently
    // degrade into visible jitter/banding.
    debug_assert!(
        center.abs().max_element() < crate::cell_loader::references::RT_ABSOLUTE_PRECISION_CEILING,
        "fog volume center {center:?} reaches the RT absolute-space f32 \
         precision ceiling ({:.0} u): see #1495 / #2235 / \
         docs/engine/shader-pipeline.md.",
        crate::cell_loader::references::RT_ABSOLUTE_PRECISION_CEILING,
    );

    let shape = match bounds.shape {
        FogShape::Sphere => 0.0,
        FogShape::Ellipsoid => 1.0,
        FogShape::Box => 2.0,
    };
    let inverse_rotation = rotation.conjugate();
    let profile = match volume.profile {
        FogProfile::Homogeneous => FOG_VOLUME_PROFILE_HOMOGENEOUS,
        FogProfile::Smoke => FOG_VOLUME_PROFILE_SMOKE,
        FogProfile::Flame => FOG_VOLUME_PROFILE_FLAME,
        FogProfile::Explosion => FOG_VOLUME_PROFILE_EXPLOSION,
    };
    let base_emission = volume.emissive_radiance.map(sanitize_emission);
    let base_temperature = volume.emission_temperature_k.max(0.0);
    let (emission, emission_temperature) = if profile == FOG_VOLUME_PROFILE_EXPLOSION {
        let age = explosion_age.map_or(0.0, |(age, _)| age);
        explosion_emission_state(base_emission, base_temperature, age)
    } else {
        (base_emission, base_temperature)
    };
    Some(GpuFogVolume {
        center_shape: [center.x, center.y, center.z, shape],
        half_extents_extinction: [
            half_extents.x,
            half_extents.y,
            half_extents.z,
            volume.extinction_per_meter / WORLD_UNITS_PER_METER,
        ],
        inverse_rotation: inverse_rotation.to_array(),
        albedo_edge: [
            volume.single_scatter_albedo[0].clamp(0.0, 1.0),
            volume.single_scatter_albedo[1].clamp(0.0, 1.0),
            volume.single_scatter_albedo[2].clamp(0.0, 1.0),
            volume.edge_softness.clamp(0.0, 1.0),
        ],
        // Emitted radiance carries no world-unit conversion. Explosion
        // temperature, chromaticity, and energy evolve here once so both the
        // froxel injector and the derived surface light consume the same
        // instantaneous source state; the shader adds only local turbulence.
        //
        // Negative or non-finite emission is clamped away rather than
        // rejected, so a bad authored value dims a flame instead of poisoning
        // the froxel accumulation buffer with NaN.
        emission_temperature: [emission[0], emission[1], emission[2], emission_temperature],
        profile_params: if profile == FOG_VOLUME_PROFILE_EXPLOSION {
            let (age, lifetime) = explosion_age.unwrap_or((0.0, 0.0));
            [profile, age.clamp(0.0, 1.0), lifetime.max(0.0), 0.0]
        } else {
            [profile, 0.0, 0.0, 0.0]
        },
    })
}

/// Resolve the global emission state of an explosion at one normalized age.
///
/// This is deliberately CPU-side at the ECS -> renderer boundary. The same
/// aged `GpuFogVolume` feeds both volumetric injection and derived point-light
/// integration, so the room illumination cannot remain at peak intensity
/// after the visible fireball has cooled into smoke.
fn explosion_emission_state(
    base_emission: [f32; 3],
    source_temperature_k: f32,
    age: f32,
) -> ([f32; 3], f32) {
    const COOLING_START_AGE: f32 = 0.06;
    const COOLING_END_AGE: f32 = 0.78;
    const VISIBLE_FIRE_START_AGE: f32 = 0.18;
    const VISIBLE_FIRE_END_AGE: f32 = 0.66;
    const COOLED_SMOKE_TEMPERATURE_K: f32 = 850.0;

    let age = age.clamp(0.0, 1.0);
    let source_temperature = if source_temperature_k.is_finite() {
        source_temperature_k.clamp(COOLED_SMOKE_TEMPERATURE_K, 4200.0)
    } else {
        COOLED_SMOKE_TEMPERATURE_K
    };
    let cooling = smoothstep(COOLING_START_AGE, COOLING_END_AGE, age);
    let temperature = (source_temperature
        + (COOLED_SMOKE_TEMPERATURE_K - source_temperature) * cooling)
        .clamp(700.0, 4200.0);
    let visible_fire = 1.0 - smoothstep(VISIBLE_FIRE_START_AGE, VISIBLE_FIRE_END_AGE, age);
    let thermal_energy = (temperature / source_temperature).powi(4);
    let luminance = linear_srgb_luminance(base_emission) * thermal_energy * visible_fire;
    let emission = blackbody_chromaticity_srgb(temperature)
        .map(|chromaticity| {
            // Gamut clipping in the core spectral conversion can move the
            // resulting Rec.709 luminance slightly away from one. Normalize
            // after clipping so this boundary preserves the intended energy
            // envelope exactly while retaining its blackbody hue.
            let chromaticity_luminance = linear_srgb_luminance(chromaticity).max(1.0e-6);
            chromaticity.map(|channel| channel * luminance / chromaticity_luminance)
        })
        .unwrap_or([0.0; 3]);
    (emission, temperature)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Clamp one emitted-radiance channel into a finite, non-negative value.
fn sanitize_emission(radiance: f32) -> f32 {
    if radiance.is_finite() {
        radiance.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::{FogBounds, FogSource};
    use byroredux_core::math::Quat;

    #[test]
    fn ecs_translation_preserves_optical_depth_under_world_unit_conversion() {
        let volume = FogVolume {
            bounds: Some(FogBounds {
                center: Vec3::new(0.0, 2.0, 0.0),
                rotation: Quat::IDENTITY,
                half_extents: Vec3::new(3.0, 4.0, 5.0),
                shape: FogShape::Ellipsoid,
            }),
            extinction_per_meter: 0.7,
            single_scatter_albedo: [0.9, 0.8, 0.7],
            edge_softness: 0.4,
            profile: FogProfile::Smoke,
            emissive_radiance: [0.0; 3],
            emission_temperature_k: 0.0,
            source: FogSource::ParticleEmitter,
        };
        let transform = GlobalTransform::new(Vec3::new(10.0, 20.0, 30.0), Quat::IDENTITY, 2.0);
        let gpu = gpu_volume_from_ecs(volume, transform).unwrap();
        assert_eq!(gpu.center_shape[..3], [10.0, 24.0, 30.0]);
        assert_eq!(gpu.half_extents_extinction[..3], [6.0, 8.0, 10.0]);
        assert!((gpu.half_extents_extinction[3] * WORLD_UNITS_PER_METER - 0.7).abs() < 1.0e-6);
        assert_eq!(gpu.profile_params[0], FOG_VOLUME_PROFILE_SMOKE);
    }

    #[test]
    fn canonical_profile_drives_gpu_behavior_independent_of_provenance() {
        let make_volume = |source, profile, emissive_radiance| FogVolume {
            bounds: Some(FogBounds {
                center: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                half_extents: Vec3::splat(4.0),
                shape: FogShape::Ellipsoid,
            }),
            extinction_per_meter: 0.7,
            single_scatter_albedo: [0.8; 3],
            edge_softness: 0.4,
            profile,
            emissive_radiance,
            emission_temperature_k: if emissive_radiance == [0.0; 3] {
                0.0
            } else {
                1850.0
            },
            source,
        };

        let authored = gpu_volume_from_ecs(
            make_volume(FogSource::AuthoredMesh, FogProfile::Homogeneous, [0.0; 3]),
            GlobalTransform::IDENTITY,
        )
        .unwrap();
        let smoke = gpu_volume_from_ecs(
            make_volume(FogSource::ParticleEmitter, FogProfile::Smoke, [0.0; 3]),
            GlobalTransform::IDENTITY,
        )
        .unwrap();
        let flame = gpu_volume_from_ecs(
            make_volume(
                FogSource::ParticleEmitter,
                FogProfile::Flame,
                [8.0, 3.0, 0.5],
            ),
            GlobalTransform::IDENTITY,
        )
        .unwrap();
        let passive_runtime_flame = gpu_volume_from_ecs(
            make_volume(FogSource::RuntimeEffect, FogProfile::Flame, [0.0; 3]),
            GlobalTransform::IDENTITY,
        )
        .unwrap();
        let explosion = gpu_volume_from_ecs_with_explosion_age(
            make_volume(
                FogSource::ParticleEmitter,
                FogProfile::Explosion,
                [24.0, 12.0, 2.0],
            ),
            GlobalTransform::IDENTITY,
            Some((0.375, 2.4)),
        )
        .unwrap();

        assert_eq!(authored.profile_params[0], FOG_VOLUME_PROFILE_HOMOGENEOUS);
        assert_eq!(smoke.profile_params[0], FOG_VOLUME_PROFILE_SMOKE);
        assert_eq!(flame.profile_params[0], FOG_VOLUME_PROFILE_FLAME);
        assert_eq!(
            passive_runtime_flame.profile_params[0], FOG_VOLUME_PROFILE_FLAME,
            "runtime behavior must come from the canonical profile, not be re-inferred from \
             provenance or current emission"
        );
        assert_eq!(
            explosion.profile_params,
            [FOG_VOLUME_PROFILE_EXPLOSION, 0.375, 2.4, 0.0]
        );
        assert!(
            linear_srgb_luminance(explosion.emission_temperature[..3].try_into().unwrap())
                < linear_srgb_luminance([24.0, 12.0, 2.0]),
            "an explosion partway through its lifetime must have cooled"
        );
        assert!(explosion.emission_temperature[3] < 1850.0);
    }

    #[test]
    fn explosion_emission_cools_and_extinguishes_once_for_all_consumers() {
        let base = [24.0, 12.0, 2.0];
        let base_luminance = linear_srgb_luminance(base);
        let (hot, hot_temperature) = explosion_emission_state(base, 2800.0, 0.0);
        let (cooling, cooling_temperature) = explosion_emission_state(base, 2800.0, 0.45);
        let (smoke, smoke_temperature) = explosion_emission_state(base, 2800.0, 0.70);

        assert!((linear_srgb_luminance(hot) - base_luminance).abs() < 1.0e-4);
        assert_eq!(hot_temperature, 2800.0);
        assert!(cooling_temperature < hot_temperature);
        assert!(linear_srgb_luminance(cooling) < base_luminance * 0.1);
        assert_eq!(smoke, [0.0; 3]);
        assert!(smoke_temperature <= cooling_temperature);

        // Cooling shifts blackbody chromaticity toward red before the fire
        // envelope reaches zero.
        assert!(cooling[1] / cooling[0] < hot[1] / hot[0]);
    }

    #[test]
    fn expired_explosion_lingers_as_smoke_only_within_canonical_window() {
        let state = CombustionState::one_shot(10.0, 8.0);
        assert!(!explosion_smoke_linger_active(17.9, state));
        assert!(explosion_smoke_linger_active(18.0, state));
        assert!(explosion_smoke_linger_active(
            18.0 + AEROSOL_LINGER_SECONDS - 0.001,
            state
        ));
        assert!(!explosion_smoke_linger_active(
            18.0 + AEROSOL_LINGER_SECONDS,
            state
        ));
    }

    #[test]
    fn invalid_or_cell_scope_volume_is_skipped() {
        let volume = FogVolume {
            bounds: None,
            extinction_per_meter: 0.5,
            single_scatter_albedo: [0.9; 3],
            edge_softness: 0.4,
            profile: FogProfile::Homogeneous,
            emissive_radiance: [0.0; 3],
            emission_temperature_k: 0.0,
            source: FogSource::Xcll,
        };
        assert!(gpu_volume_from_ecs(volume, GlobalTransform::IDENTITY).is_none());
    }

    /// Regression for #2235 (REN-D10-01): a fog volume authored (or
    /// rebased) far enough from the origin to reach the RT absolute-space
    /// f32 precision ceiling must fail loud in debug builds, matching every
    /// other absolute-space consumer guarded by
    /// `RT_ABSOLUTE_PRECISION_CEILING` (#1495).
    #[test]
    #[cfg(debug_assertions)]
    fn center_beyond_rt_precision_ceiling_asserts() {
        let volume = FogVolume {
            bounds: Some(FogBounds {
                center: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                half_extents: Vec3::splat(3.0),
                shape: FogShape::Sphere,
            }),
            extinction_per_meter: 0.5,
            single_scatter_albedo: [0.9; 3],
            edge_softness: 0.4,
            profile: FogProfile::Smoke,
            emissive_radiance: [0.0; 3],
            emission_temperature_k: 0.0,
            source: FogSource::ParticleEmitter,
        };
        let far_translation =
            Vec3::splat(crate::cell_loader::references::RT_ABSOLUTE_PRECISION_CEILING * 2.0);
        let transform = GlobalTransform::new(far_translation, Quat::IDENTITY, 1.0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            gpu_volume_from_ecs(volume, transform)
        }));
        assert!(
            result.is_err(),
            "fog volume beyond the RT absolute-space precision ceiling must debug_assert"
        );
    }
}
