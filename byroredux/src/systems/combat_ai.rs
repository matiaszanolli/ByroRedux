//! NPC combat AI (M?? — MQ101's dragon-attack/keep-escape combat gate,
//! ROADMAP.md's "MQ101 end-to-end playability" entry). `Faction.SetEnemy`/
//! `Actor.StartCombat` had no runtime consumer at all before this: this
//! is the smallest slice that gives an `AiCombatState`-armed actor
//! (`byroredux_scripting::AiCombatState`, installed by `Effect::StartCombat`)
//! behavior a player can see and be a party to — chase in a straight line,
//! strike on cooldown once in range — reusing the exact same `HitEvent` ->
//! `combat_damage_system` damage/death pipeline the player's own melee
//! already goes through, rather than a second damage-application path.
//!
//! ## v0 scope (documented approximations, mirroring `wander.rs`'s style)
//!
//! - **No NAVM routing.** Straight-line [`super::locomotion::step_toward`]
//!   only, same as `wander.rs`'s own cross-tile fallback. A wall between
//!   attacker and target is not routed around.
//! - **No animation.** Transform moves, pose doesn't — same accepted gap
//!   `wander.rs` documents for the same reason (no verified attack/run
//!   clip wired to this path yet).
//! - **No death/target-loss reaction beyond stopping.** An attacker whose
//!   target dies or despawns simply drops `AiCombatState`; it does not
//!   flee, alert allies, or pick a new target (no faction-wide aggro
//!   propagation — see [`byroredux_scripting::FactionRelations`]'s doc for
//!   why that is a separate, larger AI-perception feature).
//! - **No blocking/power-attack/sneak-attack.** Every strike is a flat
//!   `HitEvent` at [`crate::combat::attack_damage`]'s resolved value —
//!   the same fields the player's own unarmed swing leaves `false`.

use super::locomotion::{step_toward, LOCOMOTION_WALK_SPEED};
use byroredux_core::ecs::components::{Dead, Transform};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_scripting::AiCombatState;

struct Decision {
    entity: EntityId,
    new_translation: Vec3,
    new_rotation: Option<Quat>,
    /// `None` clears `AiCombatState` (target died/despawned); `Some`
    /// carries the state forward, cooldown updated.
    state: Option<AiCombatState>,
    /// `(target, damage)` when this tick's cooldown reached zero in range.
    strike: Option<(EntityId, f32)>,
}

/// Mirrors `wander_system_inner`'s read/write split: gather a decision per
/// `AiCombatState` entity while holding storage read guards, resolve the
/// chase steps against `PhysicsWorld` with no storage guard held, then apply
/// `HitEvent`/movement/state.
///
/// #4325 — `PhysicsWorld` used to be taken first and held across every
/// storage acquisition, the `Transform` write included. That reversed
/// `ragdoll_writeback_system`'s `Transform → PhysicsWorld` order and closed a
/// cycle in the lock-order checker — the "no storage under a `PhysicsWorld`
/// guard" rule #2134, #3262 and #3655 restructured their systems to follow.
pub(crate) fn npc_combat_ai_system(world: &World, dt: f32) {
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    let mut decisions = Vec::new();
    // `(decision index, from, rotation, target_xz)` for each attacker still
    // closing the distance, stepped once the storage guards are gone.
    let mut steps = Vec::new();
    // #4605 — per-attacker snapshot for the guard-free second pass:
    // (entity, actor translation, actor rotation, target translation,
    // distance², combat state).
    let mut pending: Vec<(EntityId, Vec3, Quat, Vec3, f32, AiCombatState)> = Vec::new();
    {
        let Some(combat_q) = world.query::<AiCombatState>() else {
            return;
        };
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        for (entity, state) in combat_q.iter() {
            if world.get::<Dead>(entity).is_some() {
                // The attacker itself died (e.g. to the player's own
                // counter-swing) — drop its combat state rather than
                // leaving a dead actor's AiCombatState to be iterated
                // forever. #4693: the decision carries the actor's
                // CURRENT translation, not `Vec3::ZERO` — the write pass
                // applies every decision unconditionally, and the zero
                // teleported the corpse root to world origin, where the
                // saved mutable delta persisted it across save/load.
                // (Death teardown in `reconcile_dead_actor` removes the
                // state itself now; this branch remains the fallback.)
                let current = transform_q
                    .get(entity)
                    .map(|t| t.translation)
                    .unwrap_or_default();
                decisions.push(Decision {
                    entity,
                    new_translation: current,
                    new_rotation: None,
                    state: None,
                    strike: None,
                });
                continue;
            }
            let Some(actor_transform) = transform_q.get(entity).copied() else {
                continue;
            };
            let target_transform = (world.get::<Dead>(state.target).is_none())
                .then(|| transform_q.get(state.target).copied())
                .flatten();
            let Some(target_transform) = target_transform else {
                decisions.push(Decision {
                    entity,
                    new_translation: actor_transform.translation,
                    new_rotation: None,
                    state: None,
                    strike: None,
                });
                continue;
            };

            let to_target = target_transform.translation - actor_transform.translation;
            // #4605 — reach, damage and cooldown resolve in the SECOND pass
            // (`resolve_pending_strikes` below): computing them here held
            // the AiCombatState/Transform guards across the CHARAL chain
            // (`attack_damage` → `MeleeDamageConfig` → `CharacterRuleset`
            // → `ActorValues` → `CharacterLevel`), the five-deep hold stack
            // #3473's fix removed from the callee — this caller re-created
            // it one level up. The pending entry carries what the second
            // pass needs; no storage guard is live there.
            pending.push((
                entity,
                actor_transform.translation,
                actor_transform.rotation,
                target_transform.translation,
                to_target.length_squared(),
                *state,
            ));
        }
    }

    /// Second pass: with every storage guard dropped, resolve the per-
    /// attacker questions the read pass deferred (#4605 — the #2270
    /// "snapshot before you iterate" rule applied to the helpers that
    /// walk the CHARAL chain).
    fn resolve_pending_strikes(
        world: &World,
        dt: f32,
        pending: &[(EntityId, Vec3, Quat, Vec3, f32, AiCombatState)],
        decisions: &mut Vec<Decision>,
        steps: &mut Vec<(usize, Vec3, Quat, Vec3, f32)>,
    ) {
        for &(entity, actor_translation, actor_rotation, target_translation, dist_sq, state) in
            pending
        {
            let reach = crate::combat::attack_reach_bu(world, entity);
            if dist_sq > reach * reach {
                let target_xz = Vec3::new(
                    target_translation.x,
                    actor_translation.y,
                    target_translation.z,
                );
                // M42.11 — chase at the actor's authored stride when a
                // walk clip derived one; engine default otherwise.
                let speed = world
                    .query::<crate::components::WalkSpeed>()
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|s| s.0)
                    .unwrap_or(LOCOMOTION_WALK_SPEED);
                steps.push((
                    decisions.len(),
                    actor_translation,
                    actor_rotation,
                    target_xz,
                    speed,
                ));
                decisions.push(Decision {
                    entity,
                    new_translation: actor_translation,
                    new_rotation: None,
                    state: Some(state),
                    strike: None,
                });
            } else if state.attack_cooldown_remaining <= 0.0 {
                let damage = crate::combat::attack_damage(world, entity);
                decisions.push(Decision {
                    entity,
                    new_translation: actor_translation,
                    new_rotation: None,
                    state: Some(AiCombatState {
                        target: state.target,
                        attack_cooldown_remaining: crate::combat::attack_cooldown_seconds(
                            world, entity,
                        ),
                    }),
                    strike: Some((state.target, damage)),
                });
            } else {
                decisions.push(Decision {
                    entity,
                    new_translation: actor_translation,
                    new_rotation: None,
                    state: Some(AiCombatState {
                        target: state.target,
                        attack_cooldown_remaining: state.attack_cooldown_remaining - dt,
                    }),
                    strike: None,
                });
            }
        }
    }

    // #4605 — the second pass runs with NO storage guard live.
    resolve_pending_strikes(world, dt, &pending, &mut decisions, &mut steps);

    if decisions.is_empty() {
        return;
    }

    if !steps.is_empty() {
        let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();
        for (index, from, rotation, target_xz, speed) in steps {
            let (new_pos, new_rotation) =
                step_toward(from, rotation, target_xz, dt, speed, physics.as_deref());
            decisions[index].new_translation = new_pos;
            decisions[index].new_rotation = new_rotation;
        }
    }

    // #4324 — `HitEvent` is one SparseSet row per *target* and `insert`
    // replaces it. `combat_input_system` runs first, so a strike on a target
    // the player (or an earlier attacker in this loop) already hit this frame
    // used to overwrite that hit before `combat_damage_system` read it. Such a
    // strike is held instead: the attacker stays ready and lands it next
    // frame, so no hit is lost and simultaneous attackers fall out of step.
    if let Some(mut events) = world.query_mut::<byroredux_scripting::HitEvent>() {
        for decision in &mut decisions {
            let Some((target, damage)) = decision.strike else {
                continue;
            };
            if events.get(target).is_some() {
                decision.strike = None;
                decision.state = Some(AiCombatState {
                    target,
                    attack_cooldown_remaining: 0.0,
                });
                continue;
            }
            events.insert(
                target,
                byroredux_scripting::HitEvent {
                    aggressor: decision.entity,
                    source: decision.entity,
                    projectile: 0,
                    damage,
                    power_attack: false,
                    sneak_attack: false,
                    bash_attack: false,
                    blocked: false,
                },
            );
        }
    }
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for decision in &decisions {
            if let Some(transform) = transforms.get_mut(decision.entity) {
                transform.translation = decision.new_translation;
                if let Some(rotation) = decision.new_rotation {
                    transform.rotation = rotation;
                }
            }
        }
    }
    if let Some(mut states) = world.query_mut::<AiCombatState>() {
        for decision in &decisions {
            match decision.state {
                Some(state) => states.insert(decision.entity, state),
                None => {
                    states.remove(decision.entity);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{ActorVitals, Dead};
    use byroredux_scripting::HitEvent;

    fn fixture() -> World {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<Transform>();
        world.register::<Dead>();
        world.register::<ActorVitals>();
        world
    }

    #[test]
    fn chases_target_out_of_range_without_striking() {
        let mut world = fixture();
        let attacker = world.spawn();
        let target = world.spawn();
        world.insert(attacker, Transform::from_translation(Vec3::ZERO));
        world.insert(target, Transform::from_translation(Vec3::new(1000.0, 0.0, 0.0)));
        world.insert(
            attacker,
            AiCombatState {
                target,
                attack_cooldown_remaining: 0.0,
            },
        );

        npc_combat_ai_system(&world, 1.0);

        let moved = world.get::<Transform>(attacker).unwrap().translation;
        assert!(moved.x > 0.0, "attacker must step toward the target");
        assert!(
            world.get::<HitEvent>(target).is_none(),
            "still out of melee range — no strike yet"
        );
        assert!(
            world.get::<AiCombatState>(attacker).is_some(),
            "combat continues while the target is alive and reachable"
        );
    }

    #[test]
    fn strikes_immediately_when_already_in_range_with_zero_cooldown() {
        let mut world = fixture();
        let attacker = world.spawn();
        let target = world.spawn();
        world.insert(attacker, Transform::from_translation(Vec3::ZERO));
        world.insert(target, Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)));
        world.insert(
            attacker,
            AiCombatState {
                target,
                attack_cooldown_remaining: 0.0,
            },
        );

        npc_combat_ai_system(&world, 1.0 / 60.0);

        let event = world
            .get::<HitEvent>(target)
            .expect("in-range zero-cooldown attacker must strike this tick");
        assert_eq!(event.aggressor, attacker);
        assert!(event.damage > 0.0);
        let state = world
            .get::<AiCombatState>(attacker)
            .expect("combat state persists after a strike");
        assert!(
            state.attack_cooldown_remaining > 0.0,
            "cooldown must be armed after striking"
        );
    }

    #[test]
    fn ticks_cooldown_down_while_in_range_and_not_ready() {
        let mut world = fixture();
        let attacker = world.spawn();
        let target = world.spawn();
        world.insert(attacker, Transform::from_translation(Vec3::ZERO));
        world.insert(target, Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)));
        world.insert(
            attacker,
            AiCombatState {
                target,
                attack_cooldown_remaining: 1.0,
            },
        );

        npc_combat_ai_system(&world, 0.25);

        assert!(world.get::<HitEvent>(target).is_none(), "still on cooldown");
        let state = world.get::<AiCombatState>(attacker).unwrap();
        assert!((state.attack_cooldown_remaining - 0.75).abs() < 1e-6);
    }

    #[test]
    fn clears_combat_state_when_target_dies() {
        let mut world = fixture();
        let attacker = world.spawn();
        let target = world.spawn();
        world.insert(attacker, Transform::from_translation(Vec3::ZERO));
        world.insert(target, Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)));
        world.insert(target, Dead);
        world.insert(
            attacker,
            AiCombatState {
                target,
                attack_cooldown_remaining: 0.0,
            },
        );

        npc_combat_ai_system(&world, 1.0);

        assert!(
            world.get::<AiCombatState>(attacker).is_none(),
            "combat must end once the target is dead"
        );
        assert!(world.get::<HitEvent>(target).is_none());
    }

    #[test]
    fn clears_combat_state_when_attacker_dies() {
        let mut world = fixture();
        let attacker = world.spawn();
        let target = world.spawn();
        // #4693 — away from the origin: the pre-fix dead-attacker decision
        // wrote `Vec3::ZERO` unconditionally, teleporting the corpse root
        // (a saved mutable delta) to world origin, and this test spawned
        // the attacker AT the origin so the wrong write was unobservable.
        let away = Vec3::new(-412.5, 0.0, 918.25);
        world.insert(attacker, Transform::from_translation(away));
        world.insert(attacker, Dead);
        world.insert(target, Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)));
        world.insert(
            attacker,
            AiCombatState {
                target,
                attack_cooldown_remaining: 0.0,
            },
        );

        npc_combat_ai_system(&world, 1.0);

        assert!(world.get::<AiCombatState>(attacker).is_none());
        assert_eq!(
            world.get::<Transform>(attacker).unwrap().translation,
            away,
            "a dead attacker's placement must be left exactly where it was \
             — the zeroed write persisted the corpse at world origin in \
             saves (#4693)"
        );
    }

    /// #4693 SIBLING — the death teardown itself removes `AiCombatState`
    /// (the system's own branch above is only the one-tick fallback), so
    /// the state cannot survive past `reconcile_dead_actor`.
    #[test]
    fn reconcile_dead_actor_removes_ai_combat_state() {
        let mut world = fixture();
        let actor = world.spawn();
        world.insert(
            actor,
            AiCombatState {
                target: actor,
                attack_cooldown_remaining: 0.0,
            },
        );
        world.insert(actor, Dead);

        crate::combat::reconcile_dead_actor(&world, actor);

        assert!(world.get::<AiCombatState>(actor).is_none());
    }

    fn hit_from(aggressor: EntityId) -> HitEvent {
        HitEvent {
            aggressor,
            source: aggressor,
            projectile: 0,
            damage: 5.0,
            power_attack: false,
            sneak_attack: false,
            bash_attack: false,
            blocked: false,
        }
    }

    /// End-of-frame `event_cleanup_system` stand-in.
    fn drain_hits(world: &World, target: EntityId) {
        world.query_mut::<HitEvent>().unwrap().remove(target);
    }

    /// #4324 — the player's `combat_input_system` hit lands earlier in the
    /// same frame; an NPC strike on the same target must not replace it.
    #[test]
    fn a_same_frame_hit_on_the_target_survives_and_the_strike_lands_next_frame() {
        let mut world = fixture();
        let player = world.spawn();
        let attacker = world.spawn();
        let target = world.spawn();
        world.insert(attacker, Transform::from_translation(Vec3::ZERO));
        world.insert(
            target,
            Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
        );
        world.insert(
            attacker,
            AiCombatState {
                target,
                attack_cooldown_remaining: 0.0,
            },
        );
        world.insert(target, hit_from(player));

        npc_combat_ai_system(&world, 1.0 / 60.0);
        assert_eq!(
            world.get::<HitEvent>(target).unwrap().aggressor,
            player,
            "the player's same-frame hit must not be overwritten"
        );
        assert!(
            world
                .get::<AiCombatState>(attacker)
                .unwrap()
                .attack_cooldown_remaining
                <= 0.0,
            "the held strike stays ready"
        );

        drain_hits(&world, target);
        npc_combat_ai_system(&world, 1.0 / 60.0);
        assert_eq!(world.get::<HitEvent>(target).unwrap().aggressor, attacker);
    }

    /// #4324 — two attackers armed on one fragment strike the same frame;
    /// both hits must land rather than the second replacing the first.
    #[test]
    fn simultaneous_attackers_on_one_target_all_land() {
        let mut world = fixture();
        let target = world.spawn();
        world.insert(
            target,
            Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
        );
        let attackers = [world.spawn(), world.spawn()];
        for attacker in attackers {
            world.insert(attacker, Transform::from_translation(Vec3::ZERO));
            world.insert(
                attacker,
                AiCombatState {
                    target,
                    attack_cooldown_remaining: 0.0,
                },
            );
        }

        let mut landed = Vec::new();
        for _ in 0..2 {
            npc_combat_ai_system(&world, 1.0 / 60.0);
            landed.push(world.get::<HitEvent>(target).unwrap().aggressor);
            drain_hits(&world, target);
        }
        landed.sort_unstable();
        let mut expected = attackers.to_vec();
        expected.sort_unstable();
        assert_eq!(landed, expected, "each attacker's hit lands exactly once");
    }

    /// #4325 — `PhysicsWorld` must be acquired after the storage read guards
    /// drop and released before any storage write, never held across one.
    #[test]
    fn physics_world_is_taken_only_between_the_storage_passes() {
        const SRC: &str = include_str!("combat_ai.rs");
        let body = SRC
            .split_once("pub(crate) fn npc_combat_ai_system")
            .expect("npc_combat_ai_system definition")
            .1
            .split_once("#[cfg(test)]")
            .expect("test module marker")
            .0;
        let last_read = body.rfind("world.query::<").expect("the storage read pass");
        let physics = body
            .find("world.try_resource::<byroredux_physics::PhysicsWorld>()")
            .expect("the PhysicsWorld acquisition");
        let first_write = body
            .find("world.query_mut::<")
            .expect("the storage write pass");
        assert!(
            last_read < physics && physics < first_write,
            "PhysicsWorld must sit strictly between the read and write passes (#4325)"
        );
    }

    #[test]
    fn no_combat_states_is_a_cheap_no_op() {
        let world = fixture();
        // Must not panic even with no AiCombatState storage populated.
        npc_combat_ai_system(&world, 1.0 / 60.0);
    }
}
