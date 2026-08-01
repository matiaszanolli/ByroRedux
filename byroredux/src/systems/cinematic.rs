//! App-side sinks for scripted cinematic requests.

use crate::components::{HavokAnimationTarget, HavokIdleCatalog};
use byroredux_core::animation::{AnimationPlayer, RootMotionDelta};
use byroredux_core::ecs::components::RigidBodyData;
use byroredux_core::ecs::Transform;
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::string::StringPool;
use byroredux_physics::{PhysicsWorld, RapierHandles};
use byroredux_scripting::{
    ActorCinematicState, AnimationTextKeyEvents, CinematicAnimationEvent,
    CinematicPresentationState, HorseTetherState, MotionTypeChangeRequest,
};

/// Consume queued Skyrim `PlayIdle` requests once their IDLE FormID has a
/// decoded HKX clip. Unresolved requests remain pending, allowing a later cell
/// load to install the relevant archive without losing the authored request.
pub(crate) fn havok_idle_playback_system(world: &World, _dt: f32) {
    let requests: Vec<(EntityId, u64, EntityId, u32)> = {
        let Some(catalog) = world.try_resource::<HavokIdleCatalog>() else {
            return;
        };
        let Some(states) = world.query::<ActorCinematicState>() else {
            return;
        };
        let Some(targets) = world.query::<HavokAnimationTarget>() else {
            return;
        };
        states
            .iter()
            .filter_map(|(actor, state)| {
                let target = targets.get(actor)?;
                if state.idle_request_serial == target.consumed_idle_serial {
                    return None;
                }
                let handle = catalog
                    .handles
                    .get(&state.requested_idle_form_id?)
                    .copied()?;
                Some((
                    actor,
                    state.idle_request_serial,
                    target.skeleton_root,
                    handle,
                ))
            })
            .collect()
    };
    if requests.is_empty() {
        return;
    }

    if let Some(mut players) = world.query_mut::<AnimationPlayer>() {
        for (actor, _, skeleton_root, handle) in &requests {
            let replacement = AnimationPlayer::new(*handle).with_root(*skeleton_root);
            if let Some(player) = players.get_mut(*actor) {
                *player = replacement;
            } else {
                players.insert(*actor, replacement);
            }
        }
    } else {
        return;
    }
    if let Some(mut root_motion) = world.query_mut::<RootMotionDelta>() {
        for (actor, _, _, _) in &requests {
            root_motion.insert(*actor, RootMotionDelta(byroredux_core::math::Vec3::ZERO));
        }
    }
    if let Some(mut targets) = world.query_mut::<HavokAnimationTarget>() {
        for (actor, serial, _, _) in requests {
            if let Some(target) = targets.get_mut(actor) {
                target.consumed_idle_serial = serial;
            }
        }
    }
}

/// Apply a cart-exit clip's local COM displacement to its actor root. This is
/// sequenced immediately after animation sampling and before `ExitCartEnd`
/// clears the captured vehicle orientation.
pub(crate) fn cinematic_root_motion_system(world: &World, _dt: f32) {
    let motions: Vec<(
        EntityId,
        byroredux_core::math::Vec3,
        byroredux_core::math::Quat,
    )> = {
        let Some(states) = world.query::<ActorCinematicState>() else {
            return;
        };
        let Some(root_motion) = world.query::<RootMotionDelta>() else {
            return;
        };
        let transforms = world.query::<Transform>();
        root_motion
            .iter()
            .filter_map(|(actor, motion)| {
                let state = states.get(actor)?;
                if state.awaited_event != Some(CinematicAnimationEvent::ExitCartEnd) {
                    return None;
                }
                let rotation = state.exit_root_motion_rotation.or_else(|| {
                    transforms
                        .as_ref()
                        .and_then(|transforms| transforms.get(actor).map(|actor| actor.rotation))
                })?;
                Some((actor, motion.0, rotation))
            })
            .collect()
    };
    if motions.is_empty() {
        return;
    }

    let mut positions = Vec::with_capacity(motions.len());
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for (actor, local_delta, rotation) in &motions {
            let Some(transform) = transforms.get_mut(*actor) else {
                continue;
            };
            if local_delta.is_finite() {
                transform.translation += *rotation * (*local_delta * transform.scale);
            }
            transform.rotation = *rotation;
            positions.push((*actor, transform.translation));
        }
    }
    if let Some(mut root_motion) = world.query_mut::<RootMotionDelta>() {
        for (actor, _, _) in &motions {
            if let Some(motion) = root_motion.get_mut(*actor) {
                motion.0 = byroredux_core::math::Vec3::ZERO;
            }
        }
    }
    for (actor, position) in positions {
        byroredux_physics::set_kinematic_translation(world, actor, position);
    }
}

/// Deliver behavior-level animation notifications after clip advancement and
/// before transient text-key events are drained in `Late`.
pub(crate) fn cinematic_animation_event_system(world: &World, _dt: f32) {
    let deliveries: Vec<(EntityId, CinematicAnimationEvent)> = {
        let Some(pool) = world.try_resource::<StringPool>() else {
            return;
        };
        let Some(event_query) = world.query::<AnimationTextKeyEvents>() else {
            return;
        };
        let mut deliveries = Vec::new();
        for (entity, events) in event_query.iter() {
            for event in &events.0 {
                let Some(label) = pool.resolve(event.label) else {
                    continue;
                };
                if let Some(kind) = cinematic_event_from_label(label) {
                    deliveries.push((entity, kind));
                }
            }
        }
        deliveries
    };
    if deliveries.is_empty() {
        return;
    }

    if let Some(mut states) = world.query_mut::<ActorCinematicState>() {
        for (entity, event) in &deliveries {
            let Some(state) = states.get_mut(*entity) else {
                continue;
            };
            state.last_animation_event = Some(*event);
            state.animation_event_serial = state.animation_event_serial.wrapping_add(1);
            if state.awaited_event == Some(*event) {
                state.awaited_event = None;
                if *event == CinematicAnimationEvent::ExitCartEnd {
                    state.exit_root_motion_rotation = None;
                }
            }
        }
    }

    let Some(player) = world
        .try_resource::<byroredux_scripting::papyrus_demo::PlayerEntity>()
        .map(|player| player.0)
    else {
        return;
    };
    let Some(mut presentation) = world.try_resource_mut::<CinematicPresentationState>() else {
        return;
    };
    for (_, event) in deliveries.iter().filter(|(entity, _)| *entity == player) {
        let registered = match event {
            CinematicAnimationEvent::PlayImod => presentation.player_imod_event_registered,
            CinematicAnimationEvent::IdleFurnitureExit => {
                presentation.player_furniture_exit_event_registered
            }
            CinematicAnimationEvent::ExitCartEnd => false,
        };
        if registered {
            presentation.last_player_animation_event = Some(*event);
            presentation.player_animation_event_serial =
                presentation.player_animation_event_serial.wrapping_add(1);
        }
    }
}

fn cinematic_event_from_label(label: &str) -> Option<CinematicAnimationEvent> {
    if label.eq_ignore_ascii_case("PlayImod") {
        Some(CinematicAnimationEvent::PlayImod)
    } else if label.eq_ignore_ascii_case("IdleFurnitureExit") {
        Some(CinematicAnimationEvent::IdleFurnitureExit)
    } else if label.eq_ignore_ascii_case("ExitCartEnd") {
        Some(CinematicAnimationEvent::ExitCartEnd)
    } else {
        None
    }
}

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
    use byroredux_scripting::AnimationTextKeyEvent;

    #[test]
    fn idle_request_starts_scoped_havok_player_once_per_serial() {
        let mut world = World::new();
        world.register::<ActorCinematicState>();
        world.register::<HavokAnimationTarget>();
        world.register::<AnimationPlayer>();
        world.register::<RootMotionDelta>();
        let mut catalog = HavokIdleCatalog::default();
        catalog.handles.insert(0x0010_6AE3, 17);
        world.insert_resource(catalog);

        let skeleton = world.spawn();
        let actor = world.spawn();
        world.insert(
            actor,
            ActorCinematicState {
                requested_idle_form_id: Some(0x0010_6AE3),
                idle_request_serial: 1,
                ..Default::default()
            },
        );
        world.insert(
            actor,
            HavokAnimationTarget {
                skeleton_root: skeleton,
                consumed_idle_serial: 0,
            },
        );

        havok_idle_playback_system(&world, 0.0);
        let player = world.get::<AnimationPlayer>(actor).unwrap();
        assert_eq!(player.clip_handle, 17);
        assert_eq!(player.root_entity, Some(skeleton));
        drop(player);
        assert_eq!(
            world.get::<RootMotionDelta>(actor).unwrap().0,
            byroredux_core::math::Vec3::ZERO
        );
        assert_eq!(
            world
                .get::<HavokAnimationTarget>(actor)
                .unwrap()
                .consumed_idle_serial,
            1
        );

        world.get_mut::<AnimationPlayer>(actor).unwrap().local_time = 0.25;
        havok_idle_playback_system(&world, 0.0);
        assert_eq!(
            world.get::<AnimationPlayer>(actor).unwrap().local_time,
            0.25,
            "the same request serial must not restart every frame"
        );
    }

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
    fn clip_events_complete_cart_wait_and_reach_registered_player_callback() {
        let mut world = World::new();
        world.register::<ActorCinematicState>();
        world.register::<AnimationTextKeyEvents>();
        let mut pool = StringPool::new();
        let exit_cart_end = pool.intern("ExitCartEnd");
        let furniture_exit = pool.intern("IdleFurnitureExit");
        world.insert_resource(pool);
        world.insert_resource(CinematicPresentationState {
            player_furniture_exit_event_registered: true,
            ..Default::default()
        });

        let player = world.spawn();
        world.insert_resource(byroredux_scripting::papyrus_demo::PlayerEntity(player));
        world.insert(
            player,
            ActorCinematicState {
                awaited_event: Some(CinematicAnimationEvent::ExitCartEnd),
                exit_root_motion_rotation: Some(byroredux_core::math::Quat::IDENTITY),
                ..Default::default()
            },
        );
        world.insert(
            player,
            AnimationTextKeyEvents(vec![
                AnimationTextKeyEvent {
                    label: exit_cart_end,
                    time: 1.0,
                },
                AnimationTextKeyEvent {
                    label: furniture_exit,
                    time: 1.0,
                },
            ]),
        );

        cinematic_animation_event_system(&world, 0.0);

        let actor = world.get::<ActorCinematicState>(player).unwrap();
        assert_eq!(actor.awaited_event, None);
        assert_eq!(actor.exit_root_motion_rotation, None);
        assert_eq!(
            actor.last_animation_event,
            Some(CinematicAnimationEvent::IdleFurnitureExit)
        );
        assert_eq!(actor.animation_event_serial, 2);
        drop(actor);
        let presentation = world.resource::<CinematicPresentationState>();
        assert!(presentation.player_furniture_exit_event_registered);
        assert_eq!(
            presentation.last_player_animation_event,
            Some(CinematicAnimationEvent::IdleFurnitureExit)
        );
        assert_eq!(presentation.player_animation_event_serial, 1);
    }

    #[test]
    fn cart_exit_root_motion_moves_and_orients_actor_then_drains_delta() {
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        world.register::<ActorCinematicState>();
        world.register::<RootMotionDelta>();
        world.register::<Transform>();
        let actor = world.spawn();
        let exit_rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        world.insert(
            actor,
            ActorCinematicState {
                awaited_event: Some(CinematicAnimationEvent::ExitCartEnd),
                exit_root_motion_rotation: Some(exit_rotation),
                ..Default::default()
            },
        );
        world.insert(
            actor,
            Transform::new(Vec3::new(100.0, 0.0, 50.0), Quat::IDENTITY, 1.0),
        );
        world.insert(actor, RootMotionDelta(Vec3::new(0.0, 0.0, -10.0)));

        cinematic_root_motion_system(&world, 0.0);

        let transform = world.get::<Transform>(actor).unwrap();
        assert!((transform.translation - Vec3::new(90.0, 0.0, 50.0)).length() < 1e-5);
        assert_eq!(transform.rotation, exit_rotation);
        drop(transform);
        assert_eq!(world.get::<RootMotionDelta>(actor).unwrap().0, Vec3::ZERO);
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
