//! App-side sinks for scripted cinematic requests.

use crate::components::{HavokAnimationTarget, HavokIdleCatalog};
use byroredux_core::animation::{AnimationPlayer, RootMotionDelta};
use byroredux_core::ecs::components::RigidBodyData;
use byroredux_core::ecs::Transform;
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::string::StringPool;
use byroredux_physics::{PhysicsWorld, RapierHandles};
use byroredux_scripting::{
    ActorCinematicState, AnimationTextKeyEvents, CinematicAnimationEvent, HorseTetherState,
    MotionTypeChangeRequest, PackageTargetRegistry, SceneAliasCandidate, ScenePackageCommand,
    ScenePackagePlayback,
};

const SCENE_TRIGGER_APPROACH_SPEED: f32 = 500.0;

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
        // #3446 — `AnimationTextKeyEvents` BEFORE `StringPool`, not after.
        // `StringPool` is the tail of `docs/engine/ecs.md`'s canonical
        // acquisition order precisely because nothing is ever acquired
        // beneath it; taking the pool first would record a
        // `StringPool -> AnimationTextKeyEvents` edge and demote the sink
        // to a mid-graph node, which is what makes the tail cheap to
        // reason about. Both are reads and nothing here needs the pool
        // before the query, so the sink property costs nothing to keep.
        let Some(event_query) = world.query::<AnimationTextKeyEvents>() else {
            return;
        };
        let Some(pool) = world.try_resource::<StringPool>() else {
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
    for (_, event) in deliveries.iter().filter(|(entity, _)| *entity == player) {
        byroredux_scripting::dispatch_player_cinematic_animation_event(world, *event);
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

/// Drive a native Skyrim cart tether along the horse reference's authored
/// XLKR waypoint chain. The vanilla opening convoy uses invisible XMarkers
/// for this route; they are intentionally not render entities, so positions
/// and edges come from [`PackageTargetRegistry`].
pub(crate) fn cinematic_horse_route_system(world: &World, dt: f32) {
    let Some(routes) = world.try_resource::<PackageTargetRegistry>() else {
        return;
    };
    let pending: Vec<(
        EntityId,
        EntityId,
        u32,
        byroredux_core::math::Vec3,
        byroredux_core::math::Quat,
    )> = {
        let Some(tethers) = world.query::<HorseTetherState>() else {
            return;
        };
        let Some(transforms) = world.query::<Transform>() else {
            return;
        };
        let candidates = world.query::<SceneAliasCandidate>();
        tethers
            .iter()
            .filter_map(|(cart, tether)| {
                let horse_transform = transforms.get(tether.horse)?;
                let target = tether.route_target_form_id.or_else(|| {
                    candidates
                        .as_ref()?
                        .get(tether.horse)?
                        .linked_refs
                        .iter()
                        .find(|(keyword, _)| *keyword == 0)
                        .or_else(|| candidates.as_ref()?.get(tether.horse)?.linked_refs.first())
                        .map(|(_, target)| *target)
                })?;
                routes.position(target)?;
                if tether.route_target_form_id.is_none() {
                    log::info!(
                        "Initialized tethered horse entity {} route at linked ref 0x{target:08X}",
                        tether.horse
                    );
                }
                Some((
                    cart,
                    tether.horse,
                    target,
                    horse_transform.translation,
                    horse_transform.rotation,
                ))
            })
            .collect()
    };
    if pending.is_empty() {
        return;
    }

    let decisions: Vec<_> = pending
        .into_iter()
        .filter_map(|(cart, horse, target, current, rotation)| {
            let marker = routes.position(target)?;
            // A terminal cart marker's authored heading carries the native
            // tether beyond the explicit chain and through downstream trigger
            // volumes. Continue far enough for those volumes to observe it.
            let destination = if routes.linked_reference(target).is_none() {
                marker
                    + routes
                        .direction(target)
                        .unwrap_or(byroredux_core::math::Vec3::ZERO)
                        * 4096.0
            } else {
                marker
            };
            let target_xz =
                byroredux_core::math::Vec3::new(destination.x, current.y, destination.z);
            let horizontal_before = byroredux_core::math::Vec3::new(
                current.x - destination.x,
                0.0,
                current.z - destination.z,
            )
            .length();
            // Cart routes are authored 3D splines. Generic actor locomotion's
            // ground ray is deliberately bypassed here: incomplete road
            // collision can sit hundreds of units below the XMarkers and
            // make the convoy miss vertically bounded scripted triggers.
            let (mut translation, new_rotation) =
                super::locomotion::step_toward(current, rotation, target_xz, dt, None);
            if horizontal_before > f32::EPSILON {
                let horizontal_after = byroredux_core::math::Vec3::new(
                    translation.x - destination.x,
                    0.0,
                    translation.z - destination.z,
                )
                .length();
                let progress =
                    ((horizontal_before - horizontal_after) / horizontal_before).clamp(0.0, 1.0);
                translation.y += (destination.y - current.y) * progress;
            } else {
                translation.y = destination.y;
            }
            let remaining = byroredux_core::math::Vec3::new(
                translation.x - destination.x,
                0.0,
                translation.z - destination.z,
            );
            let next_target = if remaining.length_squared()
                <= super::locomotion::LOCOMOTION_ARRIVAL_EPSILON
                    * super::locomotion::LOCOMOTION_ARRIVAL_EPSILON
            {
                routes.linked_reference(target).unwrap_or(target)
            } else {
                target
            };
            Some((cart, horse, translation, new_rotation, next_target))
        })
        .collect();
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for (_, horse, translation, rotation, _) in &decisions {
            if let Some(transform) = transforms.get_mut(*horse) {
                transform.translation = *translation;
                if let Some(rotation) = rotation {
                    transform.rotation = *rotation;
                }
            }
        }
    }
    if let Some(mut tethers) = world.query_mut::<HorseTetherState>() {
        for (cart, _, _, _, next_target) in &decisions {
            if let Some(tether) = tethers.get_mut(*cart) {
                tether.route_target_form_id = Some(*next_target);
            }
        }
    }
    if world.try_resource::<PhysicsWorld>().is_some() {
        for (_, horse, translation, _, _) in decisions {
            byroredux_physics::set_kinematic_translation(world, horse, translation);
        }
    }
}

/// Bridge offscreen cinematic locomotion for a scene phase whose authored
/// completion explicitly waits on an actor-specific trigger stage. Creation
/// can drive a mounted actor outside ordinary cell AI; choose the nearest
/// loaded actor with the trigger's required base and approach the volume.
pub(crate) fn scene_trigger_actor_approach_system(world: &World, dt: f32) {
    use byroredux_plugin::esm::records::condition::{ComparisonOp, ConditionValue};
    use byroredux_scripting::papyrus_demo::quest_advance::{ActivatorGate, QuestAdvanceOnActivate};

    let (awaited, between_scenes): (
        std::collections::HashSet<(u32, u16)>,
        std::collections::HashSet<u32>,
    ) = {
        let players: Vec<byroredux_scripting::ScenePlayer> = {
            let Some(players) = world.query::<byroredux_scripting::ScenePlayer>() else {
                return;
            };
            players.iter().map(|(_, player)| player.clone()).collect()
        };
        let Some(registry) = world.try_resource::<byroredux_scripting::SceneRegistry>() else {
            return;
        };
        let active_quests: std::collections::HashSet<u32> = players
            .iter()
            .filter(|player| player.is_running())
            .filter_map(|player| registry.definition(player.scene_form_id)?.quest_form_id)
            .collect();
        let awaited = players
            .iter()
            .filter(|player| player.is_running())
            .filter_map(|player| {
                let scene = registry.definition(player.scene_form_id)?;
                scene.phases.get(player.current_phase as usize)
            })
            .flat_map(|phase| &phase.completion_conditions)
            .filter_map(|condition| {
                (condition.function_index == 59
                    && condition.comparator == ComparisonOp::Eq
                    && matches!(condition.comparand, ConditionValue::Literal(value) if (value - 1.0).abs() <= f32::EPSILON))
                .then_some((condition.param_1, condition.param_2 as u16))
            })
            .collect();
        let between_scenes = players
            .iter()
            .filter(|player| player.state == byroredux_scripting::ScenePlaybackState::Finished)
            .filter_map(|player| registry.definition(player.scene_form_id)?.quest_form_id)
            .filter(|quest| !active_quests.contains(quest))
            .collect();
        (awaited, between_scenes)
    };
    if awaited.is_empty() && between_scenes.is_empty() {
        return;
    }

    let targets: Vec<_> = match (
        world.query::<QuestAdvanceOnActivate>(),
        world.query::<byroredux_scripting::TriggerVolume>(),
        world.try_resource::<byroredux_scripting::quest_stages::QuestStageState>(),
        world.try_resource::<
            byroredux_scripting::papyrus_demo::quest_advance::QuestTriggerApproachRegistry,
        >(),
    ) {
        (Some(advances), Some(volumes), Some(stages), Some(approaches)) => {
            let mut targets = Vec::new();
            let caps: Vec<_> = awaited
                .iter()
                .copied()
                .chain(
                    between_scenes
                        .iter()
                        .copied()
                        .filter(|quest| {
                            stages.is_running(byroredux_scripting::QuestFormId(*quest))
                        })
                        .map(|quest| (quest, u16::MAX)),
                )
                .collect();
            for (quest_form_id, awaited_stage) in caps {
                let current_stage = stages.get_stage(byroredux_scripting::QuestFormId(quest_form_id));
                let bases: std::collections::HashSet<u32> = advances
                    .iter()
                    .filter(|(_, advance)| {
                        advance.owning_quest.0 == quest_form_id
                            && (awaited_stage == u16::MAX
                                || advance.target_stage == awaited_stage)
                            && (awaited_stage != u16::MAX
                                || advance.target_stage >= current_stage)
                    })
                    .filter_map(|(_, advance)| match advance.activator_gate {
                        ActivatorGate::BaseForm(base_form_id) => Some(base_form_id),
                        _ => None,
                    })
                    .collect();
                let mut cap_targets = Vec::new();
                for base_form_id in bases {
                    let target = advances
                        .iter()
                        .filter(|(_, advance)| {
                            advance.owning_quest.0 == quest_form_id
                                && advance.target_stage <= awaited_stage
                                && (awaited_stage != u16::MAX
                                    || advance.target_stage >= current_stage)
                                && matches!(
                                    advance.activator_gate,
                                    ActivatorGate::BaseForm(candidate) if candidate == base_form_id
                                )
                                && !stages.get_stage_done(
                                    advance.owning_quest,
                                    advance.target_stage,
                                )
                        })
                        .filter(|(trigger, advance)| {
                            byroredux_scripting::evaluate_condition_list(
                                &advance.conditions,
                                world,
                                &byroredux_scripting::ConditionContext::for_subject(*trigger),
                            )
                        })
                        .filter_map(|(trigger, advance)| {
                            let center = volumes.get(trigger).map(|volume| volume.center).or_else(
                                || {
                                    approaches
                                        .entries()
                                        .iter()
                                        .find(|entry| entry.trigger_entity == trigger)
                                        .map(|entry| entry.center)
                                },
                            )?;
                            Some((advance.target_stage, trigger, center))
                        })
                        .min_by_key(|(stage, _, _)| *stage)
                        .map(|(stage, trigger, center)| {
                            (base_form_id, stage, trigger, center)
                        });
                    if let Some(target) = target {
                        cap_targets.push(target);
                    }
                }
                if awaited_stage == u16::MAX {
                    if let Some(next_stage) = cap_targets
                        .iter()
                        .map(|(_, stage, _, _)| *stage)
                        .min()
                    {
                        cap_targets.retain(|(_, stage, _, _)| *stage == next_stage);
                    }
                }
                targets.extend(cap_targets);
            }
            targets
        }
        _ => Vec::new(),
    };
    if targets.is_empty() {
        return;
    }

    let mut candidates: Vec<(EntityId, u32)> = world
        .query::<SceneAliasCandidate>()
        .map(|candidates| {
            candidates
                .iter()
                .map(|(entity, candidate)| (entity, candidate.base_form_id))
                .collect()
        })
        .unwrap_or_default();
    if let Some(remote_stubs) = world.query::<byroredux_scripting::RemoteSceneActorStub>() {
        candidates.retain(|(entity, _)| !remote_stubs.contains(*entity));
    }
    let Some(transforms) = world.query::<Transform>() else {
        return;
    };
    let mut movements = Vec::new();
    let mut arrivals = Vec::new();
    for (base_form_id, target_stage, trigger, destination) in targets {
        let actor = candidates
            .iter()
            .filter(|(_, candidate_base)| *candidate_base == base_form_id)
            .filter_map(|(entity, _)| {
                let transform = transforms.get(*entity)?;
                Some((*entity, transform.translation.distance_squared(destination)))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(entity, _)| entity);
        let Some(actor) = actor else {
            continue;
        };
        let Some(transform) = transforms.get(actor) else {
            continue;
        };
        let delta = destination - transform.translation;
        let distance = delta.length();
        if distance <= f32::EPSILON {
            arrivals.push((trigger, actor));
            continue;
        }
        let step = (SCENE_TRIGGER_APPROACH_SPEED * dt.max(0.0)).min(distance);
        if step >= distance {
            log::debug!(
                "scene trigger approach actor {actor} base=0x{base_form_id:08X} arrived at trigger {trigger} target stage {target_stage}"
            );
            arrivals.push((trigger, actor));
        }
        movements.push((actor, transform.translation + delta / distance * step));
    }
    drop(transforms);
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for (actor, translation) in &movements {
            if let Some(transform) = transforms.get_mut(*actor) {
                transform.translation = *translation;
            }
        }
    }
    if world.try_resource::<PhysicsWorld>().is_some() {
        for (actor, translation) in movements {
            byroredux_physics::set_kinematic_translation(world, actor, translation);
        }
    }
    if !arrivals.is_empty() {
        if let Some(mut events) = world.query_mut::<byroredux_scripting::OnTriggerEnterEvent>() {
            for (trigger, actor) in arrivals {
                if let Some(event) = events.get_mut(trigger) {
                    if !event.triggerers.contains(&actor) {
                        event.triggerers.push(actor);
                    }
                } else {
                    events.insert(
                        trigger,
                        byroredux_scripting::OnTriggerEnterEvent {
                            triggerers: vec![actor],
                        },
                    );
                }
            }
        }
    }
}

/// Resolve MQ101's complete attachment chain: package-driven horse -> tethered
/// cart -> `SetVehicle` riders. This runs in Update before transform
/// propagation, so carts, riders, and their skeletons observe the new root
/// poses in the same frame.
pub(crate) fn vehicle_attachment_system(world: &World, _dt: f32) {
    // Creation routes mounted actors' package locomotion through their mount.
    // `scene_package_system` has already advanced the rider root this tick;
    // solve the vehicle pose that preserves the captured attachment offset
    // before the ordinary vehicle->rider propagation below.
    let package_movers: std::collections::HashSet<EntityId> = world
        .query::<ScenePackagePlayback>()
        .map(|query| {
            query
                .iter()
                .flat_map(|(_, playback)| &playback.active_actions)
                .filter(|action| matches!(action.command, ScenePackageCommand::MoveTo { .. }))
                .map(|action| action.actor)
                .collect()
        })
        .unwrap_or_default();
    let driven_vehicles: Vec<_> = match (
        world.query::<ActorCinematicState>(),
        world.query::<Transform>(),
    ) {
        (Some(states), Some(transforms)) => states
            .iter()
            .filter(|(actor, _)| package_movers.contains(actor))
            .filter_map(|(actor, state)| {
                let vehicle_entity = state.vehicle?;
                let rider = transforms.get(actor)?;
                let vehicle = transforms.get(vehicle_entity)?;
                let local_translation = state.vehicle_local_translation?;
                let local_rotation = state.vehicle_local_rotation?;
                let rotation = rider.rotation * local_rotation.inverse();
                let translation =
                    rider.translation - rotation * (local_translation * vehicle.scale);
                Some((vehicle_entity, translation, rotation))
            })
            .collect(),
        _ => Vec::new(),
    };
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for (vehicle, translation, rotation) in &driven_vehicles {
            if let Some(transform) = transforms.get_mut(*vehicle) {
                transform.translation = *translation;
                transform.rotation = *rotation;
            }
        }
    }

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
        world.insert_resource(byroredux_scripting::CinematicPresentationState::default());
        world.insert_resource(byroredux_scripting::quest_stages::QuestStageState::default());
        let quest = byroredux_scripting::QuestFormId(0x0003_372B);
        world
            .resource_mut::<byroredux_scripting::CinematicPresentationState>()
            .register_player_animation_event(
                CinematicAnimationEvent::IdleFurnitureExit,
                quest,
                Vec::new(),
            );

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
        let presentation = world.resource::<byroredux_scripting::CinematicPresentationState>();
        assert!(!presentation
            .is_player_animation_event_registered(CinematicAnimationEvent::IdleFurnitureExit));
        assert_eq!(
            presentation.last_player_animation_event,
            Some(CinematicAnimationEvent::IdleFurnitureExit)
        );
        assert_eq!(presentation.player_animation_event_serial, 1);
        drop(presentation);
        assert_eq!(
            world
                .resource::<byroredux_scripting::quest_stages::QuestStageState>()
                .get_stage(quest),
            160
        );
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
    fn mounted_scene_package_movement_drives_vehicle_before_attachment() {
        use byroredux_core::math::{Quat, Vec3};
        use byroredux_scripting::ActiveScenePackageAction;

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<ActorCinematicState>();
        world.register::<ScenePackagePlayback>();
        let vehicle = world.spawn();
        let rider = world.spawn();
        let scene = world.spawn();
        world.insert(vehicle, Transform::IDENTITY);
        world.insert(
            rider,
            Transform::from_translation(Vec3::new(10.0, 2.0, 0.0)),
        );
        world.insert(
            rider,
            ActorCinematicState {
                vehicle: Some(vehicle),
                vehicle_local_translation: Some(Vec3::new(0.0, 2.0, 0.0)),
                vehicle_local_rotation: Some(Quat::IDENTITY),
                ..Default::default()
            },
        );
        world.insert(
            scene,
            ScenePackagePlayback {
                active_actions: vec![ActiveScenePackageAction {
                    scene_form_id: 1,
                    action_index: 2,
                    actor: rider,
                    package_candidates: vec![3],
                    package_form_id: 3,
                    template_form_id: 4,
                    command: ScenePackageCommand::MoveTo {
                        procedure_type: "Travel".to_owned(),
                        destination: Vec3::new(100.0, 0.0, 0.0),
                        arrival_radius: 1.0,
                        stall_seconds: 0.0,
                    },
                }],
            },
        );

        vehicle_attachment_system(&world, 0.0);

        assert_eq!(
            world.get::<Transform>(vehicle).unwrap().translation,
            Vec3::new(10.0, 0.0, 0.0)
        );
        assert_eq!(
            world.get::<Transform>(rider).unwrap().translation,
            Vec3::new(10.0, 2.0, 0.0)
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
                route_target_form_id: None,
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

    #[test]
    fn tethered_horse_advances_through_authored_linked_reference_route() {
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<HorseTetherState>();
        world.register::<SceneAliasCandidate>();
        byroredux_scripting::install_package_target_positions(
            &mut world,
            [
                (0x100, Vec3::new(5.0, 20.0, 0.0)),
                (0x101, Vec3::new(100.0, 0.0, 0.0)),
            ],
        );
        byroredux_scripting::install_package_linked_references(
            &mut world,
            [(0x100, vec![(0, 0x101)])],
        );
        byroredux_scripting::install_package_target_directions(&mut world, [(0x101, Vec3::X)]);

        let horse = world.spawn();
        let cart = world.spawn();
        world.insert(horse, Transform::IDENTITY);
        world.insert(
            horse,
            SceneAliasCandidate {
                reference_form_id: 0x90,
                base_form_id: 0x91,
                linked_refs: vec![(0, 0x100)],
                location_ref_types: Vec::new(),
            },
        );
        world.insert(
            cart,
            HorseTetherState {
                horse,
                horse_local_translation: Vec3::ZERO,
                horse_local_rotation: Quat::IDENTITY,
                route_target_form_id: None,
            },
        );

        cinematic_horse_route_system(&world, 0.1);
        assert_eq!(
            world
                .get::<HorseTetherState>(cart)
                .unwrap()
                .route_target_form_id,
            Some(0x101)
        );
        assert!((world.get::<Transform>(horse).unwrap().translation.x - 5.0).abs() < 1e-5);
        assert!((world.get::<Transform>(horse).unwrap().translation.y - 20.0).abs() < 1e-5);

        cinematic_horse_route_system(&world, 0.1);
        assert!((world.get::<Transform>(horse).unwrap().translation.x - 15.0).abs() < 1e-5);

        for _ in 0..50 {
            cinematic_horse_route_system(&world, 0.1);
        }
        assert!(
            world.get::<Transform>(horse).unwrap().translation.x > 100.0,
            "terminal continuation follows the marker's authored heading"
        );
    }

    #[test]
    fn awaited_actor_trigger_moves_loaded_matching_base_not_remote_stub() {
        use byroredux_core::math::{Quat, Vec3};
        use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
        use byroredux_plugin::esm::records::{ScenRecord, ScenePhase};
        use byroredux_scripting::papyrus_demo::quest_advance::{
            ActivatorGate, QuestAdvanceOnActivate,
        };
        use byroredux_scripting::{
            QuestFormId, RemoteSceneActorStub, ScenePlaybackState, TriggerShape, TriggerVolume,
        };

        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.insert_resource(byroredux_scripting::quest_stages::QuestStageState::default());
        world.register::<Transform>();
        world.register::<QuestAdvanceOnActivate>();

        let quest = QuestFormId(0x3372B);
        byroredux_scripting::install_scene_records(
            &mut world,
            [ScenRecord {
                form_id: 0xBECD4,
                quest_form_id: Some(quest.0),
                phases: vec![ScenePhase {
                    completion_conditions: vec![Condition {
                        function_index: 59,
                        comparator: ComparisonOp::Eq,
                        comparand: ConditionValue::Literal(1.0),
                        param_1: quest.0,
                        param_2: 22,
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        );
        let scene = world
            .resource::<byroredux_scripting::SceneRegistry>()
            .scene_entity(0xBECD4)
            .unwrap();
        {
            let player = world
                .get_mut::<byroredux_scripting::ScenePlayer>(scene)
                .unwrap();
            player.state = ScenePlaybackState::Playing;
        }

        let trigger = world.spawn();
        world.insert(
            trigger,
            TriggerVolume {
                center: Vec3::new(1_000.0, 0.0, 0.0),
                half_extents: Vec3::splat(100.0),
                rotation: Quat::IDENTITY,
                shape: TriggerShape::Box,
                occupant_inside: None,
            },
        );
        world.insert(
            trigger,
            QuestAdvanceOnActivate {
                owning_quest: quest,
                conditions: Vec::new(),
                target_stage: 22,
                activator_gate: ActivatorGate::BaseForm(0x654E5),
                disable_after_advance: true,
            },
        );
        byroredux_scripting::papyrus_demo::quest_advance::install_quest_trigger_approach(
            &mut world,
            0x84058,
            Vec3::new(0.0, 0.0, 1_000.0),
            QuestAdvanceOnActivate {
                owning_quest: quest,
                conditions: Vec::new(),
                target_stage: 20,
                activator_gate: ActivatorGate::BaseForm(0x654E5),
                disable_after_advance: true,
            },
        );
        byroredux_scripting::papyrus_demo::quest_advance::install_quest_trigger_approach(
            &mut world,
            0x84059,
            Vec3::new(0.0, 0.0, 1_000.0),
            QuestAdvanceOnActivate {
                owning_quest: quest,
                conditions: Vec::new(),
                target_stage: 35,
                activator_gate: ActivatorGate::BaseForm(0x654E5),
                disable_after_advance: true,
            },
        );
        byroredux_scripting::papyrus_demo::quest_advance::install_quest_trigger_approach(
            &mut world,
            0x84060,
            Vec3::new(0.0, 0.0, 1_000.0),
            QuestAdvanceOnActivate {
                owning_quest: quest,
                conditions: Vec::new(),
                target_stage: 25,
                activator_gate: ActivatorGate::BaseForm(0x654E5),
                disable_after_advance: true,
            },
        );
        byroredux_scripting::papyrus_demo::quest_advance::install_quest_trigger_approach(
            &mut world,
            0x84061,
            Vec3::new(1_000.0, 0.0, 0.0),
            QuestAdvanceOnActivate {
                owning_quest: quest,
                conditions: Vec::new(),
                target_stage: 32,
                activator_gate: ActivatorGate::BaseForm(0xB9E1D),
                disable_after_advance: true,
            },
        );

        let loaded = world.spawn();
        world.insert(loaded, Transform::IDENTITY);
        world.insert(
            loaded,
            SceneAliasCandidate {
                reference_form_id: 0x654E1,
                base_form_id: 0x654E5,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        let remote = world.spawn();
        world.insert(
            remote,
            Transform::from_translation(Vec3::new(900.0, 0.0, 0.0)),
        );
        world.insert(
            remote,
            SceneAliasCandidate {
                reference_form_id: 0x198BA,
                base_form_id: 0x654E5,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        world.insert(remote, RemoteSceneActorStub);
        let next_actor = world.spawn();
        world.insert(next_actor, Transform::IDENTITY);
        world.insert(
            next_actor,
            SceneAliasCandidate {
                reference_form_id: 0xB9DF2,
                base_form_id: 0xB9E1D,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );

        scene_trigger_actor_approach_system(&world, 0.1);

        assert_eq!(
            world.get::<Transform>(loaded).unwrap().translation,
            Vec3::new(0.0, 0.0, 50.0),
            "the lowest ready prerequisite stage is approached before the awaited stage"
        );
        assert_eq!(
            world.get::<Transform>(remote).unwrap().translation,
            Vec3::new(900.0, 0.0, 0.0),
            "synthetic offscreen stubs must not win nearest-actor selection"
        );

        {
            let mut stages =
                world.resource_mut::<byroredux_scripting::quest_stages::QuestStageState>();
            stages.start_quest(quest, Some(0));
            stages.set_stage(quest, 20);
            stages.set_stage(quest, 22);
            stages.set_stage(quest, 26);
        }
        world
            .get_mut::<byroredux_scripting::ScenePlayer>(scene)
            .unwrap()
            .state = ScenePlaybackState::Finished;

        scene_trigger_actor_approach_system(&world, 0.1);

        assert_eq!(
            world.get::<Transform>(loaded).unwrap().translation,
            Vec3::new(0.0, 0.0, 50.0),
            "stale stage 25 and later stage 35 must not move in parallel"
        );
        assert_eq!(
            world.get::<Transform>(next_actor).unwrap().translation,
            Vec3::new(50.0, 0.0, 0.0),
            "a running quest must approach only its globally next ready trigger between scenes"
        );
    }

    /// #3446 — `StringPool` is the sink at the tail of
    /// `docs/engine/ecs.md`'s canonical acquisition order: nothing is ever
    /// acquired beneath it. This system used to take the pool *before* its
    /// `AnimationTextKeyEvents` query, recording an edge out of the sink
    /// and demoting it to a mid-graph node. Which order the two guards are
    /// taken in is only visible in source (both are reads, so no runtime
    /// signal distinguishes them until a reverse edge appears elsewhere and
    /// `BYRO_LOCK_ORDER_CHECK=1` aborts), so pin it here.
    #[test]
    fn text_key_query_is_acquired_before_the_string_pool() {
        const CINEMATIC_RS: &str = include_str!("cinematic.rs");

        let body = CINEMATIC_RS
            .split_once("pub(crate) fn cinematic_animation_event_system")
            .expect("cinematic_animation_event_system definition")
            .1;
        let query = body
            .find("world.query::<AnimationTextKeyEvents>()")
            .expect("the AnimationTextKeyEvents query acquisition");
        let pool = body
            .find("world.try_resource::<StringPool>()")
            .expect("the StringPool acquisition");
        assert!(
            query < pool,
            "cinematic_animation_event_system acquires StringPool before its \
             AnimationTextKeyEvents query — that records an edge out of the \
             canonical order's sink (#3446); see docs/engine/ecs.md",
        );
    }
}
