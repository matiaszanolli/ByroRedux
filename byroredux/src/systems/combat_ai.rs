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

use super::locomotion::step_toward;
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

/// Two-pass, mirroring `wander_system_inner`'s read/write split: gather a
/// decision per `AiCombatState` entity while holding read guards, then
/// apply movement/state/`HitEvent` after they drop.
pub(crate) fn npc_combat_ai_system(world: &World, dt: f32) {
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();
    let mut decisions = Vec::new();
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
                // forever.
                decisions.push(Decision {
                    entity,
                    new_translation: Vec3::ZERO,
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
            let reach = crate::combat::attack_reach_bu(world, entity);
            if to_target.length_squared() > reach * reach {
                let target_xz = Vec3::new(
                    target_transform.translation.x,
                    actor_transform.translation.y,
                    target_transform.translation.z,
                );
                let (new_pos, rotation) = step_toward(
                    actor_transform.translation,
                    actor_transform.rotation,
                    target_xz,
                    dt,
                    physics.as_deref(),
                );
                decisions.push(Decision {
                    entity,
                    new_translation: new_pos,
                    new_rotation: rotation,
                    state: Some(*state),
                    strike: None,
                });
            } else if state.attack_cooldown_remaining <= 0.0 {
                let damage = crate::combat::attack_damage(world, entity);
                decisions.push(Decision {
                    entity,
                    new_translation: actor_transform.translation,
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
                    new_translation: actor_transform.translation,
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

    if decisions.is_empty() {
        return;
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
    if let Some(mut events) = world.query_mut::<byroredux_scripting::HitEvent>() {
        for decision in &decisions {
            if let Some((target, damage)) = decision.strike {
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
        world.insert(attacker, Transform::from_translation(Vec3::ZERO));
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
    }

    #[test]
    fn no_combat_states_is_a_cheap_no_op() {
        let world = fixture();
        // Must not panic even with no AiCombatState storage populated.
        npc_combat_ai_system(&world, 1.0 / 60.0);
    }
}
