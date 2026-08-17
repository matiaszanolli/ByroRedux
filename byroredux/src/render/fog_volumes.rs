//! ECS local fog-volume collection and GPU translation.

use byroredux_core::ecs::{FogShape, FogSource, FogVolume, GlobalTransform, World};
use byroredux_core::math::Vec3;
use byroredux_renderer::vulkan::volumetrics::{
    GpuFogVolume, FOG_VOLUME_PROFILE_FLAME, FOG_VOLUME_PROFILE_HOMOGENEOUS,
    FOG_VOLUME_PROFILE_SMOKE, MAX_GPU_FOG_VOLUMES, WORLD_UNITS_PER_METER,
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

    for (entity, volume) in volume_query.iter() {
        let Some(transform) = transform_query.get(entity) else {
            continue;
        };
        let Some(gpu) = gpu_volume_from_ecs(*volume, *transform) else {
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

fn gpu_volume_from_ecs(volume: FogVolume, transform: GlobalTransform) -> Option<GpuFogVolume> {
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
    let profile = match (volume.source, volume.is_emissive()) {
        (FogSource::ParticleEmitter, true) => FOG_VOLUME_PROFILE_FLAME,
        (FogSource::ParticleEmitter, false) => FOG_VOLUME_PROFILE_SMOKE,
        _ => FOG_VOLUME_PROFILE_HOMOGENEOUS,
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
        // Emitted radiance passes through unscaled: unlike extinction it is a
        // radiance, not a per-length coefficient, so it carries no world-unit
        // conversion. The shader multiplies it by the locally evaluated
        // `sigma_a` — which IS per world unit — to form the source term.
        //
        // Negative or non-finite emission is clamped away rather than
        // rejected, so a bad authored value dims a flame instead of poisoning
        // the froxel accumulation buffer with NaN.
        emission_temperature: [
            sanitize_emission(volume.emissive_radiance[0]),
            sanitize_emission(volume.emissive_radiance[1]),
            sanitize_emission(volume.emissive_radiance[2]),
            volume.emission_temperature_k.max(0.0),
        ],
        profile_params: [profile, 0.0, 0.0, 0.0],
    })
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
    fn gpu_profile_distinguishes_authored_fog_smoke_and_flame() {
        let make_volume = |source, emissive_radiance| FogVolume {
            bounds: Some(FogBounds {
                center: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                half_extents: Vec3::splat(4.0),
                shape: FogShape::Ellipsoid,
            }),
            extinction_per_meter: 0.7,
            single_scatter_albedo: [0.8; 3],
            edge_softness: 0.4,
            emissive_radiance,
            emission_temperature_k: if emissive_radiance == [0.0; 3] {
                0.0
            } else {
                1850.0
            },
            source,
        };

        let authored = gpu_volume_from_ecs(
            make_volume(FogSource::AuthoredMesh, [0.0; 3]),
            GlobalTransform::IDENTITY,
        )
        .unwrap();
        let smoke = gpu_volume_from_ecs(
            make_volume(FogSource::ParticleEmitter, [0.0; 3]),
            GlobalTransform::IDENTITY,
        )
        .unwrap();
        let flame = gpu_volume_from_ecs(
            make_volume(FogSource::ParticleEmitter, [8.0, 3.0, 0.5]),
            GlobalTransform::IDENTITY,
        )
        .unwrap();

        assert_eq!(authored.profile_params[0], FOG_VOLUME_PROFILE_HOMOGENEOUS);
        assert_eq!(smoke.profile_params[0], FOG_VOLUME_PROFILE_SMOKE);
        assert_eq!(flame.profile_params[0], FOG_VOLUME_PROFILE_FLAME);
    }

    #[test]
    fn invalid_or_cell_scope_volume_is_skipped() {
        let volume = FogVolume {
            bounds: None,
            extinction_per_meter: 0.5,
            single_scatter_albedo: [0.9; 3],
            edge_softness: 0.4,
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
