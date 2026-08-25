//! ECS host adapter for the renderer-independent `byroredux-sdk` Studio API.

use byroredux_core::ecs::{ActiveCamera, GlobalTransform, Material, Transform, World, WorldBound};
use byroredux_core::math::{EulerRot, Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_sdk::studio::{
    pick_spheres, BoundSphere, MaterialValue, ObjectSnapshot, StudioCommand, StudioSession,
    StudioSnapshot, TransformValue,
};

pub(crate) fn snapshot(world: &World) -> Option<StudioSnapshot> {
    let session = world.try_resource::<StudioSession>()?.clone();
    let pool = world.try_resource::<StringPool>();
    let objects = session
        .objects
        .iter()
        .filter_map(|&entity| {
            let transform = world.get::<Transform>(entity)?;
            let (x, y, z) = transform.rotation.to_euler(EulerRot::XYZ);
            let name = world
                .get::<byroredux_core::ecs::Name>(entity)
                .and_then(|name| pool.as_ref().and_then(|pool| pool.resolve(name.0)))
                .unwrap_or("")
                .to_owned();
            let material = world.get::<Material>(entity).map(|material| MaterialValue {
                diffuse_color: material.diffuse_color,
                metalness: material.metalness,
                roughness: material.roughness,
                alpha: material.alpha,
                ior: material.ior,
            });
            Some(ObjectSnapshot {
                entity,
                name,
                transform: TransformValue {
                    translation: transform.translation.to_array(),
                    rotation_degrees: [x.to_degrees(), y.to_degrees(), z.to_degrees()],
                    scale: transform.scale,
                },
                material,
            })
        })
        .collect();
    Some(StudioSnapshot {
        source_label: session.source.label,
        revision: session.revision,
        selected: session.selected,
        objects,
    })
}

pub(crate) fn apply_command(world: &mut World, command: StudioCommand) {
    if world.try_resource::<StudioSession>().is_none() {
        return;
    }
    match command {
        StudioCommand::Select(entity) => {
            let allowed = entity
                .is_none_or(|entity| world.resource::<StudioSession>().objects.contains(&entity));
            if allowed {
                world.resource_mut::<StudioSession>().selected = entity;
            }
        }
        StudioCommand::PickFromView => pick_from_view(world),
        StudioCommand::SetTransform { entity, value } => {
            if !is_object(world, entity) || !valid_transform(value) {
                return;
            }
            if let Some(transform) = world.get_mut::<Transform>(entity) {
                let radians = value.rotation_degrees.map(f32::to_radians);
                transform.translation = Vec3::from_array(value.translation);
                transform.rotation =
                    Quat::from_euler(EulerRot::XYZ, radians[0], radians[1], radians[2]);
                transform.scale = value.scale.clamp(0.001, 10_000.0);
                bump_revision(world);
            }
        }
        StudioCommand::ResetTransform(entity) => {
            let original = world
                .resource::<StudioSession>()
                .original_transforms
                .get(&entity)
                .copied();
            if let Some(value) = original {
                apply_command(world, StudioCommand::SetTransform { entity, value });
            }
        }
        StudioCommand::SetMaterial { entity, value } => {
            if !is_object(world, entity) || !valid_material(value) {
                return;
            }
            if let Some(material) = world.get_mut::<Material>(entity) {
                material.diffuse_color = value.diffuse_color.map(|v| v.clamp(0.0, 1.0));
                material.metalness = value.metalness.clamp(0.0, 1.0);
                material.roughness = value.roughness.clamp(0.0, 1.0);
                material.alpha = value.alpha.clamp(0.0, 1.0);
                material.ior = value.ior.clamp(1.0, 3.0);
                bump_revision(world);
            }
        }
        StudioCommand::FrameSelection(entity) => frame_selection(world, entity),
    }
}

fn is_object(world: &World, entity: byroredux_core::ecs::EntityId) -> bool {
    world.resource::<StudioSession>().objects.contains(&entity)
}

fn bump_revision(world: &World) {
    let mut session = world.resource_mut::<StudioSession>();
    session.revision = session.revision.saturating_add(1);
}

fn pick_from_view(world: &World) {
    let Some((origin, direction)) = crate::interaction::camera_ray(world) else {
        return;
    };
    let objects = world.resource::<StudioSession>().objects.clone();
    let spheres = objects.into_iter().filter_map(|entity| {
        let bound = world.get::<WorldBound>(entity)?;
        Some((
            entity,
            BoundSphere {
                center: bound.center.to_array(),
                radius: bound.radius,
            },
        ))
    });
    let selected = pick_spheres(origin.to_array(), direction.to_array(), spheres);
    world.resource_mut::<StudioSession>().selected = selected;
}

fn frame_selection(world: &mut World, entity: byroredux_core::ecs::EntityId) {
    if !is_object(world, entity) {
        return;
    }
    let (center, radius) = world
        .get::<WorldBound>(entity)
        .map(|bound| (bound.center, bound.radius.max(0.5)))
        .unwrap_or_else(|| {
            let center = world
                .get::<GlobalTransform>(entity)
                .map(|value| value.translation)
                .unwrap_or(Vec3::ZERO);
            (center, 1.0)
        });
    let Some(camera) = world.try_resource::<ActiveCamera>().map(|camera| camera.0) else {
        return;
    };
    let position = center + Vec3::Z * (radius * 3.0).max(2.0);
    let direction = (center - position).normalize_or_zero();
    if direction == Vec3::ZERO {
        return;
    }
    let rotation = Quat::from_rotation_arc(Vec3::NEG_Z, direction);
    if let Some(transform) = world.get_mut::<Transform>(camera) {
        transform.translation = position;
        transform.rotation = rotation;
    }
    if let Some(transform) = world.get_mut::<GlobalTransform>(camera) {
        transform.translation = position;
        transform.rotation = rotation;
    }
}

fn valid_transform(value: TransformValue) -> bool {
    value.translation.into_iter().all(f32::is_finite)
        && value.rotation_degrees.into_iter().all(f32::is_finite)
        && value.scale.is_finite()
}

fn valid_material(value: MaterialValue) -> bool {
    value.diffuse_color.into_iter().all(f32::is_finite)
        && [value.metalness, value.roughness, value.alpha, value.ior]
            .into_iter()
            .all(f32::is_finite)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use byroredux_sdk::studio::{AssetSource, CornellFit};

    fn fit() -> CornellFit {
        CornellFit {
            center: [0.0; 3],
            half_width: 2.0,
            half_depth: 2.0,
            floor_y: -1.0,
            height: 3.0,
            wall_thickness: 0.05,
            camera_position: [0.0, 0.0, 5.0],
            camera_target: [0.0; 3],
        }
    }

    #[test]
    fn typed_transform_command_mutates_only_document_objects() {
        let mut world = World::new();
        let object = world.spawn();
        let outsider = world.spawn();
        world.insert(object, Transform::IDENTITY);
        world.insert(outsider, Transform::IDENTITY);
        world.insert_resource(StudioSession {
            source: AssetSource {
                label: "fixture.nif".to_owned(),
            },
            objects: vec![object],
            selected: Some(object),
            fit: fit(),
            revision: 0,
            original_transforms: BTreeMap::from([(
                object,
                TransformValue {
                    translation: [0.0; 3],
                    rotation_degrees: [0.0; 3],
                    scale: 1.0,
                },
            )]),
        });
        let value = TransformValue {
            translation: [1.0, 2.0, 3.0],
            rotation_degrees: [0.0, 90.0, 0.0],
            scale: 2.0,
        };
        apply_command(
            &mut world,
            StudioCommand::SetTransform {
                entity: object,
                value,
            },
        );
        apply_command(
            &mut world,
            StudioCommand::SetTransform {
                entity: outsider,
                value,
            },
        );

        let transform = world.get::<Transform>(object).unwrap();
        assert_eq!(transform.translation, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(transform.scale, 2.0);
        assert_eq!(
            world.get::<Transform>(outsider).unwrap().translation,
            Vec3::ZERO
        );
        assert_eq!(world.resource::<StudioSession>().revision, 1);
    }
}
