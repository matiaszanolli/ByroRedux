//! Trigger-volume detection — the engine emit site for
//! [`OnTriggerEnterEvent`](crate::events::OnTriggerEnterEvent).
//!
//! A trigger volume is an invisible REFR (no MODL) whose geometry is an
//! `XPRM` primitive — a box or sphere placed in the world. Bethesda's
//! `default*Trigger` script family runs `OnTriggerEnter(akActionRef)` when
//! an actor crosses *into* that volume. This module is the runtime half
//! the [`quest_advance`](crate::papyrus_demo::quest_advance) recognizer
//! needs: the cell loader attaches a [`TriggerVolume`] to each trigger
//! REFR, and [`trigger_detection_system`] emits `OnTriggerEnterEvent` on
//! the volume entity the frame the player enters it.
//!
//! **Edge-triggered.** Papyrus `OnTriggerEnter` fires once per crossing,
//! not every frame the actor is inside; [`TriggerVolume::occupant_inside`]
//! carries the previous-frame state so only an outside→inside transition
//! emits. Leaving and re-entering fires again — matching the engine.
//!
//! The volume is stored in **world space** (computed once by the cell
//! loader from the REFR placement + primitive), so detection needs only
//! the player's world position — no per-frame transform composition, and
//! correct for the static volumes that triggers always are.

use crate::events::OnTriggerEnterEvent;
use crate::papyrus_demo::PlayerEntity;
use byroredux_core::ecs::components::GlobalTransform;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_core::math::{Quat, Vec3};
use std::collections::{HashMap, HashSet};

/// The primitive shape of a trigger volume (`XPRM` shape-type 1 / 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerShape {
    /// Oriented box — `half_extents` are per-axis, `rotation` orients it.
    Box,
    /// Sphere — `half_extents.x` is the radius; `rotation` is unused.
    Sphere,
}

/// World-space trigger volume attached to an invisible trigger REFR.
/// Built by the cell loader from the REFR's placement + `XPRM` primitive.
#[derive(Debug, Clone, Copy)]
pub struct TriggerVolume {
    /// World-space center (the REFR's placed position).
    pub center: Vec3,
    /// Box: per-axis half-extents. Sphere: `x` holds the radius.
    pub half_extents: Vec3,
    /// Box orientation (the REFR's placed rotation). Identity for spheres.
    pub rotation: Quat,
    pub shape: TriggerShape,
    /// Edge-trigger state: was the player inside on the previous tick this
    /// volume was checked? `None` until [`trigger_detection_system`] has
    /// evaluated it at least once — every volume is spawned with `None`
    /// (see the cell loader's `trigger_volume_from_primitive`), so the
    /// very first tick a volume exists for silently seeds this from
    /// whatever the player's actual position is that frame, WITHOUT
    /// emitting an enter event. This is what makes the seed correct
    /// whether the volume was just loaded on initial cell entry, a
    /// door-walk transition, or exterior streaming step-in: none of
    /// those call sites need to know the player's position, because
    /// "no prior tick to compare against" and "player loaded already
    /// standing inside" collapse to the same case — SCR-D6-NEW-02 /
    /// #1817. Fixes #1742-adjacent load-time false positives without
    /// threading player position through every `TriggerVolume` spawn site.
    pub occupant_inside: Option<bool>,
}

impl TriggerVolume {
    /// Whether world-space point `p` lies inside the volume.
    pub fn contains(&self, p: Vec3) -> bool {
        match self.shape {
            TriggerShape::Sphere => {
                let r = self.half_extents.x;
                (p - self.center).length_squared() <= r * r
            }
            TriggerShape::Box => {
                // Map the point into the box's local frame, then test
                // against the half-extents on each axis (OBB containment).
                let local = self.rotation.inverse() * (p - self.center);
                local.x.abs() <= self.half_extents.x
                    && local.y.abs() <= self.half_extents.y
                    && local.z.abs() <= self.half_extents.z
            }
        }
    }

    /// Whether a world-space sphere overlaps this volume. Havok trigger
    /// callbacks are shape contacts, so actor-driven triggers must account
    /// for body extent rather than testing only the actor root point.
    pub fn intersects_sphere(&self, center: Vec3, radius: f32) -> bool {
        let radius = radius.max(0.0);
        match self.shape {
            TriggerShape::Sphere => {
                let combined = self.half_extents.x + radius;
                (center - self.center).length_squared() <= combined * combined
            }
            TriggerShape::Box => {
                let local = self.rotation.inverse() * (center - self.center);
                let closest = local.clamp(-self.half_extents, self.half_extents);
                (local - closest).length_squared() <= radius * radius
            }
        }
    }
}

/// Conservative horizontal/body radius for a native tethered horse. The
/// actor parser does not yet expose NPC/CREA collision bounds to scripting;
/// this matches the broad Havok capsule needed by cart-route trigger gates.
const TETHERED_HORSE_TRIGGER_RADIUS: f32 = 96.0;

impl Component for TriggerVolume {
    type Storage = SparseSetStorage<Self>;
}

/// Register the [`TriggerVolume`] storage. Called from [`crate::register`].
pub fn register(world: &mut World) {
    world.register::<TriggerVolume>();
    if world.try_resource::<TriggerOccupancyState>().is_none() {
        world.insert_resource(TriggerOccupancyState::default());
    }
}

/// Previous-frame occupancy for non-player quest/scene references. Player
/// occupancy remains embedded in [`TriggerVolume`] for save compatibility;
/// this sparse side table covers NPC-driven patrol/package crossings.
#[derive(Debug, Default)]
struct TriggerOccupancyState {
    inside: HashMap<(EntityId, EntityId), bool>,
}

impl byroredux_core::ecs::resource::Resource for TriggerOccupancyState {}

/// Per-frame detection: for every [`TriggerVolume`], test the player's
/// world position and emit an [`OnTriggerEnterEvent`] on the volume
/// entity the frame the player transitions from outside to inside. The
/// `OnTriggerEnterEvent` marker is drained by `event_cleanup_system` at
/// end-of-frame, so the quest-advance dispatch sees exactly one enter per
/// crossing.
///
/// No-ops gracefully when there's no player, no player transform, or no
/// trigger volumes in the world.
pub fn trigger_detection_system(world: &World) {
    let Some(player) = world.try_resource::<PlayerEntity>().map(|p| p.0) else {
        return;
    };
    // The player's world position — the only spatial input detection needs.
    let Some(player_pos) = player_world_position(world, player) else {
        return;
    };

    // Phase 1 (read+update): flip each volume's occupancy and record the
    // entities that just entered. Mutating `occupant_inside` here keeps
    // the edge-trigger state on the component itself.
    let mut entered: Vec<(EntityId, EntityId)> = Vec::new();
    {
        let Some(mut vols) = world.query_mut::<TriggerVolume>() else {
            return;
        };
        for (entity, vol) in vols.iter_mut() {
            let inside = vol.contains(player_pos);
            // SCR-D6-NEW-02 / #1817 — `None` means this volume has never
            // been checked before (just spawned, this tick). Seed the
            // state silently instead of comparing against a synthetic
            // "was outside" default — a player who loads already standing
            // inside the volume must not see a spurious enter on its
            // first tick.
            if let Some(was_inside) = vol.occupant_inside {
                if inside && !was_inside {
                    entered.push((entity, player));
                }
            }
            vol.occupant_inside = Some(inside);
        }
    }

    // Quest/scene references can drive triggers too (vanilla MQ101 uses a
    // specific cart horse to set stage 22). Candidate identity keeps this
    // scan on logical placed references instead of every render child with a
    // GlobalTransform. Tethered horses are active native movers: if exterior
    // streaming first materializes a trigger around one already inside, that
    // first observation is a real entry rather than the player's load-time
    // cold-start case and must be delivered.
    let tethered_horses: HashSet<EntityId> = world
        .query::<crate::HorseTetherState>()
        .map(|query| query.iter().map(|(_, tether)| tether.horse).collect())
        .unwrap_or_default();
    let actors: Vec<(EntityId, Vec3, bool, u32)> = world
        .query::<crate::scene::SceneAliasCandidate>()
        .map(|query| {
            query
                .iter()
                .filter(|(entity, _)| *entity != player)
                .filter_map(|(entity, identity)| {
                    world.get::<GlobalTransform>(entity).map(|transform| {
                        (
                            entity,
                            transform.translation,
                            tethered_horses.contains(&entity),
                            identity.base_form_id,
                        )
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if !actors.is_empty() {
        let advances = world.query::<crate::papyrus_demo::quest_advance::QuestAdvanceOnActivate>();
        let volumes: Vec<(EntityId, TriggerVolume)> = world
            .query::<TriggerVolume>()
            .map(|query| {
                query
                    .iter()
                    .map(|(entity, volume)| (entity, *volume))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(mut occupancy) = world.try_resource_mut::<TriggerOccupancyState>() {
            let mut observed = HashSet::new();
            for (trigger, volume) in volumes {
                for (actor, position, active_mover, base_form_id) in &actors {
                    let inside = if *active_mover {
                        volume.intersects_sphere(*position, TETHERED_HORSE_TRIGGER_RADIUS)
                    } else {
                        volume.contains(*position)
                    };
                    if *active_mover
                        && (*position - volume.center).length_squared() <= 2048.0 * 2048.0
                        && advances.as_ref().is_some_and(|advances| {
                            advances.get(trigger).is_some_and(|advance| {
                                matches!(
                                    advance.activator_gate,
                                    crate::papyrus_demo::quest_advance::ActivatorGate::BaseForm(
                                        expected
                                    ) if expected == *base_form_id
                                )
                            })
                        })
                    {
                        log::debug!(
                            "base-gated trigger {trigger} actor {actor} base=0x{base_form_id:08X} position={position:?} center={:?} inside={inside}",
                            volume.center
                        );
                    }
                    let key = (trigger, *actor);
                    observed.insert(key);
                    let was_inside = occupancy.inside.insert(key, inside);
                    // If a native mover entered before this trigger's quest
                    // prerequisites became true, Papyrus declined the first
                    // event while the actor remained inside. Re-deliver once
                    // the authored condition becomes ready; the target-stage
                    // done check makes this an edge, not a per-frame pulse.
                    let became_ready_inside = inside
                        && *active_mover
                        && was_inside == Some(true)
                        && advances.as_ref().is_some_and(|advances| {
                            advances.get(trigger).is_some_and(|advance| {
                                matches!(
                                    advance.activator_gate,
                                    crate::papyrus_demo::quest_advance::ActivatorGate::BaseForm(
                                        expected
                                    ) if expected == *base_form_id
                                ) && {
                                    // #3580 — bind and DROP the
                                    // `QuestStageState` guard before
                                    // `condition::evaluate` runs. A
                                    // `try_resource(...).is_some_and(..) &&
                                    // evaluate(..)` chain keeps the guard's
                                    // temporary alive to the end of the whole
                                    // expression, so `evaluate` ran underneath
                                    // it — and its `IsSceneActionComplete` arm
                                    // takes `SceneRegistry`. That recorded
                                    // `QuestStageState -> SceneRegistry`,
                                    // closing a cycle against
                                    // `actor_quest_trigger_is_in_sequence`
                                    // below, which holds `SceneRegistry`
                                    // across its own `QuestStageState`
                                    // acquisition.
                                    let already_done = world
                                        .try_resource::<crate::quest_stages::QuestStageState>()
                                        .is_some_and(|stages| {
                                            stages.get_stage_done(
                                                advance.owning_quest,
                                                advance.target_stage,
                                            )
                                        });
                                    !already_done
                                } && crate::condition::evaluate(
                                    &advance.conditions,
                                    world,
                                    &crate::condition::ConditionContext::for_subject(trigger),
                                )
                            })
                        });
                    if inside
                        && (was_inside == Some(false)
                            || (was_inside.is_none() && *active_mover)
                            || became_ready_inside)
                    {
                        entered.push((trigger, *actor));
                    }
                }
            }
            occupancy.inside.retain(|key, _| observed.contains(key));
        }
    }

    // Phase 2 (write): emit the enter markers. Separate borrow so the
    // TriggerVolume write lock is released first.
    if entered.is_empty() {
        return;
    }
    entered.retain(|(trigger, _)| actor_quest_trigger_is_in_sequence(world, *trigger));
    if entered.is_empty() {
        return;
    }
    let Some(mut events) = world.query_mut::<OnTriggerEnterEvent>() else {
        return;
    };
    for (entity, triggerer) in entered {
        log::debug!("trigger {entity} emitted OnTriggerEnter for actor {triggerer}");
        if let Some(event) = events.get_mut(entity) {
            if !event.triggerers.contains(&triggerer) {
                event.triggerers.push(triggerer);
            }
        } else {
            events.insert(
                entity,
                OnTriggerEnterEvent {
                    triggerers: vec![triggerer],
                },
            );
        }
    }
}

/// Prevent already-occupied actor triggers from skipping ahead of the scene
/// that authors their ordering. During a scene, only stages up to the current
/// phase's explicit `GetStageDone` wait may fire. Between scenes, only the
/// lowest ready stage at or above the quest's current stage may fire.
fn actor_quest_trigger_is_in_sequence(world: &World, trigger: EntityId) -> bool {
    use byroredux_plugin::esm::records::condition::{ComparisonOp, ConditionValue};

    let Some(advances) =
        world.query::<crate::papyrus_demo::quest_advance::QuestAdvanceOnActivate>()
    else {
        return true;
    };
    let Some(advance) = advances.get(trigger) else {
        return true;
    };
    if !matches!(
        advance.activator_gate,
        crate::papyrus_demo::quest_advance::ActivatorGate::BaseForm(_)
    ) {
        return true;
    }
    let Some(players) = world.query::<crate::ScenePlayer>() else {
        return true;
    };
    // #3580 — this scope exists so the `SceneRegistry` guard is DROPPED
    // before the `QuestStageState` acquisition below. Holding it across that
    // read recorded `SceneRegistry -> QuestStageState`, which closed a cycle
    // against the opposite edge the trigger scan records
    // (`QuestStageState -> SceneRegistry`, via `condition::evaluate`'s
    // `IsSceneActionComplete` arm). Nothing here needs the registry after the
    // loop — only the three summary values it produces.
    let (has_running_scene, has_finished_scene, awaited_stages) = {
        let Some(registry) = world.try_resource::<crate::SceneRegistry>() else {
            return true;
        };

        let mut has_running_scene = false;
        let mut has_finished_scene = false;
        let mut awaited_stages = Vec::new();
        for (_, player) in players.iter() {
            let Some(scene) = registry.definition(player.scene_form_id) else {
                continue;
            };
            if scene.quest_form_id != Some(advance.owning_quest.0) {
                continue;
            }
            if player.is_running() {
                has_running_scene = true;
                if let Some(phase) = scene.phases.get(player.current_phase as usize) {
                    awaited_stages.extend(phase.completion_conditions.iter().filter_map(|condition| {
                    (condition.function_index == 59
                        && condition.comparator == ComparisonOp::Eq
                        && matches!(condition.comparand, ConditionValue::Literal(value) if (value - 1.0).abs() <= f32::EPSILON)
                        && condition.param_1 == advance.owning_quest.0)
                    .then_some(condition.param_2 as u16)
                }));
                }
            } else if player.state == crate::ScenePlaybackState::Finished {
                has_finished_scene = true;
            }
        }
        (has_running_scene, has_finished_scene, awaited_stages)
    };
    if has_running_scene {
        let allowed = awaited_stages
            .into_iter()
            .any(|stage| advance.target_stage <= stage);
        if !allowed {
            log::debug!(
                "actor trigger {trigger} target {} held for its owning scene phase",
                advance.target_stage
            );
        }
        return allowed;
    }
    if !has_finished_scene {
        return true;
    }

    let Some(stages) = world.try_resource::<crate::quest_stages::QuestStageState>() else {
        return true;
    };
    let current_stage = stages.get_stage(advance.owning_quest);
    let next_ready = advances
        .iter()
        .filter(|(_, candidate)| {
            candidate.owning_quest == advance.owning_quest
                && matches!(
                    candidate.activator_gate,
                    crate::papyrus_demo::quest_advance::ActivatorGate::BaseForm(_)
                )
                && candidate.target_stage >= current_stage
                && !stages.get_stage_done(candidate.owning_quest, candidate.target_stage)
        })
        .filter(|(entity, candidate)| {
            crate::condition::evaluate(
                &candidate.conditions,
                world,
                &crate::condition::ConditionContext::for_subject(*entity),
            )
        })
        .map(|(_, candidate)| candidate.target_stage)
        .min();
    let allowed = next_ready == Some(advance.target_stage);
    if !allowed {
        log::debug!(
            "actor trigger {trigger} target {} held between scenes; next ready={next_ready:?}",
            advance.target_stage
        );
    }
    allowed
}

/// The player's world-space translation, via its [`GlobalTransform`].
fn player_world_position(world: &World, player: EntityId) -> Option<Vec3> {
    let q = world.query::<GlobalTransform>()?;
    q.get(player).map(|gt| gt.translation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::OnTriggerEnterEvent;
    use byroredux_core::ecs::components::{GlobalTransform, Transform};
    use byroredux_core::ecs::world::World;

    /// Builds an already-primed volume (known outside on the previous
    /// tick) — the steady-state shape most tests below want. Tests
    /// exercising the fresh-spawn seeding behavior construct
    /// `TriggerVolume` directly with `occupant_inside: None`.
    fn axis_box(center: Vec3, half: Vec3) -> TriggerVolume {
        TriggerVolume {
            center,
            half_extents: half,
            rotation: Quat::IDENTITY,
            shape: TriggerShape::Box,
            occupant_inside: Some(false),
        }
    }

    #[test]
    fn aabb_contains_interior_and_rejects_exterior() {
        let v = axis_box(Vec3::ZERO, Vec3::new(2.0, 1.0, 3.0));
        assert!(v.contains(Vec3::ZERO));
        assert!(v.contains(Vec3::new(1.9, -0.9, 2.9)));
        assert!(v.contains(Vec3::new(2.0, 1.0, 3.0))); // on the face
        assert!(!v.contains(Vec3::new(2.1, 0.0, 0.0)));
        assert!(!v.contains(Vec3::new(0.0, 0.0, 3.5)));
    }

    #[test]
    fn obb_rotation_is_respected() {
        // A box rotated 45° about Y: a point that's outside the AABB on
        // the X axis but inside the rotated box must register inside, and
        // vice-versa.
        let rot = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        let v = TriggerVolume {
            center: Vec3::ZERO,
            half_extents: Vec3::new(1.0, 1.0, 1.0),
            rotation: rot,
            shape: TriggerShape::Box,
            occupant_inside: Some(false),
        };
        // Along the box-local X (rotated), a diagonal world point lands
        // inside; the same distance along world X is past the corner.
        let local_x_dir = rot * Vec3::X; // box's +X in world space
        assert!(v.contains(local_x_dir * 0.9));
        // World-space corner direction at the same radius is outside.
        assert!(!v.contains(Vec3::new(1.4, 0.0, 1.4)));
    }

    #[test]
    fn actor_sphere_contacts_box_even_when_root_point_misses() {
        let v = axis_box(Vec3::ZERO, Vec3::ONE);
        assert!(!v.contains(Vec3::new(1.5, 0.0, 0.0)));
        assert!(v.intersects_sphere(Vec3::new(1.5, 0.0, 0.0), 0.6));
        assert!(!v.intersects_sphere(Vec3::new(1.5, 0.0, 0.0), 0.4));
    }

    #[test]
    fn sphere_contains_by_radius() {
        let v = TriggerVolume {
            center: Vec3::new(5.0, 0.0, 0.0),
            half_extents: Vec3::new(2.0, 0.0, 0.0), // radius 2
            rotation: Quat::IDENTITY,
            shape: TriggerShape::Sphere,
            occupant_inside: Some(false),
        };
        assert!(v.contains(Vec3::new(5.0, 1.9, 0.0)));
        assert!(!v.contains(Vec3::new(5.0, 2.1, 0.0)));
    }

    /// Build a world with a player at `player_pos` and one box trigger,
    /// run the detection system once, and report whether an enter fired.
    fn run_once(player_pos: Vec3, vol: TriggerVolume) -> (World, EntityId, bool) {
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        world.insert(player, Transform::from_translation(player_pos));
        world.insert(
            player,
            GlobalTransform::new(player_pos, Quat::IDENTITY, 1.0),
        );
        world.insert_resource(PlayerEntity(player));
        let trigger = world.spawn();
        world.insert(trigger, vol);

        trigger_detection_system(&world);
        let fired = world.has::<OnTriggerEnterEvent>(trigger);
        (world, trigger, fired)
    }

    #[test]
    fn emits_event_when_player_inside() {
        let (world, trigger, fired) = run_once(Vec3::ZERO, axis_box(Vec3::ZERO, Vec3::splat(1.0)));
        assert!(fired, "player inside an unoccupied volume must emit enter");
        let ev = world.get::<OnTriggerEnterEvent>(trigger).unwrap();
        assert_eq!(ev.triggerers, vec![world.resource::<PlayerEntity>().0]);
    }

    #[test]
    fn no_event_when_player_outside() {
        let (_w, _t, fired) = run_once(
            Vec3::new(10.0, 0.0, 0.0),
            axis_box(Vec3::ZERO, Vec3::splat(1.0)),
        );
        assert!(!fired, "player outside the volume must not emit");
    }

    #[test]
    fn edge_triggered_not_level_triggered() {
        // Steady state: the volume was already known-occupied on the
        // previous tick (not a fresh spawn — see the None-sentinel tests
        // below for that case) and the player is still inside: no re-fire.
        let mut vol = axis_box(Vec3::ZERO, Vec3::splat(1.0));
        vol.occupant_inside = Some(true);
        let (_w, _t, fired) = run_once(Vec3::ZERO, vol);
        assert!(
            !fired,
            "a volume already occupied must not re-fire while still inside",
        );
    }

    /// SCR-D6-NEW-02 / #1817 — a freshly-spawned volume (`occupant_inside:
    /// None`, exactly what the cell loader's `trigger_volume_from_primitive`
    /// produces) with the player already standing inside must NOT fire on
    /// its very first detection tick — that first tick only seeds the
    /// state. This is the actual load-already-inside bug: pre-fix, spawning
    /// with a bare `false` made this indistinguishable from "known outside,
    /// player just crossed in," firing a spurious enter.
    #[test]
    fn fresh_spawn_seeds_silently_even_when_player_already_inside() {
        let vol = TriggerVolume {
            center: Vec3::ZERO,
            half_extents: Vec3::splat(1.0),
            rotation: Quat::IDENTITY,
            shape: TriggerShape::Box,
            occupant_inside: None,
        };
        let (_w, _t, fired) = run_once(Vec3::ZERO, vol);
        assert!(
            !fired,
            "a volume's first-ever detection tick must seed occupant_inside, \
             not fire — a player loading already inside a trigger volume \
             must not see a spurious OnTriggerEnter (#1817)"
        );
    }

    /// Counterpart: a freshly-spawned volume with the player OUTSIDE also
    /// doesn't fire on the seed tick (nothing surprising there), but the
    /// subsequent genuine crossing still fires normally — the seed doesn't
    /// permanently wedge the volume into a non-firing state.
    #[test]
    fn fresh_spawn_still_fires_on_a_later_genuine_crossing() {
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        let outside = Vec3::new(10.0, 0.0, 0.0);
        world.insert(player, Transform::from_translation(outside));
        world.insert(player, GlobalTransform::new(outside, Quat::IDENTITY, 1.0));
        world.insert_resource(PlayerEntity(player));
        let trigger = world.spawn();
        world.insert(
            trigger,
            TriggerVolume {
                center: Vec3::ZERO,
                half_extents: Vec3::splat(1.0),
                rotation: Quat::IDENTITY,
                shape: TriggerShape::Box,
                occupant_inside: None,
            },
        );

        // Seed tick — player outside, nothing fires.
        trigger_detection_system(&world);
        assert!(!world.has::<OnTriggerEnterEvent>(trigger));

        // Genuine crossing — must fire.
        {
            let mut q = world.query_mut::<GlobalTransform>().unwrap();
            q.get_mut(player).unwrap().translation = Vec3::ZERO;
        }
        trigger_detection_system(&world);
        assert!(
            world.has::<OnTriggerEnterEvent>(trigger),
            "a later genuine crossing must still fire after the seed tick"
        );
    }

    #[test]
    fn quest_actor_crossing_emits_triggerer_identity() {
        let mut world = World::new();
        crate::register(&mut world);
        let outside = Vec3::new(10.0, 0.0, 0.0);
        let player = world.spawn();
        world.insert(player, GlobalTransform::new(outside, Quat::IDENTITY, 1.0));
        world.insert_resource(PlayerEntity(player));

        let actor = world.spawn();
        world.insert(actor, GlobalTransform::new(outside, Quat::IDENTITY, 1.0));
        world.insert(
            actor,
            crate::scene::SceneAliasCandidate {
                reference_form_id: 0x100,
                base_form_id: 0x654E5,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        let trigger = world.spawn();
        world.insert(
            trigger,
            TriggerVolume {
                center: Vec3::ZERO,
                half_extents: Vec3::splat(1.0),
                rotation: Quat::IDENTITY,
                shape: TriggerShape::Box,
                occupant_inside: None,
            },
        );

        trigger_detection_system(&world); // silently seed both occupants
        world
            .query_mut::<GlobalTransform>()
            .unwrap()
            .get_mut(actor)
            .unwrap()
            .translation = Vec3::ZERO;
        trigger_detection_system(&world);

        assert_eq!(
            world
                .get::<OnTriggerEnterEvent>(trigger)
                .expect("NPC crossing event")
                .triggerers,
            vec![actor]
        );
    }

    #[test]
    fn freshly_streamed_trigger_emits_for_tethered_horse_already_inside() {
        let mut world = World::new();
        crate::register(&mut world);
        let outside = Vec3::new(10.0, 0.0, 0.0);
        let player = world.spawn();
        world.insert(player, GlobalTransform::new(outside, Quat::IDENTITY, 1.0));
        world.insert_resource(PlayerEntity(player));

        let horse = world.spawn();
        world.insert(horse, GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert(
            horse,
            crate::scene::SceneAliasCandidate {
                reference_form_id: 0x100,
                base_form_id: 0x654E5,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        let cart = world.spawn();
        world.insert(
            cart,
            crate::HorseTetherState {
                horse,
                horse_local_translation: Vec3::ZERO,
                horse_local_rotation: Quat::IDENTITY,
                route_target_form_id: None,
            },
        );
        let trigger = world.spawn();
        world.insert(
            trigger,
            TriggerVolume {
                center: Vec3::ZERO,
                half_extents: Vec3::splat(1.0),
                rotation: Quat::IDENTITY,
                shape: TriggerShape::Box,
                occupant_inside: None,
            },
        );

        trigger_detection_system(&world);

        assert_eq!(
            world
                .get::<OnTriggerEnterEvent>(trigger)
                .expect("streamed tethered-horse entry")
                .triggerers,
            vec![horse]
        );
    }

    #[test]
    fn tethered_horse_inside_reemits_when_quest_prerequisite_becomes_ready() {
        use crate::papyrus_demo::quest_advance::{ActivatorGate, QuestAdvanceOnActivate};
        use crate::quest_stages::{QuestFormId, QuestStageState};
        use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};

        let mut world = World::new();
        crate::register(&mut world);
        world.insert_resource(QuestStageState::default());
        let player = world.spawn();
        world.insert(
            player,
            GlobalTransform::new(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert_resource(PlayerEntity(player));

        let horse = world.spawn();
        world.insert(horse, GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert(
            horse,
            crate::scene::SceneAliasCandidate {
                reference_form_id: 0x100,
                base_form_id: 0xB9E1D,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        let cart = world.spawn();
        world.insert(
            cart,
            crate::HorseTetherState {
                horse,
                horse_local_translation: Vec3::ZERO,
                horse_local_rotation: Quat::IDENTITY,
                route_target_form_id: None,
            },
        );
        let trigger = world.spawn();
        world.insert(
            trigger,
            TriggerVolume {
                center: Vec3::ZERO,
                half_extents: Vec3::splat(1.0),
                rotation: Quat::IDENTITY,
                shape: TriggerShape::Box,
                occupant_inside: None,
            },
        );
        let quest = QuestFormId(0x3372B);
        world.insert(
            trigger,
            QuestAdvanceOnActivate {
                owning_quest: quest,
                conditions: vec![Condition {
                    function_index: 59,
                    comparator: ComparisonOp::Eq,
                    comparand: ConditionValue::Literal(1.0),
                    param_1: quest.0,
                    param_2: 20,
                    ..Default::default()
                }],
                target_stage: 30,
                activator_gate: ActivatorGate::BaseForm(0xB9E1D),
                disable_after_advance: true,
            },
        );

        trigger_detection_system(&world);
        world.remove::<OnTriggerEnterEvent>(trigger);
        assert!(world.get::<OnTriggerEnterEvent>(trigger).is_none());

        world.resource_mut::<QuestStageState>().set_stage(quest, 20);
        trigger_detection_system(&world);

        assert_eq!(
            world
                .get::<OnTriggerEnterEvent>(trigger)
                .expect("late-ready tethered-horse entry")
                .triggerers,
            vec![horse]
        );
    }

    #[test]
    fn preserves_all_actors_entering_one_volume_in_the_same_frame() {
        let mut world = World::new();
        crate::register(&mut world);
        let outside = Vec3::new(10.0, 0.0, 0.0);
        let player = world.spawn();
        world.insert(player, GlobalTransform::new(outside, Quat::IDENTITY, 1.0));
        world.insert_resource(PlayerEntity(player));

        let actors: Vec<_> = (0..2)
            .map(|index| {
                let actor = world.spawn();
                world.insert(actor, GlobalTransform::new(outside, Quat::IDENTITY, 1.0));
                world.insert(
                    actor,
                    crate::scene::SceneAliasCandidate {
                        reference_form_id: 0x100 + index,
                        base_form_id: 0x200 + index,
                        linked_refs: Vec::new(),
                        location_ref_types: Vec::new(),
                    },
                );
                actor
            })
            .collect();
        let trigger = world.spawn();
        world.insert(
            trigger,
            TriggerVolume {
                center: Vec3::ZERO,
                half_extents: Vec3::splat(1.0),
                rotation: Quat::IDENTITY,
                shape: TriggerShape::Box,
                occupant_inside: None,
            },
        );
        trigger_detection_system(&world);
        for actor in &actors {
            world
                .query_mut::<GlobalTransform>()
                .unwrap()
                .get_mut(*actor)
                .unwrap()
                .translation = Vec3::ZERO;
        }

        trigger_detection_system(&world);

        let event = world.get::<OnTriggerEnterEvent>(trigger).unwrap();
        assert_eq!(event.triggerers.len(), 2);
        assert!(actors.iter().all(|actor| event.triggerers.contains(actor)));
    }

    #[test]
    fn actor_triggers_follow_scene_phase_and_between_scene_stage_order() {
        use crate::papyrus_demo::quest_advance::{ActivatorGate, QuestAdvanceOnActivate};
        use crate::quest_stages::{QuestFormId, QuestStageState};
        use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue};
        use byroredux_plugin::esm::records::{ScenRecord, ScenePhase};

        let mut world = World::new();
        crate::register(&mut world);
        let quest = QuestFormId(0x3372B);
        world.insert_resource(QuestStageState::default());
        crate::install_scene_records(
            &mut world,
            [ScenRecord {
                form_id: 0xBECD4,
                quest_form_id: Some(quest.0),
                phases: vec![
                    ScenePhase::default(),
                    ScenePhase {
                        completion_conditions: vec![Condition {
                            function_index: 59,
                            comparator: ComparisonOp::Eq,
                            comparand: ConditionValue::Literal(1.0),
                            param_1: quest.0,
                            param_2: 37,
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
        );
        let scene = world
            .resource::<crate::SceneRegistry>()
            .scene_entity(0xBECD4)
            .unwrap();
        world.get_mut::<crate::ScenePlayer>(scene).unwrap().state =
            crate::ScenePlaybackState::Finished;
        let trigger_32 = world.spawn();
        let trigger_37 = world.spawn();
        for (trigger, target_stage) in [(trigger_32, 32), (trigger_37, 37)] {
            world.insert(
                trigger,
                QuestAdvanceOnActivate {
                    owning_quest: quest,
                    conditions: Vec::new(),
                    target_stage,
                    activator_gate: ActivatorGate::BaseForm(0xB9E1D),
                    disable_after_advance: true,
                },
            );
        }
        {
            let mut stages = world.resource_mut::<QuestStageState>();
            stages.start_quest(quest, Some(0));
            stages.set_stage(quest, 26);
        }

        assert!(actor_quest_trigger_is_in_sequence(&world, trigger_32));
        assert!(!actor_quest_trigger_is_in_sequence(&world, trigger_37));

        {
            let player = world.get_mut::<crate::ScenePlayer>(scene).unwrap();
            player.state = crate::ScenePlaybackState::Playing;
            player.current_phase = 0;
        }
        assert!(!actor_quest_trigger_is_in_sequence(&world, trigger_37));
        world
            .get_mut::<crate::ScenePlayer>(scene)
            .unwrap()
            .current_phase = 1;
        assert!(actor_quest_trigger_is_in_sequence(&world, trigger_37));
    }

    #[test]
    fn re_entry_fires_again() {
        // Inside → emits and sets occupied; stays inside → silent; moves
        // out → clears; comes back → fires again.
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        world.insert(player, Transform::from_translation(Vec3::ZERO));
        world.insert(
            player,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert_resource(PlayerEntity(player));
        let trigger = world.spawn();
        world.insert(trigger, axis_box(Vec3::ZERO, Vec3::splat(1.0)));

        let move_player = |world: &World, p: Vec3| {
            let mut q = world.query_mut::<GlobalTransform>().unwrap();
            q.get_mut(player).unwrap().translation = p;
        };
        let clear_event = |world: &World| {
            if world.has::<OnTriggerEnterEvent>(trigger) {
                world
                    .query_mut::<OnTriggerEnterEvent>()
                    .unwrap()
                    .remove(trigger);
            }
        };

        // Frame 1 — inside: fires.
        trigger_detection_system(&world);
        assert!(world.has::<OnTriggerEnterEvent>(trigger));
        clear_event(&world);

        // Frame 2 — still inside: no re-fire.
        trigger_detection_system(&world);
        assert!(!world.has::<OnTriggerEnterEvent>(trigger));

        // Frame 3 — moved out: no fire, occupancy clears.
        move_player(&world, Vec3::new(10.0, 0.0, 0.0));
        trigger_detection_system(&world);
        assert!(!world.has::<OnTriggerEnterEvent>(trigger));

        // Frame 4 — back inside: fires again.
        move_player(&world, Vec3::ZERO);
        trigger_detection_system(&world);
        assert!(world.has::<OnTriggerEnterEvent>(trigger));
    }
}
