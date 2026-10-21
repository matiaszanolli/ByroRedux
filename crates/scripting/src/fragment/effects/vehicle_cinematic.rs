//! The vehicle / cinematic effect applier. (#5468, split from `effects.rs`)

use super::*;

/// The cart/vehicle cinematic arms — `ActorCinematicState`,
/// `HorseTetherState` and the `MotionTypeChangeRequest` bridge.
///
/// #4340 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays
/// named in `apply_effect`'s nested-lock residual list, which the #3949
/// scan now reads across these helpers too.
pub(super) fn apply_vehicle_cinematic_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::PlayIdle { actor, idle } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings, deferred.speaker)?;
            let idle_form_id = resolve_property_form_id(vmad, idle.property_name())?;
            if !update_actor_cinematic_state(world, actor, |state| {
                state.request_idle(idle_form_id);
            }) {
                log::debug!("fragment PlayIdle skipped: cinematic actor storage is unavailable");
            }
            None
        }
        Effect::SetVehicle { actor, vehicle } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings, deferred.speaker)?;
            let vehicle = match vehicle {
                Some(vehicle) => Some(resolve_object(
                    vmad,
                    world,
                    context,
                    vehicle,
                    &deferred.scene_actor_bindings,
                )?),
                None => None,
            };
            let relative_pose = vehicle.and_then(|vehicle| {
                let actor_transform = copied_transform(world, actor)?;
                let vehicle_transform = copied_transform(world, vehicle)?;
                if vehicle_transform.scale.abs() <= f32::EPSILON {
                    return None;
                }
                let inverse_rotation = vehicle_transform.rotation.conjugate();
                Some((
                    inverse_rotation
                        * (actor_transform.translation - vehicle_transform.translation)
                        / vehicle_transform.scale,
                    inverse_rotation * actor_transform.rotation,
                ))
            });
            if !update_actor_cinematic_state(world, actor, |state| {
                state.vehicle = vehicle;
                if vehicle.is_some() {
                    state.cart_seat = None;
                }
                state.vehicle_local_translation = relative_pose.map(|pose| pose.0);
                state.vehicle_local_rotation = relative_pose.map(|pose| pose.1);
            }) {
                log::debug!("fragment SetVehicle skipped: cinematic actor storage is unavailable");
            }
            None
        }
        Effect::TetherToHorse { cart, horse } => {
            let cart = resolve_object(vmad, world, context, cart, &deferred.scene_actor_bindings)?;
            let horse =
                resolve_object(vmad, world, context, horse, &deferred.scene_actor_bindings)?;
            let Some(cart_transform) = copied_transform(world, cart) else {
                log::debug!("fragment TetherToHorse skipped: cart has no Transform");
                return None;
            };
            let Some(horse_transform) = copied_transform(world, horse) else {
                log::debug!("fragment TetherToHorse skipped: horse has no Transform");
                return None;
            };
            if horse_transform.scale.abs() <= f32::EPSILON {
                log::debug!("fragment TetherToHorse skipped: horse has zero scale");
                return None;
            }
            let inverse_rotation = horse_transform.rotation.conjugate();
            let state = crate::HorseTetherState {
                horse,
                horse_local_translation: inverse_rotation
                    * (cart_transform.translation - horse_transform.translation)
                    / horse_transform.scale,
                horse_local_rotation: inverse_rotation * cart_transform.rotation,
                route_target_form_id: None,
            };
            if let Some(mut tethers) = world.query_mut::<crate::HorseTetherState>() {
                tethers.insert(cart, state);
                log::info!("TetherToHorse attached cart entity {cart} to horse entity {horse}");
            }
            // The native engine realizes this relation through Havok. Redux's
            // deterministic transform follower needs the cart to stop being
            // pulled back from a dynamic Rapier body after PostUpdate, so make
            // the tethered cart keyframed through the same one-shot bridge as
            // authored SetMotionType calls.
            if let Some(mut requests) = world.query_mut::<crate::MotionTypeChangeRequest>() {
                requests.insert(
                    cart,
                    crate::MotionTypeChangeRequest {
                        motion_type: byroredux_core::ecs::components::MotionType::Keyframed,
                        allow_activate: true,
                    },
                );
            }
            None
        }
        Effect::SetMotionType {
            target,
            motion_type,
            allow_activate,
        } => {
            // #6237 — the persistent half: the bodies the request changes are
            // collision entities the next load or cell visit rebuilds.
            if let Some(form_id) = object_form_id(vmad, world, context, target, deferred) {
                deferred.reference_motion_type_changes.push((form_id, *motion_type));
            }
            let target =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
            if let Some(mut requests) = world.query_mut::<crate::MotionTypeChangeRequest>() {
                requests.insert(
                    target,
                    crate::MotionTypeChangeRequest {
                        motion_type: *motion_type,
                        allow_activate: *allow_activate,
                    },
                );
            }
            None
        }
        Effect::ExitCart { actor, seat } => {
            let actor =
                resolve_object(vmad, world, context, actor, &deferred.scene_actor_bindings)?;
            let idle_form_id = exit_cart_idle_property(*seat)
                .and_then(|property| resolve_property_form_id(vmad, property));
            let attachment = world
                .get::<crate::ActorCinematicState>(actor)
                .and_then(|state| Some((state.vehicle?, state.vehicle_local_rotation?)));
            let exit_root_motion_rotation = world.query::<Transform>().and_then(|transforms| {
                attachment
                    .and_then(|(vehicle, local_rotation)| {
                        transforms
                            .get(vehicle)
                            .map(|vehicle| vehicle.rotation * local_rotation)
                    })
                    .or_else(|| transforms.get(actor).map(|actor| actor.rotation))
            });
            if !update_actor_cinematic_state(world, actor, |state| {
                state.vehicle = None;
                state.vehicle_local_translation = None;
                state.vehicle_local_rotation = None;
                state.cart_seat = Some(*seat);
                state.awaited_event = Some(crate::CinematicAnimationEvent::ExitCartEnd);
                state.exit_root_motion_rotation = exit_root_motion_rotation;
                if let Some(idle_form_id) = idle_form_id {
                    state.request_idle(idle_form_id);
                }
            }) {
                log::debug!("fragment ExitCart skipped: cinematic actor storage is unavailable");
            }
            None
        }
        _ => unreachable!("apply_vehicle_cinematic_effect received a non-vehicle-cinematic effect"),
    }
}
