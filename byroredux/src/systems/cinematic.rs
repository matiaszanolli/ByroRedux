//! App-side sinks for scripted cinematic requests.

use byroredux_core::ecs::components::RigidBodyData;
use byroredux_core::ecs::Transform;
use byroredux_core::ecs::{EntityId, World};
use byroredux_physics::{PhysicsWorld, RapierHandles};
use byroredux_scripting::{ActorCinematicState, HorseTetherState, MotionTypeChangeRequest};

/// Apply Papyrus `SetMotionType` requests to both canonical ECS body data and
/// an already-live Rapier body, then drain the one-shot request.
pub(crate) fn scripted_motion_type_system(world: &World, _dt: f32) {
    let requests: Vec<(EntityId, MotionTypeChangeRequest, Option<RapierHandles>)> = {
        let Some(request_q) = world.query::<MotionTypeChangeRequest>() else {
            return;
        };
        let handles_q = world.query::<RapierHandles>();
        request_q
            .iter()
            .map(|(entity, request)| {
                (
                    entity,
                    *request,
                    handles_q
                        .as_ref()
                        .and_then(|handles| handles.get(entity).copied()),
                )
            })
            .collect()
    };
    if requests.is_empty() {
        return;
    }

    if let Some(mut bodies) = world.query_mut::<RigidBodyData>() {
        for (entity, request, _) in &requests {
            if let Some(body) = bodies.get_mut(*entity) {
                body.motion_type = request.motion_type;
            }
        }
    }

    if let Some(mut physics) = world.try_resource_mut::<PhysicsWorld>() {
        for (_, request, handles) in &requests {
            if let Some(handles) = handles {
                physics.set_motion_type(handles.body, request.motion_type, request.allow_activate);
            }
        }
    }

    if let Some(mut request_q) = world.query_mut::<MotionTypeChangeRequest>() {
        for (entity, _, _) in requests {
            request_q.remove(entity);
        }
    }
}

/// Resolve MQ101's complete attachment chain: package-driven horse -> tethered
/// cart -> `SetVehicle` riders. This runs in Update before transform
/// propagation, so carts, riders, and their skeletons observe the new root
/// poses in the same frame.
pub(crate) fn vehicle_attachment_system(world: &World, _dt: f32) {
    let tethered_carts: Vec<(
        EntityId,
        byroredux_core::math::Vec3,
        byroredux_core::math::Quat,
    )> = {
        match (
            world.query::<HorseTetherState>(),
            world.query::<Transform>(),
        ) {
            (Some(tethers), Some(transforms)) => tethers
                .iter()
                .filter_map(|(cart, tether)| {
                    let horse = transforms.get(tether.horse)?;
                    Some((
                        cart,
                        horse.translation
                            + horse.rotation * (tether.horse_local_translation * horse.scale),
                        horse.rotation * tether.horse_local_rotation,
                    ))
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for (cart, translation, rotation) in tethered_carts {
            if let Some(transform) = transforms.get_mut(cart) {
                transform.translation = translation;
                transform.rotation = rotation;
            }
        }
    }

    let attachments: Vec<(
        EntityId,
        byroredux_core::math::Vec3,
        byroredux_core::math::Quat,
    )> = {
        let Some(states) = world.query::<ActorCinematicState>() else {
            return;
        };
        let Some(transforms) = world.query::<Transform>() else {
            return;
        };
        states
            .iter()
            .filter_map(|(actor, state)| {
                let vehicle = transforms.get(state.vehicle?)?;
                let local_translation = state.vehicle_local_translation?;
                let local_rotation = state.vehicle_local_rotation?;
                Some((
                    actor,
                    vehicle.translation + vehicle.rotation * (local_translation * vehicle.scale),
                    vehicle.rotation * local_rotation,
                ))
            })
            .collect()
    };
    if attachments.is_empty() {
        return;
    }
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for (actor, translation, rotation) in &attachments {
            if let Some(transform) = transforms.get_mut(*actor) {
                transform.translation = *translation;
                transform.rotation = *rotation;
            }
        }
    }
    // Character controllers own a live kinematic Rapier body whose pose is
    // not pushed from Transform by the generic sync path. Target it here too
    // so an attached player cannot leave its collider behind on the road.
    if world.try_resource::<PhysicsWorld>().is_some() {
        for (actor, translation, _) in attachments {
            byroredux_physics::set_kinematic_translation(world, actor, translation);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::MotionType;

    #[test]
    fn request_updates_canonical_body_data_then_drains() {
        let mut world = World::new();
        world.register::<RigidBodyData>();
        world.register::<MotionTypeChangeRequest>();

        let entity = world.spawn();
        world.insert(
            entity,
            RigidBodyData {
                motion_type: MotionType::Dynamic,
                ..Default::default()
            },
        );
        world.insert(
            entity,
            MotionTypeChangeRequest {
                motion_type: MotionType::Keyframed,
                allow_activate: true,
            },
        );

        scripted_motion_type_system(&world, 0.0);

        assert_eq!(
            world.get::<RigidBodyData>(entity).unwrap().motion_type,
            MotionType::Keyframed
        );
        assert!(!world.has::<MotionTypeChangeRequest>(entity));
    }

    #[test]
    fn vehicle_attachment_follows_cart_transform() {
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<ActorCinematicState>();
        let vehicle = world.spawn();
        let actor = world.spawn();
        world.insert(
            vehicle,
            Transform::new(
                Vec3::new(100.0, 0.0, 50.0),
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                1.0,
            ),
        );
        world.insert(actor, Transform::IDENTITY);
        world.insert(
            actor,
            ActorCinematicState {
                vehicle: Some(vehicle),
                vehicle_local_translation: Some(Vec3::new(0.0, 0.0, 10.0)),
                vehicle_local_rotation: Some(Quat::IDENTITY),
                ..Default::default()
            },
        );

        vehicle_attachment_system(&world, 0.0);

        let actor_transform = world.get::<Transform>(actor).unwrap();
        assert!((actor_transform.translation - Vec3::new(110.0, 0.0, 50.0)).length() < 1e-5);
        assert!(
            (actor_transform.rotation * Vec3::Z - Vec3::X).length() < 1e-5,
            "actor inherits vehicle rotation"
        );
    }

    #[test]
    fn tethered_cart_and_rider_follow_package_driven_horse_in_one_tick() {
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<ActorCinematicState>();
        world.register::<HorseTetherState>();
        let horse = world.spawn();
        let cart = world.spawn();
        let rider = world.spawn();
        world.insert(
            horse,
            Transform::new(
                Vec3::new(100.0, 0.0, 50.0),
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                1.0,
            ),
        );
        world.insert(cart, Transform::IDENTITY);
        world.insert(rider, Transform::IDENTITY);
        world.insert(
            cart,
            HorseTetherState {
                horse,
                horse_local_translation: Vec3::new(0.0, 0.0, -10.0),
                horse_local_rotation: Quat::IDENTITY,
            },
        );
        world.insert(
            rider,
            ActorCinematicState {
                vehicle: Some(cart),
                vehicle_local_translation: Some(Vec3::new(0.0, 2.0, 0.0)),
                vehicle_local_rotation: Some(Quat::IDENTITY),
                ..Default::default()
            },
        );

        vehicle_attachment_system(&world, 0.0);

        let cart_transform = world.get::<Transform>(cart).unwrap();
        assert!((cart_transform.translation - Vec3::new(90.0, 0.0, 50.0)).length() < 1e-5);
        let rider_transform = world.get::<Transform>(rider).unwrap();
        assert!((rider_transform.translation - Vec3::new(90.0, 2.0, 50.0)).length() < 1e-5);
        assert!((rider_transform.rotation * Vec3::Z - Vec3::X).length() < 1e-5);
    }
}
