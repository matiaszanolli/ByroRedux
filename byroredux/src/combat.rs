//! First playable melee-combat vertical slice.
//!
//! A physical Attack action casts from the active camera, resolves a hit
//! ragdoll bone through [`ActorColliderOwner`], and emits the canonical
//! scripting [`HitEvent`]. A same-frame consumer applies weapon/unarmed
//! damage to the target's Health actor value and owns the alive→dead
//! transition. Transient HitEvent cleanup remains in the scripting Late stage.

use byroredux_core::animation::AnimationPlayer;
use byroredux_core::ecs::components::{ActorValues, ActorVitals, Dead, EquippedWeapon};
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::{Resource, World};

use crate::components::HavokAnimationTarget;
use crate::interaction::{camera_ray, ActionState, InputAction};
use crate::systems::{PlayerEntity, PlayerMode};

/// Camera-forward melee reach in Bethesda units.
pub(crate) const MELEE_REACH_BU: f32 = 180.0;
/// One attack edge per cooldown. Holding Attack never auto-repeats because
/// ActionState contributes only the initial press edge.
pub(crate) const MELEE_COOLDOWN_SECONDS: f32 = 0.45;
/// Authored player records may start without a weapon. Keep that state
/// playable with one explicit unarmed damage rule instead of inventing an
/// item/equipment record.
pub(crate) const UNARMED_DAMAGE: f32 = 8.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CombatTraceEntry {
    pub(crate) target: Option<EntityId>,
    pub(crate) damage: f32,
    pub(crate) health_before: Option<f32>,
    pub(crate) health_after: Option<f32>,
    pub(crate) killed: bool,
    pub(crate) outcome: String,
}

/// Runtime combat timing and smoke-test evidence.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CombatState {
    pub(crate) cooldown_remaining: f32,
    pub(crate) blocking: bool,
    pub(crate) attacks_started: u64,
    pub(crate) hits_landed: u64,
    pub(crate) kills: u64,
    pub(crate) last: Option<CombatTraceEntry>,
}

impl Default for CombatState {
    fn default() -> Self {
        Self {
            cooldown_remaining: 0.0,
            blocking: false,
            attacks_started: 0,
            hits_landed: 0,
            kills: 0,
            last: None,
        }
    }
}

impl Resource for CombatState {}

/// Physical Attack/Block action frontend and melee-hit producer.
pub(crate) fn combat_input_system(world: &World, dt: f32) {
    let (attack_pressed, block_held) = world
        .try_resource::<ActionState>()
        .map(|actions| {
            (
                actions.was_pressed(InputAction::Attack),
                actions.is_held(InputAction::Block),
            )
        })
        .unwrap_or((false, false));

    let attack_ready = if let Some(mut state) = world.try_resource_mut::<CombatState>() {
        state.blocking = block_held;
        state.cooldown_remaining = (state.cooldown_remaining - dt.max(0.0)).max(0.0);
        if attack_pressed && state.cooldown_remaining <= 0.0 {
            state.cooldown_remaining = MELEE_COOLDOWN_SECONDS;
            state.attacks_started = state.attacks_started.saturating_add(1);
            true
        } else {
            false
        }
    } else {
        false
    };
    if !attack_ready
        || !world
            .try_resource::<PlayerMode>()
            .is_some_and(|mode| *mode == PlayerMode::Character)
    {
        return;
    }

    let Some(aggressor) = world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0)
    else {
        record_miss(world, "no player entity");
        return;
    };
    let Some((origin, direction)) = camera_ray(world) else {
        record_miss(world, "no active camera ray");
        return;
    };

    // Resolve Rapier body ownership before acquiring PhysicsWorld. This keeps
    // component/resource guards non-overlapping and follows the interaction
    // ray's lock order.
    let (excluded_body, owners) = match world.query::<byroredux_physics::RapierHandles>() {
        Some(handles) => {
            let excluded = handles.get(aggressor).map(|handles| handles.body);
            let owners = handles
                .iter()
                .map(|(entity, handles)| (entity, handles.body))
                .collect::<Vec<_>>();
            (excluded, owners)
        }
        None => (None, Vec::new()),
    };
    let hit = world
        .try_resource::<byroredux_physics::PhysicsWorld>()
        .and_then(|physics| physics.cast_ray(origin, direction, MELEE_REACH_BU, excluded_body));
    let Some(hit_body) = hit.and_then(|hit| hit.body) else {
        record_miss(world, "melee swing missed");
        return;
    };
    let Some(collider_entity) = owners
        .iter()
        .find_map(|(entity, body)| (*body == hit_body).then_some(*entity))
    else {
        record_miss(world, "hit body has no ECS owner");
        return;
    };
    let Some(target) = resolve_actor_root(world, collider_entity) else {
        record_miss(world, "first obstruction is not an actor");
        return;
    };
    if target == aggressor || world.get::<Dead>(target).is_some() {
        record_miss(world, "actor target is invalid or already dead");
        return;
    }

    let damage = attack_damage(world, aggressor);
    let Some(mut events) = world.query_mut::<byroredux_scripting::HitEvent>() else {
        record_miss(world, "HitEvent storage is not registered");
        return;
    };
    events.insert(
        target,
        byroredux_scripting::HitEvent {
            aggressor,
            // Equipped weapons are inventory records rather than standalone
            // ECS entities today. Use the aggressor as the source until item
            // instances acquire stable entities; damage was snapshotted into
            // the trace and is re-read same-frame by the consumer.
            source: aggressor,
            projectile: 0,
            power_attack: false,
            sneak_attack: false,
            bash_attack: false,
            blocked: false,
        },
    );
    drop(events);
    if let Some(mut state) = world.try_resource_mut::<CombatState>() {
        state.last = Some(CombatTraceEntry {
            target: Some(target),
            damage,
            health_before: None,
            health_after: None,
            killed: false,
            outcome: format!("HitEvent queued from collider {collider_entity}"),
        });
    }
}

/// Same-frame HitEvent → Health damage → death transition.
pub(crate) fn combat_damage_system(world: &World, _dt: f32) {
    let events: Vec<(EntityId, byroredux_scripting::HitEvent)> = world
        .query::<byroredux_scripting::HitEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, event)| (entity, *event))
                .collect()
        })
        .unwrap_or_default();

    for (target, event) in events {
        if world.get::<Dead>(target).is_some() {
            continue;
        }
        let Some(vitals) = world.get::<ActorVitals>(target).map(|vitals| *vitals) else {
            continue;
        };
        let damage = if event.blocked {
            0.0
        } else {
            attack_damage(world, event.aggressor)
        };
        let Some((before, after)) = apply_health_damage(world, target, vitals.health, damage)
        else {
            continue;
        };
        let killed = after <= 0.0;
        let mut outcome = format!("health {:.1} -> {:.1}", before, after);

        if killed {
            if let Some(mut dead) = world.query_mut::<Dead>() {
                dead.insert(target, Dead);
            }
            outcome.push_str(&reconcile_dead_actor(world, target));
        }

        if let Some(mut state) = world.try_resource_mut::<CombatState>() {
            state.hits_landed = state.hits_landed.saturating_add(1);
            if killed {
                state.kills = state.kills.saturating_add(1);
            }
            state.last = Some(CombatTraceEntry {
                target: Some(target),
                damage,
                health_before: Some(before),
                health_after: Some(after),
                killed,
                outcome,
            });
        }
    }
}

fn resolve_actor_root(world: &World, collider_entity: EntityId) -> Option<EntityId> {
    world
        .get::<byroredux_physics::ActorColliderOwner>(collider_entity)
        .map(|owner| owner.0)
        .or_else(|| {
            world
                .get::<ActorVitals>(collider_entity)
                .map(|_| collider_entity)
        })
        .filter(|actor| world.get::<ActorVitals>(*actor).is_some())
}

fn attack_damage(world: &World, aggressor: EntityId) -> f32 {
    world
        .get::<EquippedWeapon>(aggressor)
        .map_or(UNARMED_DAMAGE, |weapon| weapon.damage.max(0.0))
}

fn apply_health_damage(
    world: &World,
    target: EntityId,
    health_form_id: u32,
    damage: f32,
) -> Option<(f32, f32)> {
    let mut values = world.query_mut::<ActorValues>()?;
    let values = values.get_mut(target)?;
    let before = values.current(health_form_id);
    values.apply_damage(health_form_id, damage.max(0.0));
    Some((before, values.current(health_form_id)))
}

fn remove_component<T: Component>(world: &World, entity: EntityId) {
    if let Some(mut query) = world.query_mut::<T>() {
        query.remove(entity);
    }
}

fn disable_actor_ai(world: &World, actor: EntityId) {
    crate::npc_spawn::ai_package::clear_ambient_behavior(world, actor);
}

/// Rebuild the runtime consequences of the persisted [`Dead`] fact.
///
/// Live-load deltas are intentionally additive, so absence of AI and
/// animation components is not serialized as a second, generic tombstone
/// format. Both the combat transition and save-load drain call this one
/// reconciler, keeping those derived removals consistent (#3022).
fn reconcile_dead_actor(world: &World, actor: EntityId) -> String {
    disable_actor_ai(world, actor);
    let Some(skeleton_root) = world
        .get::<HavokAnimationTarget>(actor)
        .map(|target| target.skeleton_root)
    else {
        return "; no ragdoll target".to_owned();
    };
    remove_component::<AnimationPlayer>(world, skeleton_root);
    match crate::ragdoll::activate_ragdoll(world, skeleton_root) {
        Ok(body_count) => format!("; ragdoll activated ({body_count} bodies)"),
        Err(error) => format!("; ragdoll unavailable: {error}"),
    }
}

/// Reconcile every saved death marker after a freshly loaded cell has had its
/// mutable deltas applied. Returns the number of dead actors processed.
pub(crate) fn reconcile_dead_actor_runtime_state(world: &World) -> usize {
    let actors: Vec<EntityId> = world
        .query::<Dead>()
        .map(|dead| dead.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    for actor in actors.iter().copied() {
        reconcile_dead_actor(world, actor);
    }
    actors.len()
}

fn record_miss(world: &World, outcome: &str) {
    if let Some(mut state) = world.try_resource_mut::<CombatState>() {
        state.last = Some(CombatTraceEntry {
            target: None,
            damage: 0.0,
            health_before: None,
            health_after: None,
            killed: false,
            outcome: outcome.to_owned(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::InventoryIndex;
    use byroredux_core::ecs::components::{FollowBehavior, FollowState};

    fn damage_fixture(health: f32, weapon_damage: Option<f32>) -> (World, EntityId, EntityId) {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<ActorValues>();
        world.register::<ActorVitals>();
        world.register::<EquippedWeapon>();
        world.register::<Dead>();
        world.insert_resource(CombatState::default());

        let aggressor = world.spawn();
        if let Some(damage) = weapon_damage {
            world.insert(
                aggressor,
                EquippedWeapon {
                    inventory_index: InventoryIndex(0),
                    base_form_id: 0x1CB64,
                    damage,
                },
            );
        }
        let target = world.spawn();
        world.insert(target, ActorValues::from_pairs([(0x2D4, health)]));
        world.insert(target, ActorVitals { health: 0x2D4 });
        world.insert(
            target,
            byroredux_scripting::HitEvent {
                aggressor,
                source: aggressor,
                projectile: 0,
                power_attack: false,
                sneak_attack: false,
                bash_attack: false,
                blocked: false,
            },
        );
        (world, aggressor, target)
    }

    #[test]
    fn equipped_weapon_hit_applies_authored_damage() {
        let (world, _aggressor, target) = damage_fixture(50.0, Some(18.0));
        combat_damage_system(&world, 0.0);

        assert_eq!(
            world.get::<ActorValues>(target).unwrap().current(0x2D4),
            32.0
        );
        assert!(world.get::<Dead>(target).is_none());
        let state = world.resource::<CombatState>();
        assert_eq!(state.hits_landed, 1);
        assert_eq!(state.kills, 0);
    }

    #[test]
    fn lethal_hit_marks_dead_and_persists_trace() {
        let (world, _aggressor, target) = damage_fixture(10.0, Some(18.0));
        combat_damage_system(&world, 0.0);

        assert!(world.get::<Dead>(target).is_some());
        let state = world.resource::<CombatState>();
        assert_eq!(state.kills, 1);
        let last = state.last.as_ref().unwrap();
        assert_eq!(last.health_before, Some(10.0));
        assert_eq!(last.health_after, Some(-8.0));
        assert!(last.killed);
    }

    #[test]
    fn unarmed_hit_uses_explicit_fallback_damage() {
        let (world, _aggressor, target) = damage_fixture(20.0, None);
        combat_damage_system(&world, 0.0);
        assert_eq!(
            world.get::<ActorValues>(target).unwrap().current(0x2D4),
            12.0
        );
    }

    #[test]
    fn dead_state_reconciliation_removes_respawned_ai() {
        let mut world = World::new();
        world.register::<Dead>();
        world.register::<FollowBehavior>();
        world.register::<FollowState>();
        let actor = world.spawn();
        world.insert(actor, Dead);
        world.insert(
            actor,
            FollowBehavior {
                target_form_id: Some(0x14),
                follow_distance: Some(64.0),
            },
        );
        world.insert(
            actor,
            FollowState {
                target_entity: Some(0),
            },
        );

        assert_eq!(reconcile_dead_actor_runtime_state(&world), 1);
        assert!(world.get::<FollowBehavior>(actor).is_none());
        assert!(world.get::<FollowState>(actor).is_none());
        assert!(world.get::<Dead>(actor).is_some());
    }
}
