//! App-side sinks for scripted cinematic requests.

use crate::components::{AnimationTarget, CinematicReAdoption, CellRootIndex, IdleClipCatalog};
use byroredux_core::ecs::components::{CellRoot, Parent};
use byroredux_core::animation::{AnimationPlayer, RootMotionDelta};
use byroredux_core::ecs::components::RigidBodyData;
use byroredux_core::ecs::Transform;
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::coord::zup_to_yup_pos;
use byroredux_core::string::StringPool;
use byroredux_physics::{PhysicsWorld, RapierHandles};
use byroredux_scripting::{
    ActorCinematicState, AnimationTextKeyEvents, CinematicAnimationEvent, HorseTetherState,
    MotionTypeChangeRequest, PackageTargetRegistry, PlayingScriptedIdle, SceneAliasCandidate,
    ScenePackageCommand, ScenePackagePlayback,
};

const SCENE_TRIGGER_APPROACH_SPEED: f32 = 500.0;

/// Consume queued Skyrim `PlayIdle` requests once their IDLE FormID has a
/// decoded HKX clip. Unresolved requests remain pending, allowing a later cell
/// load to install the relevant archive without losing the authored request.
pub(crate) fn idle_clip_playback_system(world: &World, _dt: f32) {
    let requests: Vec<(EntityId, u64, EntityId, u32)> = {
        let Some(catalog) = world.try_resource::<IdleClipCatalog>() else {
            return;
        };
        // #4546 — AnimationTarget before ActorCinematicState: walk_anim's
        // read pass holds AnimationTarget while acquiring ACState, so the
        // reverse order here closed a cross-thread cycle
        // (ACState → AnimationTarget vs AnimationTarget → … → ACState)
        // once the Transform cycle below was broken.
        let Some(targets) = world.query::<AnimationTarget>() else {
            return;
        };
        let Some(states) = world.query::<ActorCinematicState>() else {
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

    // The idle the actor was in, for the one-shots that hand it back (see `idle_resume_system`).
    let mut resumes: Vec<(EntityId, crate::components::IdleResume)> = Vec::new();
    // #6208 — resolve which requested clips are one-shots under one registry read, dropped before the
    // `AnimationPlayer` write guard: `animation_system_inner` holds the registry outermost and then takes
    // the player/stack storages (#2400), so reading the registry under `players` closes a cycle.
    let one_shots: Vec<bool> = {
        let clips = world.try_resource::<byroredux_core::animation::AnimationClipRegistry>();
        requests
            .iter()
            .map(|(_, _, _, handle)| {
                clips
                    .as_ref()
                    .and_then(|clips| clips.get(*handle))
                    .is_some_and(|clip| clip.cycle_type == byroredux_core::animation::CycleType::Clamp)
            })
            .collect()
    };
    if let Some(mut players) = world.query_mut::<AnimationPlayer>() {
        for ((actor, _, skeleton_root, handle), &one_shot) in requests.iter().zip(&one_shots) {
            let replacement = AnimationPlayer::new(*handle).with_root(*skeleton_root);
            if let Some(player) = players.get_mut(*actor) {
                if player.clip_handle != *handle && one_shot {
                    resumes.push((
                        *actor,
                        crate::components::IdleResume {
                            idle_clip: *handle,
                            clip: Some(player.clip_handle),
                            speed: player.speed,
                            local_time: player.local_time,
                        },
                    ));
                }
                *player = replacement;
            } else {
                // It was playing nothing: when the one-shot ends the player goes again.
                if one_shot {
                    resumes.push((
                        *actor,
                        crate::components::IdleResume { idle_clip: *handle, clip: None, speed: 1.0, local_time: 0.0 },
                    ));
                }
                players.insert(*actor, replacement);
            }
        }
    } else {
        return;
    }
    if !resumes.is_empty() {
        let playing: Vec<EntityId> = resumes.iter().map(|(actor, _)| *actor).collect();
        if let Some(mut store) = world.query_mut::<crate::components::IdleResume>() {
            for (actor, resume) in resumes {
                // A second one-shot over the first keeps the idle from before the first.
                match store.get_mut(actor) {
                    Some(existing) => existing.idle_clip = resume.idle_clip,
                    None => {
                        store.insert(actor, resume);
                    }
                }
            }
        }
        // ObScript `IsIdlePlaying` reads this marker; it lives exactly as long as the `IdleResume`
        // (taken after the store above is released, so the two locks are never nested).
        if let Some(mut marks) = world.query_mut::<PlayingScriptedIdle>() {
            for actor in playing {
                marks.insert(actor, PlayingScriptedIdle);
            }
        }
    }
    if let Some(mut root_motion) = world.query_mut::<RootMotionDelta>() {
        for (actor, _, _, _) in &requests {
            root_motion.insert(*actor, RootMotionDelta(byroredux_core::math::Vec3::ZERO));
        }
    }
    if let Some(mut targets) = world.query_mut::<AnimationTarget>() {
        for (actor, serial, _, _) in requests {
            if let Some(target) = targets.get_mut(actor) {
                target.consumed_idle_serial = serial;
            }
        }
    }
}

/// ObScript `PickIdle`: the actor picks a new idle. The engine's actor idle choice is the one it was
/// spawned with (`SpawnIdle`: a creature's own clip, a humanoid's from the generic pool — #6231's rule,
/// since a creature's rig binds none of the humanoid idle's bones); the actor starts it again from the
/// first key, unless a scripted one-shot idle is running (which hands the idle back itself when it
/// ends). An actor with no spawn idle or no animation to restart is left alone.
pub(crate) fn pick_idle_system(world: &World, _dt: f32) {
    let requested: Vec<EntityId> =
        super::take_requests::<byroredux_scripting::PickIdleRequest>(world).into_iter().map(|(actor, _)| actor).collect();
    if requested.is_empty() {
        return;
    }
    let resting: Vec<EntityId> = match world.query::<crate::components::IdleResume>() {
        Some(running) => requested.into_iter().filter(|actor| running.get(*actor).is_none()).collect(),
        None => requested,
    };
    let resting: Vec<(EntityId, u32)> = resting
        .into_iter()
        .filter_map(|actor| Some((actor, world.get::<crate::npc_spawn::SpawnIdle>(actor)?.clip)))
        .collect();
    let Some(mut players) = world.query_mut::<AnimationPlayer>() else { return };
    for (actor, clip) in resting {
        if let Some(player) = players.get_mut(actor) {
            player.clip_handle = clip;
            player.local_time = 0.0;
            player.prev_time = 0.0;
            player.playing = true;
            player.reverse_direction = false;
        }
    }
}

/// Hand an actor back the animation it was in when a scripted one-shot IDLE (`PlayIdle`) has finished:
/// the `Clamp` clip ran to its duration. A player given something else in the meantime forgets the
/// resume.
pub(crate) fn idle_resume_system(world: &World, _dt: f32) {
    let resumable: Vec<(EntityId, crate::components::IdleResume)> = match world.query::<crate::components::IdleResume>() {
        Some(resumes) if !resumes.is_empty() => resumes.iter().map(|(actor, resume)| (actor, *resume)).collect(),
        _ => return,
    };
    // #6208 — each resume's one-shot duration, read before the `AnimationPlayer` write guard (see
    // `idle_clip_playback_system`). Only a player still on `idle_clip` is tested, so that is the clip to
    // look up; a clip (or registry) that is gone counts as finished.
    let durations: Vec<Option<f32>> = {
        let clips = world.try_resource::<byroredux_core::animation::AnimationClipRegistry>();
        resumable
            .iter()
            .map(|(_, resume)| clips.as_ref().and_then(|clips| clips.get(resume.idle_clip)).map(|clip| clip.duration))
            .collect()
    };
    let mut done: Vec<EntityId> = Vec::new();
    let mut finished_idle: Vec<EntityId> = Vec::new();
    if let Some(mut players) = world.query_mut::<AnimationPlayer>() {
        for ((actor, resume), duration) in resumable.iter().zip(&durations) {
            let Some(player) = players.get_mut(*actor) else {
                done.push(*actor);
                continue;
            };
            if player.clip_handle != resume.idle_clip {
                done.push(*actor);
            } else if duration.is_none_or(|duration| player.local_time >= duration) {
                match resume.clip {
                    Some(clip) => {
                        let root = player.root_entity;
                        let mut back = AnimationPlayer::new(clip);
                        back.root_entity = root;
                        back.speed = resume.speed;
                        back.local_time = resume.local_time;
                        back.prev_time = resume.local_time;
                        *player = back;
                    }
                    None => finished_idle.push(*actor),
                }
                done.push(*actor);
            }
        }
        for actor in finished_idle {
            players.remove(actor);
        }
    }
    if let Some(mut store) = world.query_mut::<crate::components::IdleResume>() {
        for actor in &done {
            store.remove(*actor);
        }
    }
    if let Some(mut marks) = world.query_mut::<PlayingScriptedIdle>() {
        for actor in done {
            marks.remove(actor);
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
        // #4546 — Transform before ActorCinematicState. Transform is
        // canonical-early in the documented acquisition order
        // (`docs/engine/ecs.md`), and this system held the ACState read
        // while taking the Transform read, observing the ACState →
        // Transform edge that closed the reported cross-thread cycle
        // against walk_anim's Transform → AnimationPlayer → ACState.
        let transforms = world.query::<Transform>();
        let Some(states) = world.query::<ActorCinematicState>() else {
            return;
        };
        let Some(root_motion) = world.query::<RootMotionDelta>() else {
            return;
        };
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

    let Some(player) = byroredux_scripting::papyrus_demo::PapyrusPlayerEntity::get(world)
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

/// How many parents up a collision entity is searched for the reference it belongs to.
const MAX_BODY_OWNER_HOPS: usize = 16;

/// The entities that carry a rigid body for the reference `root`: its own, those of
/// entities beneath it, and the bare collision entities its REFR owns (their
/// `PhysicsSourceForm` names it). A script's `SetMotionType` / `ApplyHavokImpulse` on
/// a placed reference acts on these (the CK: the reference's collision), not on the
/// placement root, which carries no body.
pub(crate) fn reference_body_entities(world: &World, root: EntityId) -> Vec<EntityId> {
    let form = world.get::<byroredux_core::ecs::components::FormIdComponent>(root).map(|id| id.0);
    let candidates: Vec<EntityId> = match world.query::<RigidBodyData>() {
        Some(bodies) => bodies.iter().map(|(entity, _)| entity).collect(),
        None => return Vec::new(),
    };
    candidates
        .into_iter()
        .filter(|&entity| {
            if entity == root {
                return true;
            }
            let mut at = entity;
            for _ in 0..MAX_BODY_OWNER_HOPS {
                let Some(parent) = world.get::<byroredux_core::ecs::Parent>(at).map(|parent| parent.0) else {
                    break;
                };
                if parent == root {
                    return true;
                }
                at = parent;
            }
            form.is_some_and(|form| {
                world
                    .get::<byroredux_core::ecs::components::PhysicsSourceForm>(entity)
                    .is_some_and(|source| source.0 == form)
            })
        })
        .collect()
}

/// Apply Papyrus `SetMotionType` requests to both canonical ECS body data and
/// an already-live Rapier body, then drain the one-shot request.
pub(crate) fn scripted_motion_type_system(world: &World, _dt: f32) {
    let asked = super::take_requests::<MotionTypeChangeRequest>(world);
    if asked.is_empty() {
        return;
    }
    // A reference's request acts on the bodies it owns (see `reference_body_entities`).
    let mut targets: Vec<(EntityId, MotionTypeChangeRequest)> = Vec::new();
    for (entity, request) in &asked {
        let bodies = reference_body_entities(world, *entity);
        if bodies.is_empty() {
            targets.push((*entity, *request));
        }
        targets.extend(bodies.into_iter().map(|body| (body, *request)));
    }
    let requests: Vec<(EntityId, MotionTypeChangeRequest, Option<RapierHandles>)> = {
        let handles_q = world.query::<RapierHandles>();
        targets
            .iter()
            .map(|(entity, request)| {
                (*entity, *request, handles_q.as_ref().and_then(|handles| handles.get(*entity).copied()))
            })
            .collect()
    };

    if let Some(mut bodies) = world.query_mut::<RigidBodyData>() {
        for (entity, request, _) in &requests {
            if let Some(body) = bodies.get_mut(*entity) {
                body.motion_type = request.motion_type;
            }
        }
    }

    if let Some(mut physics) = world.try_resource_mut::<PhysicsWorld>() {
        for (entity, request, handles) in &requests {
            if let Some(handles) = handles {
                physics.set_motion_type(handles.body, request.motion_type, request.allow_activate);
            } else {
                log::debug!("SetMotionType({:?}) on entity {entity}: no live body", request.motion_type);
            }
        }
    }
}

/// Skyrim's Havok scale (Bethesda units per Havok metre), as the NIF importer
/// applies it to collision geometry.
const SKYRIM_HAVOK_SCALE: f32 = 69.99125;

/// Apply the `ApplyHavokImpulse` requests scripts queued: each reference's summed
/// `direction × magnitude` (Z-up game coordinates), times the game's gameplay
/// impulse scale and the multiplier of the body's Havok layer
/// (`byroredux_scripting::havok_impulse`), as a linear impulse on its Rapier body
/// — Y-up, in Bethesda units. A reference with no live dynamic body is the CK's
/// "no 3D" failure: the request is dropped. One-shot, drained here.
pub(crate) fn scripted_havok_impulse_system(world: &World, _dt: f32) {
    let asked = super::take_requests::<byroredux_scripting::HavokImpulseRequest>(world);
    if asked.is_empty() {
        return;
    }
    // Without the rules there is no engine factor to apply: the requests are
    // dropped, not left to be re-resolved against every rigid body each frame
    // until the resource appears (#6009).
    let Some(rules) = world.try_resource::<byroredux_scripting::HavokImpulseRules>().map(|rules| *rules) else {
        return;
    };
    // The impulse acts on each body of the reference's collision.
    let mut requests: Vec<(EntityId, byroredux_scripting::HavokImpulseRequest)> = Vec::new();
    for (entity, request) in &asked {
        let bodies = reference_body_entities(world, *entity);
        if bodies.is_empty() {
            requests.push((*entity, *request));
        }
        requests.extend(bodies.into_iter().map(|body| (body, *request)));
    }
    let bodies: Vec<(Option<RapierHandles>, u8)> = {
        let handles_q = world.query::<RapierHandles>();
        let data_q = world.query::<RigidBodyData>();
        requests
            .iter()
            .map(|(entity, _)| {
                (
                    handles_q.as_ref().and_then(|handles| handles.get(*entity).copied()),
                    data_q.as_ref().and_then(|data| data.get(*entity).map(|body| body.havok_layer)).unwrap_or(0),
                )
            })
            .collect()
    };
    if let Some(mut physics) = world.try_resource_mut::<PhysicsWorld>() {
        for ((entity, request), (handles, layer)) in requests.iter().zip(&bodies) {
            let Some(handles) = handles else {
                log::debug!("ApplyHavokImpulse on entity {entity}: no physics body, skipped");
                continue;
            };
            let factor = rules.engine_factor(*layer, SKYRIM_HAVOK_SCALE);
            let impulse = byroredux_core::math::Vec3::from(zup_to_yup_pos(request.vector)) * factor;
            log::debug!("ApplyHavokImpulse on entity {entity}: {impulse:?} (layer {layer})");
            physics.apply_impulse(handles.body, impulse);
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
        bool,
    )> = {
        // #4546 — Transform (canonical-early) before HorseTetherState:
        // vehicle_attachment_system observes Transform → ACState →
        // …, so this system's old HorseTetherState → Transform read
        // closed the three-hop cycle the checker reported.
        let Some(transforms) = world.query::<Transform>() else {
            return;
        };
        let Some(tethers) = world.query::<HorseTetherState>() else {
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
                    false,
                ))
            })
            .collect()
    };
    if pending.is_empty() {
        return;
    }

    let decisions: Vec<_> = pending
        .into_iter()
        .filter_map(|(cart, horse, target, current, rotation, _arrived_terminal)| {
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
                super::locomotion::step_toward(
                    current,
                    rotation,
                    target_xz,
                    dt,
                    super::locomotion::LOCOMOTION_WALK_SPEED,
                    None,
                );
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
            // #3817 — the tether's authored route is exhausted: the horse
            // has arrived at a marker with no further XLKR link (its
            // destination is the terminal marker's heading extension).
            // This is where vanilla's opening convoy stops, and it is the
            // only authored signal the engine has for "the drive is over"
            // — releasing here is what keeps a tethered cart from being
            // retained (and un-reclaimable) for the rest of the session.
            let arrived_terminal = remaining.length_squared()
                <= super::locomotion::LOCOMOTION_ARRIVAL_EPSILON
                    * super::locomotion::LOCOMOTION_ARRIVAL_EPSILON
                && routes.linked_reference(target).is_none();
            Some((cart, horse, translation, new_rotation, next_target, arrived_terminal))
        })
        .collect();
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for (_, horse, translation, rotation, _, _) in &decisions {
            if let Some(transform) = transforms.get_mut(*horse) {
                transform.translation = *translation;
                if let Some(rotation) = rotation {
                    transform.rotation = *rotation;
                }
            }
        }
    }
    if let Some(mut tethers) = world.query_mut::<HorseTetherState>() {
        for (cart, _, _, _, next_target, _) in &decisions {
            if let Some(tether) = tethers.get_mut(*cart) {
                tether.route_target_form_id = Some(*next_target);
            }
        }
    }
    if world.try_resource::<PhysicsWorld>().is_some() {
        for (_, horse, translation, _, _, arrived_terminal) in &decisions {
            if !arrived_terminal {
                // A convoy parked at its terminal never moves again; the
                // per-tick kinematic re-pin stops with it.
                byroredux_physics::set_kinematic_translation(world, *horse, *translation);
            }
        }
    }
    let terminal_arrivals: Vec<(EntityId, EntityId)> = decisions
        .iter()
        .filter(|(_, _, _, _, _, arrived)| *arrived)
        .map(|(cart, horse, _, _, _, _)| (*cart, *horse))
        .collect();
    release_finished_tethers(world, &terminal_arrivals);
}

/// #3817 — end a tether whose authored XLKR route is exhausted, and
/// queue the formerly-retained convoy for cell re-adoption.
///
/// `cinematic_retained_entities` (cell_loader/unload.rs) keeps a cart, its
/// horse, its riders and their whole render subtrees out of cell teardown
/// for as long as a `HorseTetherState` / attached `ActorCinematicState`
/// exists — and neither state was ever removed in production, so once a
/// convoy's home cell unloaded mid-tether (the #3254-scoped strip), the
/// cart, horse, every rider and all their GPU resources stayed resident
/// at their last transform across worldspace changes and interior
/// transitions, permanently. The route system is the one place that
/// *knows* the drive is over: the horse has arrived at the terminal
/// marker of its authored XLKR chain — the point where vanilla's own
/// convoy stops.
///
/// Release is behaviour-invisible at the terminal: horse and cart are
/// parked, so un-pinning the cart from the horse and the riders from the
/// cart leaves every transform exactly where it was. The cart keeps its
/// keyframed motion type (a parked cart cannot slide, matching vanilla's
/// settled cart), riders keep `cart_seat` / `vehicle_local_*` so a later
/// scripted exit animation still resolves. Release clears the rider's
/// `vehicle`, so `Effect::ExitCart` — which reads `vehicle` +
/// `vehicle_local_rotation` to derive the exit root-motion heading —
/// takes its fallback and uses the rider's own `Transform.rotation`. That
/// is the same heading only because `vehicle_attachment_system` last wrote
/// the rider's rotation as `vehicle.rotation * vehicle_local_rotation`;
/// `released_rider_exit_heading_matches_the_attached_heading` pins that
/// equivalence.
///
/// Re-adoption covers the half the #3254 fix deferred: an entity that
/// lost its `CellRoot` to a mid-tether home-cell unload has no owner and
/// no path back. Released entities without a `CellRoot` are queued on
/// the [`CinematicReAdoption`] pending list, and the streaming step's
/// retry (`retry_cinematic_readoption`) stamps each onto the root of
/// whatever loaded exterior cell contains it — an un-rooted entity is
/// despawn-immune to cell unload, so the pending list is what bounds
/// that population instead of a silent permanent residency.
fn release_finished_tethers(world: &World, finished: &[(EntityId, EntityId)]) {
    if finished.is_empty() {
        return;
    }
    let carts: Vec<EntityId> = finished.iter().map(|(cart, _)| *cart).collect();

    // ── Read pass 1 — riders attached to a finishing cart. ──
    let riders: Vec<(EntityId, EntityId)> = match world.query::<ActorCinematicState>() {
        Some(states) => states
            .iter()
            .filter_map(|(actor, state)| {
                let vehicle = state.vehicle?;
                carts.contains(&vehicle).then_some((actor, vehicle))
            })
            .collect(),
        None => Vec::new(),
    };

    // ── Read pass 2 — carts + horses + riders + their render subtrees. ──
    let mut seeds: Vec<EntityId> =
        finished.iter().copied().flat_map(|(cart, horse)| [cart, horse]).collect();
    seeds.extend(riders.iter().map(|(actor, _)| *actor));
    let (release_set, player_subtree) = {
        // #5513 — the shared guarded collector (visited set + budget),
        // one call per seed instead of a hand-rolled stack walk.
        let set: std::collections::HashSet<EntityId> = seeds
            .iter()
            .flat_map(|seed| byroredux_core::ecs::collect_subtree(world, *seed))
            .collect();
        // #5379 — the process-lifetime player is never cell-owned
        // (`player_body.rs`: the body root survives live cell reloads by
        // design). A player rider reaches this walk through its Children,
        // and queueing it would let `retry_cinematic_readoption` stamp a
        // `CellRoot` on it — after which the next streaming unload or
        // save-load teardown despawns the player and the session is left
        // with no body. Collect the player's own subtree here so pass 3
        // can leave every member out of the queue.
        let player = world
            .try_resource::<crate::systems::character::PlayerEntity>()
            .and_then(|player| player.0);
        let player_subtree = match player {
            Some(player) => byroredux_core::ecs::collect_subtree(world, player)
                .into_iter()
                .collect::<std::collections::HashSet<EntityId>>(),
            None => std::collections::HashSet::new(),
        };
        (set, player_subtree)
    };

    // ── Read pass 3 — who needs adoption. Entities still owned by their
    // (loaded) home cell need nothing; the rest are queued for the
    // streaming step's re-adoption retry, which resolves the loaded
    // exterior cell at each entity's position (`WorldStreamingState`
    // lives on the App, not in the ECS, so the route system cannot see
    // it — #3817). The player subtree never queues (#5379).
    let unplaced: Vec<EntityId> = {
        let roots = world.query::<CellRoot>();
        // #5384 — queue only PARENTLESS members: a subtree node's local
        // `Transform` cannot resolve a cell, so queueing it only ever
        // parked it in `pending` forever (or, pre-#5384, bound it to a
        // wrong origin cell). The root's adoption stamps the subtree.
        let parents = world.query::<Parent>();
        release_set
            .iter()
            .filter(|entity| {
                !player_subtree.contains(*entity)
                    && roots.as_ref().is_none_or(|roots| roots.get(**entity).is_none())
                    && parents.as_ref().is_none_or(|parents| parents.get(**entity).is_none())
            })
            .copied()
            .collect()
    };

    // ── Write pass — one storage at a time. ──
    if let Some(mut tethers) = world.query_mut::<HorseTetherState>() {
        for cart in &carts {
            if tethers.remove(*cart).is_some() {
                log::info!(
                    "cinematic tether on cart entity {cart} released: authored route terminal reached (#3817)"
                );
            }
        }
    }
    if !riders.is_empty() {
        if let Some(mut states) = world.query_mut::<ActorCinematicState>() {
            for (actor, vehicle) in &riders {
                if let Some(state) = states.get_mut(*actor) {
                    if state.vehicle == Some(*vehicle) {
                        state.vehicle = None;
                    }
                }
            }
        }
    }
    if !unplaced.is_empty() {
        log::info!(
            "{} released cinematic entit(y/ies) lack a cell owner — queued for re-adoption by the streaming step (#3817)",
            unplaced.len()
        );
        if let Some(mut pending) = world.try_resource_mut::<CinematicReAdoption>() {
            pending.pending.extend(unplaced);
        }
    }
}

/// Retry pass for [`CinematicReAdoption`]: entities whose release landed
/// outside every loaded cell get adopted the moment one loads beneath
/// them. Called from the streaming step (which owns both the world and
/// the loaded-cell map) — with an empty list (the normal session) it is
/// one resource read. Returns how many entities were adopted.
///
/// An un-rooted entity is despawn-immune to cell unload (nothing
/// enumerates it), so entries are dropped here only when the entity has
/// been despawned by some path outside cell teardown.
pub(crate) fn retry_cinematic_readoption(
    world: &mut World,
    loaded: &std::collections::HashMap<(i32, i32), crate::streaming::LoadedCell>,
) -> usize {
    let Some(pending) = world.try_resource::<CinematicReAdoption>() else {
        return 0;
    };
    if pending.pending.is_empty() {
        return 0;
    }

    // Read pass — partition into adopted / alive-still-pending / dead.
    // All storage guards are read-only here and drop before the writes
    // below (read-then-write on one storage is the lock-order hygiene
    // the ECS rules require).
    // #5384 — only PARENTLESS members resolve a cell: a subtree node's
    // `Transform` is local (an offset from its parent), so mapping it
    // through `world_pos_to_grid` sent render-subtree nodes to whatever
    // origin cell happened to be loaded (splitting the hierarchy across
    // cells) or left them pending forever — orphaned meshes that outlive
    // their root and survive its cell's unload. A parented node now
    // rides with its root: the root's adoption stamps the whole
    // `Children` subtree with the same `CellRoot`.
    let mut adoptions: Vec<(EntityId, EntityId, Vec<EntityId>)> = Vec::new();
    let mut still_pending: Vec<EntityId> = Vec::new();
    {
        let transforms = world.query::<Transform>();
        let roots = world.query::<CellRoot>();
        let parents = world.query::<Parent>();
        // #5379 — the process-lifetime player is never cell-owned, no
        // matter who queued it: a `CellRoot` stamp here would make the
        // next streaming unload / save-load teardown despawn the player.
        // Members of the player's subtree are consumed (dropped from the
        // pending list), not adopted.
        let player = world
            .try_resource::<crate::systems::character::PlayerEntity>()
            .and_then(|player| player.0);
        let player_subtree = match player {
            Some(player) => byroredux_core::ecs::collect_subtree(world, player)
                .into_iter()
                .collect::<std::collections::HashSet<EntityId>>(),
            None => std::collections::HashSet::new(),
        };
        for &entity in &pending.pending {
            if player_subtree.contains(&entity) {
                continue;
            }
            let Some(gt) = transforms.as_ref().and_then(|t| t.get(entity)) else {
                // Every convoy entity is a world entity with a Transform
                // (the render subtree included); one without is a despawn
                // from outside cell teardown — drop it.
                continue;
            };
            if roots.as_ref().is_some_and(|roots| roots.get(entity).is_some()) {
                continue; // adopted by an earlier tick
            }
            if parents.as_ref().is_some_and(|parents| parents.get(entity).is_some()) {
                // A subtree node cannot resolve a cell from a local
                // transform (#5384); its root carries it. Consumed, not
                // kept pending.
                log::debug!(
                    "cinematic re-adoption: entity {entity} is parented — \
                     riding its root's adoption (#5384)"
                );
                continue;
            }
            let (gx, gy) = crate::streaming::world_pos_to_grid(gt.translation.x, gt.translation.z);
            match loaded.get(&(gx, gy)) {
                Some(cell) => {
                    // The root's whole Children subtree lands with it.
                    // #5513 — the shared guarded collector (visited set +
                    // budget). The player-subtree filter is equivalent to
                    // the old stop-descent check: every descendant of a
                    // player-subtree node is itself a player-subtree
                    // member, so excluding members excludes their
                    // subtrees too.
                    let subtree = byroredux_core::ecs::collect_subtree(world, entity)
                        .into_iter()
                        .filter(|member| !player_subtree.contains(member))
                        .collect::<Vec<EntityId>>();
                    adoptions.push((entity, cell.cell_root, subtree));
                }
                None => still_pending.push(entity),
            }
        }
    }
    let adopted = adoptions.len();
    let dropped = pending
        .pending
        .len()
        .saturating_sub(adopted + still_pending.len());
    drop(pending);

    let mut adopted_entities = 0usize;
    for (_root_entity, root, subtree) in &adoptions {
        for member in subtree {
            world.insert(*member, CellRoot(*root));
            if let Some(mut idx) = world.try_resource_mut::<CellRootIndex>() {
                idx.map.entry(*root).or_default().push(*member);
            }
        }
        adopted_entities += subtree.len();
    }
    if adopted > 0 {
        log::info!(
            "cinematic re-adoption: {adopted} root entit(y/ies) + \
             {adopted_entities} subtree member(s) stamped onto loaded exterior \
             cell roots (#3817, #5384)"
        );
    }
    if dropped > 0 {
        log::info!(
            "cinematic re-adoption: {dropped} pending entit(y/ies) were despawned outside cell teardown and dropped (#3817)"
        );
    }
    if adopted > 0 || dropped > 0 {
        if let Some(mut pending) = world.try_resource_mut::<CinematicReAdoption>() {
            pending.pending = still_pending;
        }
    }
    adopted
}

/// Per-frame scratch for [`scene_trigger_actor_approach_system_inner`], owned
/// by the closure [`make_scene_trigger_actor_approach_system`] returns.
///
/// #3838 — the system used to build a fresh `Vec<ScenePlayer>` (a deep clone of
/// every player) plus two `HashSet`s every frame and drop them at tick end, on
/// any cell where a SCEN has ever played. Same shape, same fix as the AI-package
/// systems under #2033 / #3269 / #3353 and `make_animation_system` (#1372).
///
/// The clone itself stays: it exists to release the `ScenePlayer` storage read
/// lock before taking `SceneRegistry`, which is a lock-ordering requirement, not
/// an allocation one. Only the per-frame *allocation* is amortised away.
#[derive(Default)]
pub(crate) struct SceneTriggerApproachScratch {
    /// #4190 — the three scalar fields the consumer passes read,
    /// snapshotted per player instead of deep-cloning each ScenePlayer's
    /// unused `active_actions` / `completed_actions` heap collections
    /// every frame.
    players: Vec<ScenePlayerSnapshot>,
    active_quests: std::collections::HashSet<u32>,
    awaited: std::collections::HashSet<(u32, u16)>,
    between_scenes: std::collections::HashSet<u32>,
}

/// #4190 — the `Copy` projection of [`byroredux_scripting::ScenePlayer`]
/// the three consumer passes actually read: scene identity, playback
/// state, and the current phase index.
#[derive(Debug, Clone, Copy)]
struct ScenePlayerSnapshot {
    scene_form_id: u32,
    state: byroredux_scripting::ScenePlaybackState,
    current_phase: u32,
}

impl ScenePlayerSnapshot {
    fn is_running(&self) -> bool {
        matches!(
            self.state,
            byroredux_scripting::ScenePlaybackState::WaitingForStart
                | byroredux_scripting::ScenePlaybackState::WaitingForPhaseStart
                | byroredux_scripting::ScenePlaybackState::Playing
        )
    }
}

/// Build the scene-trigger approach system with persistent scratch.
pub(crate) fn make_scene_trigger_actor_approach_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = SceneTriggerApproachScratch::default();
    move |world: &World, dt: f32| {
        scene_trigger_actor_approach_system_inner(world, dt, &mut scratch);
    }
}

/// Bridge offscreen cinematic locomotion for a scene phase whose authored
/// completion explicitly waits on an actor-specific trigger stage. Creation
/// can drive a mounted actor outside ordinary cell AI; choose the nearest
/// loaded actor with the trigger's required base and approach the volume.
fn scene_trigger_actor_approach_system_inner(
    world: &World,
    dt: f32,
    scratch: &mut SceneTriggerApproachScratch,
) {
    use byroredux_scripting::papyrus_demo::quest_advance::{ActivatorGate, QuestAdvanceOnActivate};

    // Destructured up front so the four buffers are disjoint borrows: the
    // `extend` calls below read one field while writing another.
    let SceneTriggerApproachScratch {
        players,
        active_quests,
        awaited,
        between_scenes,
    } = scratch;

    players.clear();
    {
        let Some(query) = world.query::<byroredux_scripting::ScenePlayer>() else {
            return;
        };
        players.extend(query.iter().map(|(_, player)| ScenePlayerSnapshot {
            scene_form_id: player.scene_form_id,
            state: player.state,
            current_phase: player.current_phase,
        }));
    }
    let Some(registry) = world.try_resource::<byroredux_scripting::SceneRegistry>() else {
        return;
    };
    active_quests.clear();
    active_quests.extend(
        players
            .iter()
            .filter(|player| player.is_running())
            .filter_map(|player| registry.definition(player.scene_form_id)?.quest_form_id),
    );
    awaited.clear();
    awaited.extend(
        players
            .iter()
            .filter(|player| player.is_running())
            .filter_map(|player| {
                let scene = registry.definition(player.scene_form_id)?;
                let phase = scene.phases.get(player.current_phase as usize)?;
                Some((scene.quest_form_id, phase))
            })
            .flat_map(|(scene_quest, phase)| {
                phase
                    .completion_conditions
                    .iter()
                    .map(move |condition| (scene_quest, condition))
            })
            // #3954 — the shared recogniser also enforces that the scene
            // authoring the wait belongs to the quest it waits on. Without
            // that filter this collected `GetStageDone(Q, S)` waits from a
            // scene owned by some *other* quest and routed Q's actor at a
            // trigger the gate would then refuse.
            .filter_map(|(scene_quest, condition)| {
                byroredux_scripting::scene_phase_awaited_stage(scene_quest, condition)
            }),
    );
    between_scenes.clear();
    between_scenes.extend(
        players
            .iter()
            .filter(|player| player.state == byroredux_scripting::ScenePlaybackState::Finished)
            .filter_map(|player| registry.definition(player.scene_form_id)?.quest_form_id)
            .filter(|quest| !active_quests.contains(quest)),
    );
    // #3937 — the registry guard ends HERE, before anything else is
    // acquired. `docs/engine/ecs.md`'s canonical order is
    // `QuestAdvanceOnActivate -> ScenePlayer -> QuestStageState ->
    // SceneRegistry`, so holding the registry across the
    // `QuestAdvanceOnActivate` / `QuestStageState` / `Transform` /
    // `PhysicsWorld` / `OnTriggerEnterEvent` acquisitions below records
    // edges *out* of the order's sink. `actor_quest_trigger_is_in_sequence`
    // (`crates/scripting/src/trigger.rs`) takes the canonical direction on
    // every BaseForm-gated trigger entry, so a guard that lived to the end
    // of this function closed a two-edge cycle — latent only because both
    // systems are `add_exclusive`, and an abort under
    // `BYRO_LOCK_ORDER_CHECK=1`. #3838 flattened the block that used to end
    // the guard here; this restores that boundary without giving up the
    // scratch reuse. Same pair #3580 fixed on the `trigger.rs` side.
    drop(registry);
    if awaited.is_empty() && between_scenes.is_empty() {
        return;
    }

    // #6008 — the advance conditions are evaluated HERE, before the
    // `QuestStageState` guard below is taken. `evaluate_condition_list`
    // re-acquires that resource for the `GetStage` family, so evaluating
    // inside the filter that holds `stages` was a recursive read (the shape
    // 35f1cf051 removed from the Story Manager, #2386: a second read guard on
    // one thread can deadlock behind a queued writer). Only the BaseForm-gated,
    // conditioned advances of a quest the router is routing for are asked, so
    // the pre-pass costs no more than the filter chain it replaces.
    let passing_triggers: std::collections::HashSet<EntityId> = {
        let conditioned: Vec<_> = world
            .query::<QuestAdvanceOnActivate>()
            .map(|advances| {
                advances
                    .iter()
                    .filter(|(_, advance)| {
                        matches!(advance.activator_gate, ActivatorGate::BaseForm(_))
                            && !advance.conditions.is_empty()
                            && (between_scenes.contains(&advance.owning_quest.0)
                                || awaited.iter().any(|(quest, _)| *quest == advance.owning_quest.0))
                    })
                    .map(|(trigger, advance)| (trigger, advance.conditions.clone()))
                    .collect()
            })
            .unwrap_or_default();
        conditioned
            .into_iter()
            .filter(|(trigger, conditions)| {
                byroredux_scripting::evaluate_condition_list(
                    conditions,
                    world,
                    &byroredux_scripting::ConditionContext::for_subject(*trigger),
                )
            })
            .map(|(trigger, _)| trigger)
            .collect()
    };

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
                            if !matches!(
                                advance.activator_gate,
                                ActivatorGate::BaseForm(candidate) if candidate == base_form_id
                            ) {
                                return false;
                            }
                            if awaited_stage == u16::MAX {
                                // #4333 — between scenes the router asks the
                                // gate's question through the gate's own
                                // predicate. #3954 extracted it for exactly
                                // this, but the router kept an inline copy.
                                byroredux_scripting::base_form_advance_is_eligible(
                                    advance,
                                    byroredux_scripting::QuestFormId(quest_form_id),
                                    current_stage,
                                    &stages,
                                )
                            } else {
                                advance.owning_quest.0 == quest_form_id
                                    && advance.target_stage <= awaited_stage
                                    && !stages
                                        .get_stage_done(advance.owning_quest, advance.target_stage)
                            }
                        })
                        .filter(|(trigger, advance)| {
                            advance.conditions.is_empty() || passing_triggers.contains(trigger)
                        })
                        // #3954 — pick the stage FIRST, exactly as the gate's
                        // `next_ready` does, and only then demand a center.
                        // Folding the center lookup into the `min` dropped a
                        // centerless lowest-stage trigger out of the running
                        // and routed to the next-lowest, which the gate then
                        // refused because its own `min` still returned the
                        // centerless one — the cart stalls with no diagnostic.
                        // A centerless winner now yields no target at all,
                        // which is the honest outcome: there is nowhere to
                        // walk to, and no other stage is allowed.
                        .min_by_key(|(_, advance)| advance.target_stage)
                        .and_then(|(trigger, advance)| {
                            let center = volumes.get(trigger).map(|volume| volume.center).or_else(
                                || {
                                    approaches
                                        .entries()
                                        .iter()
                                        .find(|entry| entry.trigger_entity == trigger)
                                        .map(|entry| entry.center)
                                },
                            )?;
                            Some((base_form_id, advance.target_stage, trigger, center))
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
    // #4546 — tuple order is acquisition order: Transform (canonical
    // early) before ActorCinematicState.
    let driven_vehicles: Vec<_> = match (
        world.query::<Transform>(),
        world.query::<ActorCinematicState>(),
    ) {
        (Some(transforms), Some(states)) => states
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
        // #4546 — tuple order is acquisition order: Transform first.
        match (
            world.query::<Transform>(),
            world.query::<HorseTetherState>(),
        ) {
            (Some(transforms), Some(tethers)) => tethers
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
        // #4546 — Transform (canonical-early) before ActorCinematicState,
        // same as the driven-vehicles half above.
        let Some(transforms) = world.query::<Transform>() else {
            return;
        };
        let Some(states) = world.query::<ActorCinematicState>() else {
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
    use byroredux_core::ecs::components::{Children, MotionType};
    use byroredux_scripting::AnimationTextKeyEvent;

    /// `PickIdle` restarts the idle the actor was spawned with (a creature's own clip, not the humanoid
    /// one — #6231's rule), and leaves an actor with no spawn idle alone. The request drains either way.
    #[test]
    fn pick_idle_restarts_the_actors_own_spawn_idle() {
        let mut world = World::new();
        let (creature, unrecorded) = (world.spawn(), world.spawn());
        for actor in [creature, unrecorded] {
            let mut player = AnimationPlayer::new(3);
            player.local_time = 1.5;
            world.insert(actor, player);
            world.insert(actor, byroredux_scripting::PickIdleRequest);
        }
        world.insert(creature, crate::npc_spawn::SpawnIdle { clip: 7, desync_key: 1 });
        pick_idle_system(&world, 0.0);
        let restarted = world.get::<AnimationPlayer>(creature).unwrap();
        assert_eq!((restarted.clip_handle, restarted.local_time), (7, 0.0));
        drop(restarted);
        assert_eq!(world.get::<AnimationPlayer>(unrecorded).unwrap().clip_handle, 3);
        assert!(world.query::<byroredux_scripting::PickIdleRequest>().unwrap().is_empty());
    }

    /// #6008 — the scene-trigger router holds `QuestStageState` for the whole
    /// `targets` match; `evaluate_condition_list` re-reads it for the `GetStage`
    /// family, a recursive read. The conditions are evaluated in a pre-pass
    /// instead, so the guarded region must not call the evaluator.
    #[test]
    fn scene_trigger_router_evaluates_no_conditions_under_the_stages_guard() {
        let production = byroredux_core::source_scan::production_text(include_str!("cinematic.rs"));
        let start = production
            .find("let targets: Vec<_> = match (")
            .expect("router targets match present");
        let end = start
            + production[start..]
                .find("if targets.is_empty()")
                .expect("router targets match end present");
        let guarded = &production[start..end];
        assert!(guarded.contains("try_resource::<byroredux_scripting::quest_stages::QuestStageState>"));
        assert!(
            !guarded.contains("evaluate_condition_list"),
            "a condition list is evaluated while the router holds QuestStageState"
        );
    }

    #[test]
    fn idle_request_starts_scoped_animation_player_once_per_serial() {
        let mut world = World::new();
        world.register::<ActorCinematicState>();
        world.register::<AnimationTarget>();
        world.register::<AnimationPlayer>();
        world.register::<RootMotionDelta>();
        let mut catalog = IdleClipCatalog::default();
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
            AnimationTarget {
                skeleton_root: skeleton,
                consumed_idle_serial: 0,
            },
        );

        idle_clip_playback_system(&world, 0.0);
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
                .get::<AnimationTarget>(actor)
                .unwrap()
                .consumed_idle_serial,
            1
        );

        world.get_mut::<AnimationPlayer>(actor).unwrap().local_time = 0.25;
        idle_clip_playback_system(&world, 0.0);
        assert_eq!(
            world.get::<AnimationPlayer>(actor).unwrap().local_time,
            0.25,
            "the same request serial must not restart every frame"
        );
    }

    /// ObScript `IsIdlePlaying` (0x1070) reads `PlayingScriptedIdle`: a `PlayIdle` one-shot sets it and
    /// the resume system clears it when the clamp clip has run its duration (#6179).
    #[test]
    fn scripted_one_shot_idle_marks_the_actor_until_it_finishes() {
        use byroredux_core::animation::{AnimationClip, AnimationClipRegistry, CycleType};
        let mut world = World::new();
        world.register::<ActorCinematicState>();
        world.register::<AnimationTarget>();
        world.register::<AnimationPlayer>();
        world.register::<RootMotionDelta>();
        world.register::<crate::components::IdleResume>();
        world.register::<PlayingScriptedIdle>();
        let mut clips = AnimationClipRegistry::new();
        let handle = clips.add(AnimationClip {
            name: "one_shot".into(),
            duration: 1.0,
            cycle_type: CycleType::Clamp,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: Default::default(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
            root_yaw: Vec::new(),
        });
        world.insert_resource(clips);
        let mut catalog = IdleClipCatalog::default();
        catalog.handles.insert(0x0010_6AE3, handle);
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
        world.insert(actor, AnimationTarget { skeleton_root: skeleton, consumed_idle_serial: 0 });
        assert!(!world.has::<PlayingScriptedIdle>(actor));

        idle_clip_playback_system(&world, 0.0);
        assert!(world.has::<PlayingScriptedIdle>(actor), "the one-shot marks the actor");
        assert!(world.has::<crate::components::IdleResume>(actor));

        idle_resume_system(&world, 0.0);
        assert!(world.has::<PlayingScriptedIdle>(actor), "still playing: the clip has not run out");

        world.get_mut::<AnimationPlayer>(actor).unwrap().local_time = 1.0;
        idle_resume_system(&world, 0.0);
        assert!(!world.has::<PlayingScriptedIdle>(actor), "the marker follows the IdleResume off");
        assert!(!world.has::<crate::components::IdleResume>(actor));
    }

    /// `ApplyHavokImpulse(0, 0, -1, 15)` on a Trap-layer body: the impulse is
    /// `15 × fGameplayImpulseScale (150) × fGameplayImpulseMultTrap (0.15)` along
    /// Z-down, which is engine -Y; the body's velocity change is that over its mass.
    #[test]
    fn havok_impulse_request_pushes_the_body_by_the_traps_scaled_magnitude() {
        use byroredux_core::ecs::components::collision::CollisionShape;
        use byroredux_core::ecs::components::GlobalTransform;
        use byroredux_plugin::esm::{reader::GameKind, records::EsmIndex};

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<CollisionShape>();
        world.register::<RigidBodyData>();
        world.register::<RapierHandles>();
        byroredux_scripting::register(&mut world);
        world.insert_resource(PhysicsWorld::new());
        byroredux_scripting::install_havok_impulse_rules(
            &mut world,
            &EsmIndex { game: GameKind::Skyrim, ..Default::default() },
        );
        let trap = world.spawn();
        let origin = byroredux_core::math::Vec3::new(0.0, 500.0, 0.0);
        world.insert(trap, Transform::from_translation(origin));
        world.insert(trap, GlobalTransform::new(origin, byroredux_core::math::Quat::IDENTITY, 1.0));
        world.insert(trap, CollisionShape::Cuboid { half_extents: byroredux_core::math::Vec3::splat(16.0) });
        world.insert(
            trap,
            RigidBodyData { motion_type: MotionType::Dynamic, mass: 10.0, havok_layer: 14, ..Default::default() },
        );
        byroredux_physics::physics_sync_system(&world, 0.0);
        let body = world.get::<RapierHandles>(trap).expect("registered").body;
        let mass = world.resource::<PhysicsWorld>().body_mass(body).expect("mass");

        assert!(byroredux_scripting::request_havok_impulse(&world, trap, 0.0, 0.0, -2.0, 15.0));
        scripted_havok_impulse_system(&world, 0.0);

        let velocity = world.resource::<PhysicsWorld>().bodies[body].linvel().y;
        let expected = -15.0 * 150.0 * 0.15 / mass;
        assert!((velocity - expected).abs() < 0.05 * expected.abs(), "vy {velocity} vs {expected}");
        assert!(world.query::<byroredux_scripting::HavokImpulseRequest>().unwrap().get(trap).is_none(), "drained");
    }

    /// #6009 — a request that finds no `HavokImpulseRules` is drained, not left
    /// to be re-resolved against every rigid body each frame.
    #[test]
    fn havok_impulse_request_is_drained_even_without_rules() {
        let mut world = World::new();
        world.register::<RigidBodyData>();
        byroredux_scripting::register(&mut world);
        let entity = world.spawn();
        assert!(byroredux_scripting::request_havok_impulse(&world, entity, 0.0, 0.0, 1.0, 1.0));
        assert!(world.try_resource::<byroredux_scripting::HavokImpulseRules>().is_none());

        scripted_havok_impulse_system(&world, 0.0);

        assert!(world.query::<byroredux_scripting::HavokImpulseRequest>().unwrap().get(entity).is_none());
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
        world.insert_resource(byroredux_scripting::papyrus_demo::PapyrusPlayerEntity(
            player,
        ));
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
                disable_reference_after_advance: false,
                start_if_stopped: false,
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
                disable_reference_after_advance: false,
                start_if_stopped: false,
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
                disable_reference_after_advance: false,
                start_if_stopped: false,
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
                disable_reference_after_advance: false,
                start_if_stopped: false,
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
                disable_reference_after_advance: false,
                start_if_stopped: false,
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

        let mut system = make_scene_trigger_actor_approach_system();
        system(&world, 0.1);

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

        let mut system = make_scene_trigger_actor_approach_system();
        system(&world, 0.1);

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

        // #3838 — the players Vec and the two HashSets are now persistent
        // scratch reused across frames instead of freshly allocated each tick.
        // The failure mode of that refactor is a missing `clear()`: last
        // frame's `awaited` / `between_scenes` surviving into a frame that
        // should see none. Drive the SAME closure again with every scene
        // player despawned — the sets must come up empty and nothing may move.
        let players: Vec<_> = {
            let q = world
                .query::<byroredux_scripting::ScenePlayer>()
                .expect("scene players were installed above");
            q.iter().map(|(entity, _)| entity).collect()
        };
        for entity in players {
            world.remove::<byroredux_scripting::ScenePlayer>(entity);
        }
        let before_loaded = world.get::<Transform>(loaded).unwrap().translation;
        let before_next = world.get::<Transform>(next_actor).unwrap().translation;

        system(&world, 0.1);

        assert_eq!(
            world.get::<Transform>(loaded).unwrap().translation,
            before_loaded,
            "no ScenePlayer remains, so a reused scratch must not replay last \
             frame's awaited set (#3838)"
        );
        assert_eq!(
            world.get::<Transform>(next_actor).unwrap().translation,
            before_next,
            "no ScenePlayer remains, so a reused scratch must not replay last \
             frame's between_scenes set (#3838)"
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

    /// #3937 — `SceneRegistry` is the sink of `docs/engine/ecs.md`'s
    /// canonical acquisition order, and this system reads it only to
    /// resolve scene definitions into the scratch buffers. #3838's scratch
    /// rework replaced the block that scoped the guard with straight-line
    /// code, so the guard lived to the end of the function and recorded
    /// edges out of the sink into `QuestAdvanceOnActivate`,
    /// `QuestStageState`, `Transform` and `PhysicsWorld` — the reverse of
    /// the order `actor_quest_trigger_is_in_sequence` takes, i.e. a cycle.
    /// Only visible in source (both sides are reads until a `Transform`
    /// write appears beneath), so pin it here.
    #[test]
    fn the_scene_registry_guard_is_released_before_the_quest_acquisitions() {
        const CINEMATIC_RS: &str = include_str!("cinematic.rs");

        let body = CINEMATIC_RS
            .split_once("fn scene_trigger_actor_approach_system_inner")
            .expect("scene_trigger_actor_approach_system_inner definition")
            .1;
        let acquire = body
            .find("world.try_resource::<byroredux_scripting::SceneRegistry>()")
            .expect("the SceneRegistry acquisition");
        let release = body
            .find("drop(registry);")
            .expect("the SceneRegistry guard must be released explicitly");
        let advances = body
            .find("world.query::<QuestAdvanceOnActivate>()")
            .expect("the QuestAdvanceOnActivate query acquisition");
        assert!(
            acquire < release && release < advances,
            "scene_trigger_actor_approach_system_inner must release its \
             SceneRegistry guard before acquiring QuestAdvanceOnActivate — \
             holding it across the quest/transform acquisitions records an \
             edge out of the canonical order's sink and cycles against \
             actor_quest_trigger_is_in_sequence (#3937); see docs/engine/ecs.md",
        );
    }

    /// #4333 — #3954 extracted `base_form_advance_is_eligible` so this router
    /// and the trigger gate answer "which stage is next" identically, but the
    /// router kept an inline copy of the conjunction, so a later change to
    /// the gate's rule would not have reached it. Pin the call, not the copy.
    #[test]
    fn the_router_calls_the_gates_shared_eligibility_predicate() {
        const CINEMATIC_RS: &str = include_str!("cinematic.rs");

        let body = CINEMATIC_RS
            .split_once("fn scene_trigger_actor_approach_system_inner")
            .expect("scene_trigger_actor_approach_system_inner definition")
            .1;
        let body = byroredux_core::source_scan::production_text(body);
        assert!(
            body.contains("byroredux_scripting::base_form_advance_is_eligible("),
            "the between-scenes router must call the gate's shared predicate (#4333)"
        );
    }
    /// #3817 — the full retention lifecycle: a tethered convoy drives its
    /// authored XLKR route, the terminal marker is reached (vanilla's own
    /// convoy stops there), and the release must (a) remove the
    /// `HorseTetherState`, (b) detach riders' vehicle attachment while
    /// keeping their seat bookkeeping, and (c) queue the entities that
    /// lost their `CellRoot` (the #3254 mid-tether strip) for
    /// re-adoption — never leave them as permanent zombies.
    #[test]
    fn tether_releases_at_the_authored_route_terminal_and_detaches_riders() {
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<HorseTetherState>();
        world.register::<ActorCinematicState>();
        world.register::<CellRoot>();
        world.register::<Children>();
        world.insert_resource(CinematicReAdoption::default());
        world.insert_resource(CellRootIndex::new());
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
        let rider = world.spawn();
        // World positions inside exterior grid (0, 0) so the re-adoption
        // retry resolves them to the cell root the test registers below.
        world.insert(horse, Transform::IDENTITY);
        world.insert(cart, Transform::new(Vec3::new(100.0, 0.0, -100.0), Quat::IDENTITY, 1.0));
        world.insert(rider, Transform::new(Vec3::new(100.0, 2.0, -100.0), Quat::IDENTITY, 1.0));
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
        world.insert(
            rider,
            ActorCinematicState {
                vehicle: Some(cart),
                vehicle_local_translation: Some(Vec3::new(0.0, 2.0, 0.0)),
                vehicle_local_rotation: Some(Quat::IDENTITY),
                cart_seat: Some(2),
                ..Default::default()
            },
        );

        // Drive until the horse has arrived at the terminal marker's
        // heading extension (~x = 4096 + 100 at walk speed 100 u/s) and
        // the release fires. The loop must not depend on exact tick
        // arithmetic — it terminates on the observable.
        let mut released = false;
        for _ in 0..2_000 {
            cinematic_horse_route_system(&world, 0.1);
            if world.get::<HorseTetherState>(cart).is_none() {
                released = true;
                break;
            }
        }
        assert!(
            released,
            "the tether must release once the authored route terminal is reached — retention would otherwise be permanent (#3817)"
        );

        // (b) rider detachment: attachment gone, seat bookkeeping kept so
        // a later scripted exit animation still resolves.
        let (vehicle_after, seat, local) = {
            let rider_state = world.get::<ActorCinematicState>(rider).unwrap();
            (rider_state.vehicle, rider_state.cart_seat, rider_state.vehicle_local_translation)
        };
        assert_eq!(vehicle_after, None, "the rider must detach");
        assert_eq!(seat, Some(2));
        assert_eq!(local, Some(Vec3::new(0.0, 2.0, 0.0)));

        // (c) neither cart nor rider carried a `CellRoot` (the #3254 strip
        // fired when the home cell unloaded mid-tether) — both must now be
        // queued for re-adoption, not silently un-owned forever.
        let queued = world
            .try_resource::<CinematicReAdoption>()
            .map(|p| p.pending.clone())
            .expect("re-adoption resource");
        assert!(
            queued.contains(&cart) && queued.contains(&rider),
            "un-rooted released entities must be queued for re-adoption, got {queued:?}"
        );

        // The streaming step's retry adopts them onto the loaded cell at
        // their position. World-space (100, ·, -100) is grid (0, 0).
        let mut loaded = std::collections::HashMap::new();
        let cell_root = world.spawn();
        loaded.insert((0, 0), crate::streaming::LoadedCell { cell_root });
        let adopted = crate::systems::retry_cinematic_readoption(&mut world, &loaded);
        assert_eq!(adopted, 2, "both un-rooted entities must be adopted");
        assert_eq!(
            world.get::<CellRoot>(cart).map(|root| root.0),
            Some(cell_root)
        );
        assert_eq!(
            world.get::<CellRoot>(rider).map(|root| root.0),
            Some(cell_root)
        );
        // The horse drove past the cell boundary during the route (its
        // terminal extension is ~4096 units out) — it is still un-owned in
        // an unloaded grid cell, so it stays pending until a cell loads
        // beneath it. That is the surfaced-bounded-population contract,
        // not a leak.
        assert_eq!(
            world.try_resource::<CinematicReAdoption>().map(|p| p.pending.clone()),
            Some(vec![horse]),
            "only the entity outside every loaded cell may stay pending"
        );
        assert!(
            world
                .try_resource::<CellRootIndex>()
                .map(|idx| idx
                    .map
                    .get(&cell_root)
                    .is_some_and(|owned| owned.contains(&cart) && owned.contains(&rider)))
                .unwrap_or(false),
            "the re-adoption must be registered in the unload index, or the next cell unload cannot find them"
        );
    }

    /// #5461 — `Effect::ExitCart` derives the exit root-motion heading from
    /// `vehicle.rotation * vehicle_local_rotation` while the rider is
    /// attached, and from the rider's own `Transform.rotation` once release
    /// has cleared `vehicle`. Those are the same heading only if the
    /// attachment system left the rider's rotation composed that way on the
    /// last tethered tick; nothing pinned it, and the release doc claimed the
    /// opposite of what the code does. Uses a non-trivial cart heading and
    /// seat rotation so an identity-everywhere coincidence cannot pass.
    #[test]
    fn released_rider_exit_heading_matches_the_attached_heading() {
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<HorseTetherState>();
        world.register::<ActorCinematicState>();
        world.register::<CellRoot>();
        world.register::<Children>();
        world.insert_resource(CinematicReAdoption::default());
        world.insert_resource(CellRootIndex::new());

        let horse_rotation = Quat::from_rotation_y(0.7);
        let cart_local_rotation = Quat::from_rotation_y(0.3);
        let seat_rotation = Quat::from_rotation_y(-0.4) * Quat::from_rotation_x(0.2);
        let horse = world.spawn();
        let cart = world.spawn();
        let rider = world.spawn();
        world.insert(horse, Transform::new(Vec3::new(10.0, 0.0, -10.0), horse_rotation, 1.0));
        world.insert(cart, Transform::IDENTITY);
        world.insert(rider, Transform::IDENTITY);
        world.insert(
            cart,
            HorseTetherState {
                horse,
                horse_local_translation: Vec3::ZERO,
                horse_local_rotation: cart_local_rotation,
                route_target_form_id: None,
            },
        );
        world.insert(
            rider,
            ActorCinematicState {
                vehicle: Some(cart),
                vehicle_local_translation: Some(Vec3::new(0.0, 2.0, 0.0)),
                vehicle_local_rotation: Some(seat_rotation),
                cart_seat: Some(2),
                ..Default::default()
            },
        );

        // The last tethered tick: the cart follows the horse, the rider
        // follows the cart.
        vehicle_attachment_system(&world, 0.0);

        // The heading `ExitCart` derives while the rider is still attached.
        // #5535 — each read copies its field out and drops its guard before
        // the next: holding the `ActorCinematicState` guard across the
        // `Transform` read recorded `ActorCinematicState` → `Transform`, the
        // reverse of the #4546 production order (`Transform` first), and
        // closed a lock-order cycle under `BYRO_LOCK_ORDER_CHECK=1`.
        let attached = {
            let cart_rotation = world.get::<Transform>(cart).unwrap().rotation;
            let seat_rotation = world
                .get::<ActorCinematicState>(rider)
                .unwrap()
                .vehicle_local_rotation
                .expect("attached rider");
            cart_rotation * seat_rotation
        };
        assert!(
            attached.angle_between(Quat::IDENTITY) > 0.3,
            "fixture: a non-trivial heading, so an identity coincidence cannot pass"
        );

        release_finished_tethers(&world, &[(cart, horse)]);
        assert_eq!(
            world.get::<ActorCinematicState>(rider).unwrap().vehicle,
            None,
            "fixture: release clears the rider's vehicle, which is what sends \
             ExitCart down its fallback branch"
        );

        // The heading `ExitCart` derives after release: the rider's own rotation.
        let released = world.get::<Transform>(rider).unwrap().rotation;
        assert!(
            attached.angle_between(released) < 1.0e-4,
            "release must leave the rider's rotation at vehicle.rotation * \
             vehicle_local_rotation, or ExitCart's fallback changes the exit heading"
        );
    }

    /// #5379 — the process-lifetime player rides the convoy (Skyrim
    /// MQ101's opening cart ride) but must never be queued for cell
    /// re-adoption: a `CellRoot` stamp on the player makes the next
    /// streaming unload or save-load teardown despawn it, leaving the
    /// session with no body. Covers both guards: the release walk
    /// excludes the player's subtree, and the retry declines a player
    /// entry even when one is handed to it directly.
    #[test]
    fn player_rider_is_never_queued_or_cell_adopted() {
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<HorseTetherState>();
        world.register::<ActorCinematicState>();
        world.register::<CellRoot>();
        world.register::<Children>();
        world.insert_resource(CinematicReAdoption::default());
        world.insert_resource(CellRootIndex::new());

        let horse = world.spawn();
        let cart = world.spawn();
        let player = world.spawn();
        let body = world.spawn();
        // Everything inside exterior grid (0, 0), so the retry below CAN
        // resolve a loaded cell at each position — the test's point is
        // that the player must not resolve to one.
        world.insert(horse, Transform::new(Vec3::new(100.0, 0.0, -100.0), Quat::IDENTITY, 1.0));
        world.insert(cart, Transform::new(Vec3::new(100.0, 0.0, -100.0), Quat::IDENTITY, 1.0));
        world.insert(player, Transform::new(Vec3::new(100.0, 2.0, -100.0), Quat::IDENTITY, 1.0));
        world.insert(body, Transform::new(Vec3::new(100.0, 2.0, -99.0), Quat::IDENTITY, 1.0));
        world.insert(player, Children(vec![body]));
        world.insert(
            cart,
            HorseTetherState {
                horse,
                horse_local_translation: Vec3::ZERO,
                horse_local_rotation: Quat::IDENTITY,
                route_target_form_id: None,
            },
        );
        world.insert(
            player,
            ActorCinematicState {
                vehicle: Some(cart),
                ..Default::default()
            },
        );
        world.insert_resource(crate::systems::character::PlayerEntity(Some(player)));

        release_finished_tethers(&world, &[(cart, horse)]);

        let queued = world
            .try_resource::<CinematicReAdoption>()
            .map(|p| p.pending.clone())
            .expect("re-adoption resource");
        assert!(
            !queued.contains(&player) && !queued.contains(&body),
            "the player and its body subtree must never queue for cell \
             adoption, got {queued:?}"
        );
        assert!(
            queued.contains(&cart) && queued.contains(&horse),
            "the convoy itself still queues"
        );

        // Defense in depth: a player entry that reaches the list anyway
        // (queued before this fix, or by a future installer) is consumed
        // by the retry, never adopted.
        if let Some(mut pending) = world.try_resource_mut::<CinematicReAdoption>() {
            pending.pending.push(player);
        }
        let mut loaded = std::collections::HashMap::new();
        let cell_root = world.spawn();
        loaded.insert((0, 0), crate::streaming::LoadedCell { cell_root });
        crate::systems::retry_cinematic_readoption(&mut world, &loaded);
        assert!(
            world.get::<CellRoot>(player).is_none(),
            "the retry must never stamp a CellRoot on the process-lifetime player"
        );
        let after = world
            .try_resource::<CinematicReAdoption>()
            .map(|p| p.pending.clone())
            .expect("re-adoption resource");
        assert!(
            !after.contains(&player),
            "the doctored player entry is consumed, not kept pending"
        );
    }

    /// #5384 — a released convoy's PARENTED render subtree must ride its
    /// root's adoption: pre-fix the retry read each member's LOCAL
    /// `Transform` (a few units from its parent), mapped it to an origin
    /// grid cell, and either split the hierarchy across two cells or left
    /// the node pending forever — orphaned meshes that outlive their root
    /// and keep drawing where they last stood. Only the parentless root
    /// queues, and its adoption stamps the whole subtree.
    #[test]
    fn parented_subtree_rides_the_roots_adoption() {
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<HorseTetherState>();
        world.register::<ActorCinematicState>();
        world.register::<CellRoot>();
        world.register::<Children>();
        world.register::<Parent>();
        world.insert_resource(CinematicReAdoption::default());
        world.insert_resource(CellRootIndex::new());

        let horse = world.spawn();
        let cart = world.spawn();
        let rider = world.spawn();
        let bone = world.spawn();
        // Cart, rider and a render-subtree bone (parented to the cart).
        world.insert(horse, Transform::new(Vec3::new(100.0, 0.0, -100.0), Quat::IDENTITY, 1.0));
        world.insert(cart, Transform::new(Vec3::new(100.0, 0.0, -100.0), Quat::IDENTITY, 1.0));
        world.insert(rider, Transform::new(Vec3::new(100.0, 2.0, -100.0), Quat::IDENTITY, 1.0));
        // The bone's LOCAL offset — a few units from the cart, which
        // pre-fix mapped to a wrong origin grid cell.
        world.insert(bone, Transform::new(Vec3::new(2.0, 0.0, 1.0), Quat::IDENTITY, 1.0));
        world.insert(cart, Children(vec![bone]));
        world.insert(bone, Parent(cart));
        world.insert(
            cart,
            HorseTetherState {
                horse,
                horse_local_translation: Vec3::ZERO,
                horse_local_rotation: Quat::IDENTITY,
                route_target_form_id: None,
            },
        );
        world.insert(
            rider,
            ActorCinematicState {
                vehicle: Some(cart),
                ..Default::default()
            },
        );

        release_finished_tethers(&world, &[(cart, horse)]);

        // The parented bone never queues — only the parentless roots do.
        let queued = world
            .try_resource::<CinematicReAdoption>()
            .map(|p| p.pending.clone())
            .expect("re-adoption resource");
        assert!(
            !queued.contains(&bone),
            "a parented subtree node must never queue for cell adoption, got {queued:?}"
        );
        assert!(queued.contains(&cart) && queued.contains(&rider));

        // The retry adopts the roots at their WORLD positions and stamps
        // the subtree with the same CellRoot — the hierarchy stays whole.
        let mut loaded = std::collections::HashMap::new();
        let cell_root = world.spawn();
        loaded.insert((0, 0), crate::streaming::LoadedCell { cell_root });
        crate::systems::retry_cinematic_readoption(&mut world, &loaded);
        assert_eq!(
            world.get::<CellRoot>(cart).map(|root| root.0),
            Some(cell_root)
        );
        assert_eq!(
            world.get::<CellRoot>(rider).map(|root| root.0),
            Some(cell_root)
        );
        assert_eq!(
            world.get::<CellRoot>(bone).map(|root| root.0),
            Some(cell_root),
            "the parented bone rides the cart root's adoption — same cell, \
             hierarchy whole, one unload despawns all of it (#5384)"
        );
        assert!(world
            .try_resource::<CinematicReAdoption>()
            .is_some_and(|p| p.pending.is_empty()));
    }

    /// #3817 companion pin — a convoy that kept its `CellRoot` (the common
    /// case: the home cell is still loaded when the route ends) must NOT
    /// be queued for re-adoption; it simply stops being retained.
    #[test]
    fn tether_release_touches_nothing_when_the_home_cell_still_owns_the_convoy() {
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<HorseTetherState>();
        world.register::<CellRoot>();
        world.insert_resource(CinematicReAdoption::default());
        byroredux_scripting::install_package_target_positions(
            &mut world,
            [(0x100, Vec3::new(5.0, 20.0, 0.0)), (0x101, Vec3::new(100.0, 0.0, 0.0))],
        );
        byroredux_scripting::install_package_linked_references(
            &mut world,
            [(0x100, vec![(0, 0x101)])],
        );

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
        // Still owned by its loaded home cell.
        let home = world.spawn();
        world.insert(cart, CellRoot(home));
        world.insert(horse, CellRoot(home));

        let mut released = false;
        for _ in 0..2_000 {
            cinematic_horse_route_system(&world, 0.1);
            if world.get::<HorseTetherState>(cart).is_none() {
                released = true;
                break;
            }
        }
        assert!(released, "the tether must still release");
        assert_eq!(
            world.get::<CellRoot>(cart).map(|root| root.0),
            Some(home),
            "a still-owned entity must keep its root untouched"
        );
        assert!(
            world
                .try_resource::<CinematicReAdoption>()
                .is_some_and(|p| p.pending.is_empty()),
            "nothing may be queued when every released entity is still owned"
        );
    }


    /// A placed reference carries no body itself: its bodies are the entities beneath it
    /// and the bare collision entities whose `PhysicsSourceForm` names its REFR (found
    /// live: a trap's `SetMotionType` and `ApplyHavokImpulse` reached no body).
    #[test]
    fn a_references_bodies_are_its_own_its_children_and_the_collision_entities_it_owns() {
        use byroredux_core::ecs::components::{FormIdComponent, PhysicsSourceForm};
        use byroredux_core::ecs::Parent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
        let mut world = World::new();
        world.register::<RigidBodyData>();
        world.register::<Parent>();
        world.register::<FormIdComponent>();
        world.register::<PhysicsSourceForm>();
        let mut pool = FormIdPool::new();
        let mut form = |local| pool.intern(FormIdPair { plugin: PluginId::from_filename("Skyrim.esm"), local: LocalFormId(local) });
        let (rock, other) = (form(0xC27C8), form(0xC27C7));
        world.insert_resource(pool);
        let root = world.spawn();
        let child = world.spawn();
        let bare = world.spawn();
        let stranger = world.spawn();
        let foreign = world.spawn();
        world.insert(root, FormIdComponent(rock));
        world.insert(child, Parent(root));
        world.insert(bare, PhysicsSourceForm(rock));
        world.insert(foreign, PhysicsSourceForm(other));
        for entity in [child, bare, stranger, foreign] {
            world.insert(entity, RigidBodyData::static_body());
        }
        let mut bodies = reference_body_entities(&world, root);
        bodies.sort_unstable();
        assert_eq!(bodies, vec![child, bare], "not the stranger, not another REFR's body");
    }
}
