//! First playable melee-combat vertical slice.
//!
//! A physical Attack action casts from the active camera, resolves a hit
//! ragdoll bone through [`ActorColliderOwner`], and emits the canonical
//! scripting [`HitEvent`]. A same-frame consumer applies weapon/unarmed
//! damage to the target's Health actor value and owns the alive→dead
//! transition. Transient HitEvent cleanup remains in the scripting Late stage.

use byroredux_core::animation::AnimationPlayer;
use byroredux_core::character::{CharacterLevel, CharacterRuleset, MeleeDamageConfig};
use byroredux_core::ecs::components::{ActorValues, ActorVitals, Dead, EquippedWeapon};
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::{Resource, World};

use crate::components::HavokAnimationTarget;
use crate::interaction::{camera_ray, ActionState, InputAction};
use crate::systems::{PlayerEntity, PlayerMode};

/// Camera-forward melee reach in Bethesda units — the unarmed / no-weapon
/// baseline. `EquippedWeapon::reach` is an authored *multiplier* on this
/// baseline (1.0 = same reach as a longsword/mace; source: CS wiki's
/// `fCombatDistance * NPCScale * Reach` formula), not an absolute distance.
pub(crate) const MELEE_REACH_BU: f32 = 180.0;
/// One attack edge per cooldown. Holding Attack never auto-repeats because
/// ActionState contributes only the initial press edge. This is the
/// unarmed / no-weapon baseline; `EquippedWeapon::speed` is an authored
/// cadence multiplier (>1.0 = faster attacks, matching the CS wiki's
/// "numbers greater than one mean a fast weapon") that divides it.
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

    // Resolved once up front: both the cooldown arm below and the reach
    // used for the ray cast need the equipped weapon, and the cooldown has
    // to be armed before the mode/aggressor gating below returns early.
    let aggressor = world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0);

    let attack_ready = if let Some(mut state) = world.try_resource_mut::<CombatState>() {
        state.blocking = block_held;
        state.cooldown_remaining = (state.cooldown_remaining - dt.max(0.0)).max(0.0);
        if attack_pressed && state.cooldown_remaining <= 0.0 {
            state.cooldown_remaining =
                aggressor.map_or(MELEE_COOLDOWN_SECONDS, |aggressor| {
                    attack_cooldown_seconds(world, aggressor)
                });
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

    let Some(aggressor) = aggressor else {
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
    let reach = attack_reach_bu(world, aggressor);
    let hit = world
        .try_resource::<byroredux_physics::PhysicsWorld>()
        .and_then(|physics| physics.cast_ray(origin, direction, reach, excluded_body));
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
            // `block_held` is the *aggressor's* own Block state, snapshotted
            // above (#2976). Canonically `HitEvent::blocked` describes the
            // target's defense, but this slice has exactly one HitEvent
            // producer and it is always player-initiated — no hostile/NPC
            // attack path exists to give a target its own blocking signal
            // yet. Wiring the aggressor's own hold here at least makes the
            // field live and testable rather than a permanently-false
            // constant: swinging while holding Block now deals no damage
            // instead of costing the player nothing. `projectile`,
            // `power_attack`, `sneak_attack`, `bash_attack` stay `false` —
            // unlike Block, none of them has an input action driving them at
            // all (no power-attack charge, sneak-attack, or bash input
            // exists), so there is nothing to wire them to yet.
            blocked: block_held,
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
    match world.get::<EquippedWeapon>(aggressor) {
        // The capture document is explicit that Melee Damage is "an
        // additive bonus to Melee Weapon damage" and that "Unarmed has its
        // own stat" (Unarmed Damage, a different AVIF-governed formula) —
        // the two are gated on whether a weapon is actually equipped, not
        // both stacked onto every swing regardless. Wiring Unarmed Damage
        // itself is a separate, deferred gap (#3092's own suggested fix only
        // names Melee Damage); UNARMED_DAMAGE stays the flat engine baseline
        // it always was for the no-weapon case.
        Some(weapon) => weapon.damage.max(0.0) + melee_damage_charal_bonus(world, aggressor),
        None => UNARMED_DAMAGE,
    }
}

/// CHARAL-derived additive Melee Damage bonus on top of a weapon's own
/// authored damage (#3092) — FO3/FNV `STR × 0.5`, per
/// `docs/engine/charal-fnv-fo3-ruleset.md`'s "an **additive** bonus to Melee
/// Weapon damage" (matching how `resolve_inherited_stats`/similar CHARAL
/// consumers degrade: a missing piece means zero contribution, never a
/// panic). `0.0` — not an error — whenever any link in the chain is
/// unavailable: no [`MeleeDamageConfig`] (the loaded game authors no
/// `MeleeDamage` AVIF at all — FO4, TES), no live [`CharacterRuleset`], no
/// `ActorValues` on the aggressor (a non-actor swinging, or a test fixture
/// that doesn't need this), or the row simply doesn't resolve for this
/// actor. `DerivedOutput::Multiplier` (FO4's `×(1 + STR/10)`) still has no
/// reader — vanilla FO4 authors no AVIF to key a derived row on at all (see
/// `fallout4_ruleset`'s docstring, #3093), so there is nothing here to route
/// it through yet; that gap stays open and undocumented-as-solved rather
/// than papered over with an invented lookup.
fn melee_damage_charal_bonus(world: &World, aggressor: EntityId) -> f32 {
    let Some(config) = world.try_resource::<MeleeDamageConfig>() else {
        return 0.0;
    };
    let config = *config;
    let Some(ruleset) = world.try_resource::<CharacterRuleset>() else {
        return 0.0;
    };
    let Some(avs) = world.get::<ActorValues>(aggressor) else {
        return 0.0;
    };
    let level = world
        .get::<CharacterLevel>(aggressor)
        .map_or(1, |level| level.level);
    ruleset
        .derived_value(config.melee_damage_avif, &avs, level)
        .unwrap_or(0.0)
}

/// Weapon-scaled melee reach. `EquippedWeapon::reach` is a multiplier on
/// [`MELEE_REACH_BU`] (1.0 = same reach as a longsword/mace); `0.0` means
/// the source game's weapon layout isn't decoded yet (see
/// `ItemKind::Weapon::reach`), so it falls back to the flat baseline —
/// same rule the unarmed case already uses.
fn attack_reach_bu(world: &World, aggressor: EntityId) -> f32 {
    world
        .get::<EquippedWeapon>(aggressor)
        .filter(|weapon| weapon.reach > 0.0)
        .map_or(MELEE_REACH_BU, |weapon| MELEE_REACH_BU * weapon.reach)
}

/// Weapon-scaled swing cooldown. `EquippedWeapon::speed` is a cadence
/// multiplier (>1.0 = faster attacks) on [`MELEE_COOLDOWN_SECONDS`]; `0.0`
/// (unset/undecoded) falls back to the flat baseline.
fn attack_cooldown_seconds(world: &World, aggressor: EntityId) -> f32 {
    world
        .get::<EquippedWeapon>(aggressor)
        .filter(|weapon| weapon.speed > 0.0)
        .map_or(MELEE_COOLDOWN_SECONDS, |weapon| {
            MELEE_COOLDOWN_SECONDS / weapon.speed
        })
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

    fn damage_fixture(
        health: f32,
        weapon_damage: Option<f32>,
        blocked: bool,
    ) -> (World, EntityId, EntityId) {
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
                    reach: 0.0,
                    speed: 0.0,
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
                blocked,
            },
        );
        (world, aggressor, target)
    }

    #[test]
    fn equipped_weapon_hit_applies_authored_damage() {
        let (world, _aggressor, target) = damage_fixture(50.0, Some(18.0), false);
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
        let (world, _aggressor, target) = damage_fixture(10.0, Some(18.0), false);
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
        let (world, _aggressor, target) = damage_fixture(20.0, None, false);
        combat_damage_system(&world, 0.0);
        assert_eq!(
            world.get::<ActorValues>(target).unwrap().current(0x2D4),
            12.0
        );
    }

    /// Regression for #3092. `attack_damage` bypassed CHARAL entirely —
    /// Strength contributed nothing to melee damage on any game. Wire a
    /// minimal FO3/FNV-shaped ruleset (`MeleeDamage = STR × 0.5`) and confirm
    /// it lands as a bonus ON TOP of the flat weapon/unarmed baseline, per
    /// the capture document's "additive bonus" wording — not a replacement.
    #[test]
    fn attack_damage_adds_the_charal_melee_damage_bonus() {
        use byroredux_core::character::{
            CharacterRuleset, DerivedInput, DerivedStatFormula, LevelingModel,
        };

        const STRENGTH: u32 = 0x05;
        const MELEE_DAMAGE: u32 = 0x2D2;

        let mut world = World::new();
        world.register::<ActorValues>();
        world.register::<EquippedWeapon>();

        let mut rs = CharacterRuleset::new(LevelingModel::FNV);
        rs.push_derived(
            MELEE_DAMAGE,
            DerivedStatFormula::affine(DerivedInput::actor_value(STRENGTH), 0.5, 0.0),
        );
        world.insert_resource(rs);
        world.insert_resource(MeleeDamageConfig {
            melee_damage_avif: MELEE_DAMAGE,
        });

        let aggressor = world.spawn();
        world.insert(
            aggressor,
            EquippedWeapon {
                inventory_index: byroredux_core::ecs::components::InventoryIndex(0),
                base_form_id: 0x1CB64,
                damage: 18.0,
                reach: 0.0,
                speed: 0.0,
            },
        );
        world.insert(aggressor, ActorValues::from_pairs([(STRENGTH, 10.0)]));

        // 18.0 weapon damage + (10.0 STR × 0.5) = 23.0, not 18.0.
        assert_eq!(attack_damage(&world, aggressor), 23.0);
    }

    /// Companion to the above: no `MeleeDamageConfig` resource at all (FO4,
    /// TES, or simply not yet loaded) must fall back to the exact pre-#3092
    /// flat baseline, not panic or silently zero the weapon's own damage.
    #[test]
    fn attack_damage_falls_back_to_flat_baseline_without_a_melee_damage_config() {
        let mut world = World::new();
        world.register::<ActorValues>();
        world.register::<EquippedWeapon>();
        let aggressor = world.spawn();
        world.insert(
            aggressor,
            EquippedWeapon {
                inventory_index: byroredux_core::ecs::components::InventoryIndex(0),
                base_form_id: 0x1CB64,
                damage: 18.0,
                reach: 0.0,
                speed: 0.0,
            },
        );
        assert_eq!(attack_damage(&world, aggressor), 18.0);

        let unarmed = world.spawn();
        assert_eq!(attack_damage(&world, unarmed), UNARMED_DAMAGE);
    }

    /// Regression for #3092's gating. The capture document is explicit that
    /// Melee Damage is a bonus to *Melee Weapon* damage and that "Unarmed
    /// has its own stat" — a fully-wired CHARAL bonus must still NOT apply
    /// to a swing with no equipped weapon, even when the aggressor's own
    /// Strength would make it nonzero if it did.
    #[test]
    fn attack_damage_does_not_apply_the_melee_bonus_to_unarmed_swings() {
        use byroredux_core::character::{
            CharacterRuleset, DerivedInput, DerivedStatFormula, LevelingModel,
        };

        const STRENGTH: u32 = 0x05;
        const MELEE_DAMAGE: u32 = 0x2D2;

        let mut world = World::new();
        world.register::<ActorValues>();
        world.register::<EquippedWeapon>();

        let mut rs = CharacterRuleset::new(LevelingModel::FNV);
        rs.push_derived(
            MELEE_DAMAGE,
            DerivedStatFormula::affine(DerivedInput::actor_value(STRENGTH), 0.5, 0.0),
        );
        world.insert_resource(rs);
        world.insert_resource(MeleeDamageConfig {
            melee_damage_avif: MELEE_DAMAGE,
        });

        let unarmed = world.spawn();
        world.insert(unarmed, ActorValues::from_pairs([(STRENGTH, 10.0)]));

        assert_eq!(
            attack_damage(&world, unarmed),
            UNARMED_DAMAGE,
            "no equipped weapon means no Melee Damage bonus, regardless of Strength"
        );
    }

    /// Regression for #2976. `HitEvent::blocked` was hardcoded `false` at the
    /// sole producer, so this arm of `combat_damage_system` was unreachable
    /// from any live path. A blocked hit must apply zero damage while still
    /// landing (counted, traced, no death check skipped).
    #[test]
    fn blocked_hit_applies_zero_damage_but_still_lands() {
        let (world, _aggressor, target) = damage_fixture(50.0, Some(18.0), true);
        combat_damage_system(&world, 0.0);

        assert_eq!(
            world.get::<ActorValues>(target).unwrap().current(0x2D4),
            50.0,
            "a blocked hit must not reduce Health"
        );
        assert!(world.get::<Dead>(target).is_none());
        let state = world.resource::<CombatState>();
        assert_eq!(state.hits_landed, 1, "a blocked hit still counts as a hit");
        assert_eq!(state.kills, 0);
        let last = state.last.as_ref().unwrap();
        assert_eq!(last.damage, 0.0);
        assert_eq!(last.health_before, Some(50.0));
        assert_eq!(last.health_after, Some(50.0));
    }

    #[test]
    fn unarmed_or_undecoded_weapon_falls_back_to_flat_reach_and_cooldown() {
        let mut world = World::new();
        world.register::<EquippedWeapon>();
        let unarmed = world.spawn();
        assert_eq!(attack_reach_bu(&world, unarmed), MELEE_REACH_BU);
        assert_eq!(attack_cooldown_seconds(&world, unarmed), MELEE_COOLDOWN_SECONDS);

        // A weapon whose game's DNAM layout isn't decoded yet (reach/speed
        // still 0.0 — e.g. Skyrim) gets the same unarmed-style fallback,
        // not a zero-length / zero-cooldown swing.
        let undecoded = world.spawn();
        world.insert(
            undecoded,
            EquippedWeapon {
                inventory_index: InventoryIndex(0),
                base_form_id: 0x1234,
                damage: 10.0,
                reach: 0.0,
                speed: 0.0,
            },
        );
        assert_eq!(attack_reach_bu(&world, undecoded), MELEE_REACH_BU);
        assert_eq!(attack_cooldown_seconds(&world, undecoded), MELEE_COOLDOWN_SECONDS);
    }

    #[test]
    fn authored_reach_and_speed_scale_the_flat_baseline() {
        let mut world = World::new();
        world.register::<EquippedWeapon>();

        // A dagger: short reach (0.7x), fast cadence (1.5x) — shorter
        // cooldown, shorter effective range than a longsword.
        let dagger = world.spawn();
        world.insert(
            dagger,
            EquippedWeapon {
                inventory_index: InventoryIndex(0),
                base_form_id: 0xAAAA,
                damage: 6.0,
                reach: 0.7,
                speed: 1.5,
            },
        );
        // A warhammer: long reach (1.4x), slow cadence (0.5x).
        let warhammer = world.spawn();
        world.insert(
            warhammer,
            EquippedWeapon {
                inventory_index: InventoryIndex(0),
                base_form_id: 0xBBBB,
                damage: 40.0,
                reach: 1.4,
                speed: 0.5,
            },
        );

        let dagger_reach = attack_reach_bu(&world, dagger);
        let hammer_reach = attack_reach_bu(&world, warhammer);
        assert!(
            dagger_reach < hammer_reach,
            "dagger ({dagger_reach}) should reach less than a warhammer ({hammer_reach})"
        );
        assert!((dagger_reach - MELEE_REACH_BU * 0.7).abs() < 1e-6);
        assert!((hammer_reach - MELEE_REACH_BU * 1.4).abs() < 1e-6);

        let dagger_cooldown = attack_cooldown_seconds(&world, dagger);
        let hammer_cooldown = attack_cooldown_seconds(&world, warhammer);
        assert!(
            dagger_cooldown < hammer_cooldown,
            "dagger ({dagger_cooldown}) should swing faster than a warhammer ({hammer_cooldown})"
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
