//! ECS host adapter for the renderer-independent `byroredux-sdk` Studio API.

use std::collections::BTreeMap;

use byroredux_core::ecs::{
    ActiveCamera, EntityId, GlobalTransform, Material, Resource, Transform, World, WorldBound,
};
use byroredux_core::math::{EulerRot, Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_sdk::identity::ObjectId;
use byroredux_sdk::studio::{
    pick_spheres, AssetSource, BoundSphere, MaterialValue, ObjectSnapshot, StudioCommand,
    StudioSnapshot, TransformValue,
};

#[derive(Debug, Clone, Copy)]
struct StudioObjectBinding {
    id: ObjectId,
    entity: EntityId,
}

/// ECS-owned state behind the public renderer-independent Studio contract.
/// Raw entity IDs remain private to this adapter.
#[derive(Debug, Clone)]
pub(crate) struct StudioSession {
    source: AssetSource,
    objects: Vec<StudioObjectBinding>,
    selected: Option<ObjectId>,
    revision: u64,
    original_transforms: BTreeMap<ObjectId, TransformValue>,
}

impl Resource for StudioSession {}

/// Install a Studio document and assign stable IDs from canonical import order.
pub(crate) fn install_session(world: &mut World, source: AssetSource, entities: Vec<EntityId>) {
    let objects: Vec<_> = entities
        .into_iter()
        .enumerate()
        .map(|(ordinal, entity)| StudioObjectBinding {
            id: ObjectId::from_import_ordinal(ordinal)
                .expect("Studio object count exceeds the ObjectId range"),
            entity,
        })
        .collect();
    let original_transforms = objects
        .iter()
        .filter_map(|binding| {
            world.get::<Transform>(binding.entity).map(|transform| {
                let (x, y, z) = transform.rotation.to_euler(EulerRot::XYZ);
                (
                    binding.id,
                    TransformValue {
                        translation: transform.translation.to_array(),
                        rotation_degrees: [x.to_degrees(), y.to_degrees(), z.to_degrees()],
                        scale: transform.scale,
                    },
                )
            })
        })
        .collect();
    world.insert_resource(StudioSession {
        source,
        selected: objects.first().map(|binding| binding.id),
        objects,
        revision: 0,
        original_transforms,
    });
}

pub(crate) fn snapshot(world: &World) -> Option<StudioSnapshot> {
    let session = world.try_resource::<StudioSession>()?.clone();
    let pool = world.try_resource::<StringPool>();
    let objects = session
        .objects
        .iter()
        .filter_map(|binding| {
            let transform = world.get::<Transform>(binding.entity)?;
            let (x, y, z) = transform.rotation.to_euler(EulerRot::XYZ);
            let name = world
                .get::<byroredux_core::ecs::Name>(binding.entity)
                .and_then(|name| pool.as_ref().and_then(|pool| pool.resolve(name.0)))
                .unwrap_or("")
                .to_owned();
            let material = world
                .get::<Material>(binding.entity)
                .map(|material| MaterialValue {
                    diffuse_color: material.diffuse_color,
                    metalness: material.metalness,
                    roughness: material.roughness,
                    alpha: material.alpha,
                    ior: material.ior,
                });
            Some(ObjectSnapshot {
                id: binding.id,
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
        StudioCommand::Select(object) => {
            let allowed = object.is_none_or(|object| entity_for(world, object).is_some());
            if allowed {
                world.resource_mut::<StudioSession>().selected = object;
            }
        }
        StudioCommand::PickFromView => pick_from_view(world),
        StudioCommand::SetTransform { object, value } => {
            let Some(entity) = entity_for(world, object) else {
                return;
            };
            if !valid_transform(value) {
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
        StudioCommand::ResetTransform(object) => {
            let original = world
                .resource::<StudioSession>()
                .original_transforms
                .get(&object)
                .copied();
            if let Some(value) = original {
                apply_command(world, StudioCommand::SetTransform { object, value });
            }
        }
        StudioCommand::SetMaterial { object, value } => {
            let Some(entity) = entity_for(world, object) else {
                return;
            };
            if !valid_material(value) {
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
        StudioCommand::FrameSelection(object) => frame_selection(world, object),
    }
}

fn entity_for(world: &World, object: ObjectId) -> Option<EntityId> {
    world
        .resource::<StudioSession>()
        .objects
        .iter()
        .find_map(|binding| (binding.id == object).then_some(binding.entity))
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
    let spheres = objects.into_iter().filter_map(|binding| {
        let bound = world.get::<WorldBound>(binding.entity)?;
        Some((
            binding.id,
            BoundSphere {
                center: bound.center.to_array(),
                radius: bound.radius,
            },
        ))
    });
    let selected = pick_spheres(origin.to_array(), direction.to_array(), spheres);
    world.resource_mut::<StudioSession>().selected = selected;
}

fn frame_selection(world: &mut World, object: ObjectId) {
    let Some(entity) = entity_for(world, object) else {
        return;
    };
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
    use super::*;

    #[test]
    fn typed_transform_command_mutates_only_document_objects() {
        let mut world = World::new();
        let object = world.spawn();
        let outsider = world.spawn();
        world.insert(object, Transform::IDENTITY);
        world.insert(outsider, Transform::IDENTITY);
        install_session(
            &mut world,
            AssetSource {
                label: "fixture.nif".to_owned(),
            },
            vec![object],
        );
        let object_id = ObjectId::new(1).unwrap();
        let outsider_id = ObjectId::new(2).unwrap();
        let value = TransformValue {
            translation: [1.0, 2.0, 3.0],
            rotation_degrees: [0.0, 90.0, 0.0],
            scale: 2.0,
        };
        apply_command(
            &mut world,
            StudioCommand::SetTransform {
                object: object_id,
                value,
            },
        );
        apply_command(
            &mut world,
            StudioCommand::SetTransform {
                object: outsider_id,
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

    #[test]
    fn snapshot_exposes_document_ids_not_ecs_entity_ids() {
        let mut world = World::new();
        for _ in 0..8 {
            world.spawn();
        }
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        install_session(
            &mut world,
            AssetSource {
                label: "fixture.nif".to_owned(),
            },
            vec![entity],
        );

        let snapshot = snapshot(&world).unwrap();
        assert_ne!(entity as u64, snapshot.objects[0].id.get());
        assert_eq!(snapshot.objects[0].id, ObjectId::new(1).unwrap());
        assert_eq!(snapshot.selected, Some(snapshot.objects[0].id));
    }
}
