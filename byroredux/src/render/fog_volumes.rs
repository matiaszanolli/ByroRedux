//! ECS local fog-volume collection and GPU translation.

use byroredux_core::ecs::{FogShape, FogVolume, GlobalTransform, World};
use byroredux_core::math::Vec3;
use byroredux_renderer::vulkan::volumetrics::{
    GpuFogVolume, MAX_GPU_FOG_VOLUMES, WORLD_UNITS_PER_METER,
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
    out.sort_by(|a, b| {
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

    let shape = match bounds.shape {
        FogShape::Sphere => 0.0,
        FogShape::Ellipsoid => 1.0,
        FogShape::Box => 2.0,
    };
    let inverse_rotation = rotation.conjugate();
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
    })
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
            source: FogSource::ParticleEmitter,
        };
        let transform = GlobalTransform::new(Vec3::new(10.0, 20.0, 30.0), Quat::IDENTITY, 2.0);
        let gpu = gpu_volume_from_ecs(volume, transform).unwrap();
        assert_eq!(gpu.center_shape[..3], [10.0, 24.0, 30.0]);
        assert_eq!(gpu.half_extents_extinction[..3], [6.0, 8.0, 10.0]);
        assert!((gpu.half_extents_extinction[3] * WORLD_UNITS_PER_METER - 0.7).abs() < 1.0e-6);
    }

    #[test]
    fn invalid_or_cell_scope_volume_is_skipped() {
        let volume = FogVolume {
            bounds: None,
            extinction_per_meter: 0.5,
            single_scatter_albedo: [0.9; 3],
            edge_softness: 0.4,
            source: FogSource::Xcll,
        };
        assert!(gpu_volume_from_ecs(volume, GlobalTransform::IDENTITY).is_none());
    }
}
