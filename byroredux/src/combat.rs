//! First playable melee-combat vertical slice.
//!
//! A physical Attack action casts from the active camera, resolves a hit
//! ragdoll bone through [`ActorColliderOwner`], and emits the canonical
//! scripting [`HitEvent`]. A same-frame consumer applies weapon/unarmed
//! damage to the target's Health actor value and owns the alive→dead
//! transition. Transient HitEvent cleanup remains in the scripting Late stage.

use byroredux_core::animation::AnimationPlayer;
use byroredux_core::character::{CharacterLevel, CharacterRuleset, MeleeDamageConfig};
use byroredux_core::ecs::components::{
    ActorValues, ActorVitals, CreatureAttack, Dead, EquippedWeapon,
};
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::{Resource, SparseSetStorage, World};

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

/// Session-wide melee telemetry: counters and the last resolved
/// attack/hit/death, for `combat.status` and smoke-test evidence.
///
/// #3709 (ECS-P2-06) — `cooldown_remaining`/`blocking` used to live here
/// too, but those are per-combatant facts, not session-global ones; a
/// single `Resource` field can only ever represent one combatant's
/// cooldown/block state at a time. There is exactly one melee producer
/// today (`combat_input_system` resolves its aggressor from
/// `PlayerEntity`), so this was latent rather than a live bug — but the
/// first NPC attacker would have made the two combatants share one
/// cooldown clock. Split out to [`MeleeState`], a per-entity component.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CombatState {
    pub(crate) attacks_started: u64,
    pub(crate) hits_landed: u64,
    pub(crate) kills: u64,
    pub(crate) last: Option<CombatTraceEntry>,
}

impl Default for CombatState {
    fn default() -> Self {
        Self {
            attacks_started: 0,
            hits_landed: 0,
            kills: 0,
            last: None,
        }
    }
}

impl Resource for CombatState {}

/// Per-combatant melee timing: this entity's own attack-cooldown clock and
/// block-held flag. #3709 (ECS-P2-06) — split out of [`CombatState`],
/// which could only ever represent one combatant since it was a
/// `Resource`. `SparseSetStorage` matches the sibling per-actor behavior
/// components in `crate::components` (e.g. `HavokAnimationTarget`) — most
/// entities never fight, so a dense/packed storage would waste space.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct MeleeState {
    pub(crate) cooldown_remaining: f32,
    pub(crate) blocking: bool,
}

impl Component for MeleeState {
    type Storage = SparseSetStorage<Self>;
}

/// Death transitions produced inside parallel systems are queued here and
/// reconciled by one Late-stage exclusive sink. The persisted [`Dead`] marker
/// is the fact; this queue only defers the structural AI/animation/ragdoll
/// consequences until it is safe to mutate their component storages.
#[derive(Debug, Default)]
pub(crate) struct PendingDeathReconciliations {
    actors: Vec<EntityId>,
}

impl Resource for PendingDeathReconciliations {}

pub(crate) fn queue_dead_actor_reconciliation(world: &World, actor: EntityId) {
    let Some(mut pending) = world.try_resource_mut::<PendingDeathReconciliations>() else {
        log::error!("PendingDeathReconciliations missing; actor {actor} remains unreconciled");
        return;
    };
    if !pending.actors.contains(&actor) {
        pending.actors.push(actor);
    }
}

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
    // used for the ray cast need the equipped weapon.
    let aggressor = world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0);

    // #3033 — the eligibility gate must be read BEFORE the `CombatState`
    // mutation, not after it. Pre-fix the attack edge was consumed, the
    // cooldown armed and `attacks_started` incremented, and only *then* was
    // the mode checked — so a fly-cam session inflated the swing counter and
    // reported a cooldown the player never incurred, corrupting the very
    // telemetry `combat.status` (the P2 gate's console surface) reads.
    let in_character_mode = world
        .try_resource::<PlayerMode>()
        .is_some_and(|mode| *mode == PlayerMode::Character);

    // #3697 (ECS-P2-01) — resolved before `CombatState`'s write guard opens,
    // not from inside it. `attack_cooldown_seconds` reads `EquippedWeapon`,
    // and calling it while `try_resource_mut::<CombatState>()` is still held
    // records a `CombatState(write) -> EquippedWeapon(read)` edge in the
    // global lock-order graph — exactly the "snapshot into an owned local
    // and drop your guards before calling a helper that locks" pattern
    // `crates/core/src/ecs/world.rs`'s house rule forbids. Cheap either way
    // (one component lookup), so precomputing it unconditionally rather
    // than only on the frames that arm a cooldown is the simpler flattening.
    let armed_cooldown = aggressor
        .map_or(MELEE_COOLDOWN_SECONDS, |aggressor| {
            attack_cooldown_seconds(world, aggressor)
        });

    // #3709 (ECS-P2-06) — cooldown/blocking are per-combatant facts, tracked
    // on `MeleeState` (the aggressor entity's own component), not on the
    // session-global `CombatState` resource; without an aggressor entity
    // there is nowhere to attach that state, and no attack could resolve
    // past the `let Some(aggressor) = aggressor else { record_miss(...) }`
    // bail below regardless.
    let attack_ready = match aggressor {
        Some(aggressor) => world
            .query_mut::<MeleeState>()
            .is_some_and(|mut melee| {
                if melee.get_mut(aggressor).is_none() {
                    melee.insert(aggressor, MeleeState::default());
                }
                let state = melee
                    .get_mut(aggressor)
                    .expect("just inserted if it was missing");
                // Continuous state, not an edge: the cooldown clock and the
                // block flag keep tracking in every mode, so entering and
                // leaving fly-cam neither freezes a running cooldown nor
                // strands `blocking` true.
                state.blocking = block_held;
                state.cooldown_remaining = (state.cooldown_remaining - dt.max(0.0)).max(0.0);
                if attack_pressed && in_character_mode && state.cooldown_remaining <= 0.0 {
                    state.cooldown_remaining = armed_cooldown;
                    true
                } else {
                    false
                }
            }),
        None => false,
    };
    if attack_ready {
        if let Some(mut state) = world.try_resource_mut::<CombatState>() {
            state.attacks_started = state.attacks_started.saturating_add(1);
        }
    }
    if !attack_ready {
        // Deliberately NOT `record_miss`: every miss reason below describes a
        // swing that happened and failed to connect. A press outside
        // character mode (or during cooldown) is not a swing at all, so
        // `CombatState.last` keeps the previous real attempt rather than
        // being overwritten by a non-event. #3033.
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
            // instances acquire stable entities; the damage that weapon was
            // worth rides on the event itself (`damage` below) rather than
            // being re-derived from `source`.
            source: aggressor,
            projectile: 0,
            // #2980 — resolved once, here. `combat_damage_system` used to
            // call `attack_damage(world, event.aggressor)` a second time and
            // this producer's value was discarded; the comment above claimed
            // the opposite. Same-frame equality made that harmless but
            // undetectable, and a scripted producer has no `EquippedWeapon`
            // to recompute from.
            damage,
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
        // `blocked` is the target's defense, so it is resolved here rather
        // than folded into the producer's `damage` (#2980).
        let damage = if event.blocked {
            0.0
        } else {
            event.damage.max(0.0)
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
    // #3762 — a creature's authored `CREA.DATA.Damage`, stamped at spawn as
    // `CreatureAttack`. Checked before the weapon arm only in the sense of
    // the fallback it replaces: a creature carrying an equipped weapon
    // (rare, but authored — brahmin-drovers' pack items aside, a few FNV
    // creatures do equip) still resolves through the weapon arm, because an
    // equipped weapon is the more specific statement about what the actor
    // is swinging. What this arm replaces is the flat `UNARMED_DAMAGE`
    // fallback, which was making every FO3/FNV creature hit for 8.
    //
    // The CHARAL `MeleeDamage` bonus is deliberately NOT added here: the
    // capture document scopes `STR × 0.5` to *Melee Weapon* damage, and a
    // creature's `DATA.Damage` is already its whole authored attack, not a
    // weapon's base to be modified.
    //
    // #3473 — the weapon's damage is snapshotted here, once, so the
    // `EquippedWeapon` read guard dies at the end of this statement.
    // Matching on the `ComponentRef` directly bound it for the whole arm,
    // and the arm calls `melee_damage_charal_bonus`, which nests
    // `MeleeDamageConfig` -> `CharacterRuleset` -> `ActorValues` ->
    // `CharacterLevel` beneath it: a five-deep hold stack established
    // across a helper call, which is exactly what `world.rs`'s "snapshot
    // before you iterate" house rule (#2270) exists to prohibit. It also
    // collapses what used to be two separate `EquippedWeapon` lookups.
    let weapon_damage = world
        .get::<EquippedWeapon>(aggressor)
        .map(|weapon| weapon.damage);
    if weapon_damage.is_none() {
        if let Some(attack) = world.get::<CreatureAttack>(aggressor) {
            if attack.damage.is_finite() && attack.damage > 0.0 {
                return attack.damage;
            }
        }
    }
    match weapon_damage {
        // The capture document is explicit that Melee Damage is "an
        // additive bonus to Melee Weapon damage" and that "Unarmed has its
        // own stat" (Unarmed Damage, a different AVIF-governed formula) —
        // the two are gated on whether a weapon is actually equipped, not
        // both stacked onto every swing regardless. Wiring Unarmed Damage
        // itself is a separate, deferred gap (#3092's own suggested fix only
        // names Melee Damage); UNARMED_DAMAGE stays the flat engine baseline
        // it always was for the no-weapon case.
        Some(damage) => damage.max(0.0) + melee_damage_charal_bonus(world, aggressor),
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
    // #3473 — scoped, not shadowed. `let config = *config;` copied the value
    // but left the original binding (and therefore the `MeleeDamageConfig`
    // read guard) live to the end of the function, so the guard sat under
    // every acquisition below it. The block ends the borrow at the copy.
    let Some(melee_damage_avif) = world
        .try_resource::<MeleeDamageConfig>()
        .map(|config| config.melee_damage_avif)
    else {
        return 0.0;
    };
    // `CharacterRuleset` -> `ActorValues` is the direction settled by #3441
    // (see `docs/engine/ecs.md`'s canonical order): the resource is the
    // outer lock, and `condition.rs`'s `GetActorValue` arm — the only site
    // that reads the component first — clones and drops before touching the
    // ruleset. Do not invert these two.
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
        .derived_value(melee_damage_avif, &avs, level)
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
pub(crate) fn reconcile_dead_actor(world: &World, actor: EntityId) -> String {
    disable_actor_ai(world, actor);
    // #3708 (ECS-P2-03) — `disable_actor_ai` -> `clear_ambient_behavior`
    // deliberately does NOT remove `AmbientPackageRuntime` (it's shared
    // with the live schedule-handover path, where that runtime state must
    // survive), so a corpse kept it forever: its
    // `last_evaluated_game_minute` marker froze at the actor's final live
    // evaluation, permanently satisfying `ambient_ai_package_system`'s
    // pass-2 "due" gate on every subsequent frame whose game-minute
    // differs, and pass 3 paid a real `package_candidates` clone (plus a
    // `Dead` lookup) for it every time before the pass-3 loop's `Dead`
    // skip ever ran. A corpse has no package to select — remove the
    // runtime here, in the death-only path, not in the shared handover
    // function. `EvaluatePackageRequest` is comparatively low-priority
    // (it's a one-shot marker `scripting::package::scene_package_system`
    // drains every tick regardless), removed alongside for the same
    // reason: a corpse needs neither.
    remove_component::<crate::components::AmbientPackageRuntime>(world, actor);
    remove_component::<byroredux_scripting::EvaluatePackageRequest>(world, actor);
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

/// Drain parallel-safe death notifications after all producers have run.
pub(crate) fn reconcile_pending_dead_actors_system(world: &World, _dt: f32) {
    let actors = world
        .try_resource_mut::<PendingDeathReconciliations>()
        .map(|mut pending| std::mem::take(&mut pending.actors))
        .unwrap_or_default();
    for actor in actors {
        if world.get::<Dead>(actor).is_some() {
            reconcile_dead_actor(world, actor);
        }
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
    use crate::components::AmbientPackageRuntime;
    use byroredux_scripting::EvaluatePackageRequest;

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
        // Mirror the live producer: resolve damage once, at production
        // time, and put it on the event (#2980).
        world.insert(
            target,
            byroredux_scripting::HitEvent {
                aggressor,
                source: aggressor,
                projectile: 0,
                damage: attack_damage(&world, aggressor),
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

    /// Regression for #3762. A `CREA` actor with an authored
    /// `DATA.Damage` and no weapon must attack for that damage, not the
    /// flat `UNARMED_DAMAGE` baseline.
    ///
    /// #3390 gave creatures SPECIAL + Health, which made them melee
    /// participants; nothing read the one number defining their attack, so
    /// all 692 FNV / 186 FO3 weaponless damage-authoring creatures hit for
    /// 8 — a Deathclaw's authored 125 included, a 15.6x shortfall.
    #[test]
    fn creature_attack_damage_beats_the_unarmed_baseline() {
        let mut world = World::new();
        world.register::<ActorValues>();
        world.register::<EquippedWeapon>();
        world.register::<CreatureAttack>();

        let deathclaw = world.spawn();
        world.insert(deathclaw, CreatureAttack { damage: 125.0 });

        assert_eq!(attack_damage(&world, deathclaw), 125.0);
        assert_ne!(attack_damage(&world, deathclaw), UNARMED_DAMAGE);
    }

    /// An actor with no `CreatureAttack` keeps the pre-#3762 baseline
    /// exactly — the new arm fills a gap, it does not move the floor.
    #[test]
    fn actors_without_a_creature_attack_keep_the_unarmed_baseline() {
        let mut world = World::new();
        world.register::<ActorValues>();
        world.register::<EquippedWeapon>();
        world.register::<CreatureAttack>();

        let human = world.spawn();
        assert_eq!(attack_damage(&world, human), UNARMED_DAMAGE);
    }

    /// An equipped weapon is the more specific statement about what the
    /// actor is swinging, so it still wins — and still takes the CHARAL
    /// bonus, which a natural attack does not (a creature's authored damage
    /// is its whole attack, not a weapon base to modify).
    #[test]
    fn an_equipped_weapon_still_outranks_a_natural_attack() {
        let mut world = World::new();
        world.register::<ActorValues>();
        world.register::<EquippedWeapon>();
        world.register::<CreatureAttack>();

        let actor = world.spawn();
        world.insert(actor, CreatureAttack { damage: 125.0 });
        world.insert(
            actor,
            EquippedWeapon {
                inventory_index: byroredux_core::ecs::components::InventoryIndex(0),
                base_form_id: 0x1CB64,
                damage: 18.0,
                reach: 0.0,
                speed: 0.0,
            },
        );

        // No MeleeDamageConfig resource here, so the CHARAL bonus is 0.0.
        assert_eq!(attack_damage(&world, actor), 18.0);
    }

    /// A non-finite or non-positive authored damage must not reach the
    /// damage pipeline. The spawn stamp already drops those, so this pins
    /// the second gate rather than a reachable state — cheap, and it keeps
    /// a future non-ESM producer of this component honest.
    #[test]
    fn a_degenerate_creature_attack_falls_back_to_the_baseline() {
        for damage in [0.0, -5.0, f32::NAN, f32::INFINITY] {
            let mut world = World::new();
            world.register::<ActorValues>();
            world.register::<EquippedWeapon>();
            world.register::<CreatureAttack>();
            let actor = world.spawn();
            world.insert(actor, CreatureAttack { damage });
            assert_eq!(
                attack_damage(&world, actor),
                UNARMED_DAMAGE,
                "damage {damage} must not reach the pipeline"
            );
        }
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

    /// Regression for #2980. The producer resolved `damage` and the consumer
    /// threw it away, calling `attack_damage` a second time — two derivations
    /// of one number, indistinguishable while both ran same-frame against an
    /// unchanged `EquippedWeapon`. Pin the event as the single source: an
    /// aggressor with no `EquippedWeapon` at all (what a scripted producer
    /// looks like) must still land the damage the event carries, not the
    /// `UNARMED_DAMAGE` baseline a recompute would return.
    #[test]
    fn consumer_applies_the_producers_damage_rather_than_recomputing_it() {
        let (world, aggressor, target) = damage_fixture(100.0, None, false);
        assert_eq!(
            attack_damage(&world, aggressor),
            UNARMED_DAMAGE,
            "fixture precondition: a recompute would return the unarmed baseline"
        );
        if let Some(mut events) = world.query_mut::<byroredux_scripting::HitEvent>() {
            events.get_mut(target).unwrap().damage = 25.0;
        }

        combat_damage_system(&world, 0.0);

        assert_eq!(
            world.get::<ActorValues>(target).unwrap().current(0x2D4),
            75.0,
            "the consumer must apply HitEvent::damage, not re-derive it"
        );
        assert_eq!(
            world
                .resource::<CombatState>()
                .last
                .as_ref()
                .unwrap()
                .damage,
            25.0
        );
    }

    /// Companion: the live producer is what fills that field, so the
    /// end-to-end value must still be the weapon's own authored damage plus
    /// any CHARAL bonus — `attack_damage` moved, it did not disappear.
    #[test]
    fn producer_snapshots_attack_damage_onto_the_event() {
        let (world, aggressor, target) = damage_fixture(100.0, Some(18.0), false);
        let event = *world.get::<byroredux_scripting::HitEvent>(target).unwrap();
        assert_eq!(event.damage, attack_damage(&world, aggressor));
        assert_eq!(event.damage, 18.0);
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
        assert_eq!(
            attack_cooldown_seconds(&world, unarmed),
            MELEE_COOLDOWN_SECONDS
        );

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
        assert_eq!(
            attack_cooldown_seconds(&world, undecoded),
            MELEE_COOLDOWN_SECONDS
        );
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
        world.register::<AmbientPackageRuntime>();
        world.register::<EvaluatePackageRequest>();
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
        // #3708 — the corpse's ambient-package runtime state. Pre-fix this
        // survived death forever, keeping the actor in
        // `ambient_ai_package_system`'s pass-1 query and paying a real
        // `package_candidates` clone every frame its frozen minute marker
        // disagreed with the current one.
        world.insert(
            actor,
            AmbientPackageRuntime {
                package_candidates: vec![0x1000, 0x2000],
                active_package_form_id: Some(0x1000),
                actor_form_id: 0x14,
                last_evaluated_game_minute: Some(42),
            },
        );
        world.insert(actor, EvaluatePackageRequest);

        assert_eq!(reconcile_dead_actor_runtime_state(&world), 1);
        assert!(world.get::<FollowBehavior>(actor).is_none());
        assert!(world.get::<FollowState>(actor).is_none());
        assert!(
            world.get::<AmbientPackageRuntime>(actor).is_none(),
            "a corpse must not retain its ambient-package runtime — it has \
             no package to select, and keeping it forever costs a \
             package_candidates clone every frame the minute marker \
             disagrees with the current one (#3708)"
        );
        assert!(
            world.get::<EvaluatePackageRequest>(actor).is_none(),
            "a corpse must not retain a pending package re-evaluation request"
        );
        assert!(world.get::<Dead>(actor).is_some());
    }

    // ── attack edge is gated on PlayerMode::Character (#3033) ───────────

    /// Drive the real input pipeline so the attack edge is produced the
    /// way the engine produces it, then run one `combat_input_system` tick.
    /// #3709 — spawns a real aggressor entity and installs it as
    /// `PlayerEntity`, rather than the pre-split fixture's `PlayerEntity
    /// (None)`. `MeleeState` (cooldown/blocking) now lives on that entity,
    /// so a fixture with no aggressor could no longer exercise arming at
    /// all — there is nowhere to attach the per-combatant state. Returns
    /// the aggressor's `EntityId` so callers can read its `MeleeState`.
    fn attack_edge_fixture(mode: PlayerMode) -> (World, EntityId) {
        use crate::components::InputState;
        use crate::interaction::{ActionBindings, InjectedKeyHold, InjectedKeyPulse};

        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<EquippedWeapon>();
        world.register::<Dead>();
        world.register::<MeleeState>();
        world.insert_resource(CombatState::default());
        world.insert_resource(InputState::default());
        world.insert_resource(ActionBindings::default());
        world.insert_resource(ActionState::default());
        world.insert_resource(InjectedKeyPulse::default());
        world.insert_resource(InjectedKeyHold::default());
        world.insert_resource(mode);
        let aggressor = world.spawn();
        world.insert_resource(PlayerEntity(Some(aggressor)));
        (world, aggressor)
    }

    fn melee_state_of(world: &World, entity: EntityId) -> MeleeState {
        world
            .query::<MeleeState>()
            .and_then(|q| q.get(entity).copied())
            .unwrap_or_default()
    }

    fn press_attack(world: &World) {
        use crate::components::InputState;
        use winit::keyboard::KeyCode;

        world
            .resource_mut::<InputState>()
            .keys_held
            .insert(KeyCode::KeyR);
        crate::interaction::refresh_action_state(world);
        assert!(world
            .resource::<ActionState>()
            .was_pressed(InputAction::Attack));
    }

    #[test]
    fn fly_cam_attack_press_does_not_burn_the_edge_or_arm_the_cooldown() {
        // #3033 — pre-fix the `CombatState` mutation ran *before* the
        // `PlayerMode::Character` gate, so a fly-cam press inflated
        // `attacks_started` and armed a cooldown the player never incurred,
        // corrupting the telemetry `combat.status` reports.
        let (world, aggressor) = attack_edge_fixture(PlayerMode::FlyCam);
        press_attack(&world);

        combat_input_system(&world, 1.0 / 60.0);

        let state = world.resource::<CombatState>();
        assert_eq!(
            state.attacks_started, 0,
            "a fly-cam press is not a swing and must not move the counter"
        );
        assert_eq!(
            melee_state_of(&world, aggressor).cooldown_remaining,
            0.0,
            "a fly-cam press must not arm the melee cooldown"
        );
        assert!(
            state.last.is_none(),
            "the mode bail deliberately leaves `last` untouched — it is not a miss"
        );
    }

    #[test]
    fn character_mode_attack_press_consumes_the_edge_and_arms_the_cooldown() {
        // Companion to the above: the gate must not have broken the real
        // path. In character mode the same press still counts and arms.
        let (world, aggressor) = attack_edge_fixture(PlayerMode::Character);
        press_attack(&world);

        combat_input_system(&world, 1.0 / 60.0);

        let state = world.resource::<CombatState>();
        assert_eq!(state.attacks_started, 1);
        assert_eq!(
            melee_state_of(&world, aggressor).cooldown_remaining,
            MELEE_COOLDOWN_SECONDS
        );
    }

    #[test]
    fn cooldown_keeps_ticking_down_in_fly_cam() {
        // The decay and the block flag are continuous state, not an edge:
        // entering fly-cam mid-cooldown must not freeze the clock, or the
        // player returns to character mode still locked out.
        let (world, aggressor) = attack_edge_fixture(PlayerMode::Character);
        press_attack(&world);
        combat_input_system(&world, 0.0);
        assert_eq!(
            melee_state_of(&world, aggressor).cooldown_remaining,
            MELEE_COOLDOWN_SECONDS
        );

        *world.resource_mut::<PlayerMode>() = PlayerMode::FlyCam;
        combat_input_system(&world, 0.2);
        let remaining = melee_state_of(&world, aggressor).cooldown_remaining;
        assert!(
            (remaining - (MELEE_COOLDOWN_SECONDS - 0.2)).abs() < 1e-5,
            "cooldown must keep decaying in fly-cam, got {remaining}"
        );
    }

    /// #3709 (ECS-P2-06) — the actual regression: `MeleeState` is a
    /// per-entity `SparseSet` component, not a shared `Resource` field, so
    /// two combatants' cooldown/block state cannot collide. There is no
    /// NPC melee producer yet (`combat_input_system` only ever arms its
    /// `PlayerEntity` aggressor), so this drives the real system for the
    /// player and confirms a second, independently-seeded combatant's
    /// `MeleeState` is completely untouched by it — the structural
    /// guarantee the pre-split resource could never make, since a single
    /// `cooldown_remaining` field can only ever represent one combatant.
    #[test]
    fn two_combatants_have_independent_cooldowns() {
        let (mut world, player) = attack_edge_fixture(PlayerMode::Character);
        let other = world.spawn();
        world.insert(
            other,
            MeleeState {
                cooldown_remaining: 0.2,
                blocking: true,
            },
        );
        press_attack(&world);

        combat_input_system(&world, 1.0 / 60.0);

        assert_eq!(
            melee_state_of(&world, player).cooldown_remaining,
            MELEE_COOLDOWN_SECONDS,
            "the player's cooldown must arm"
        );
        let other_state = melee_state_of(&world, other);
        assert_eq!(
            other_state.cooldown_remaining, 0.2,
            "a second combatant's cooldown must be untouched by the player's swing"
        );
        assert!(
            other_state.blocking,
            "a second combatant's block flag must be untouched by the player's swing"
        );
    }

    /// #3697 (ECS-P2-01) — `combat_input_system`'s cooldown-arming branch
    /// must resolve `attack_cooldown_seconds` (which reads `EquippedWeapon`)
    /// *before* opening `CombatState`'s write guard, not from a nested call
    /// while the guard is still live — otherwise it records a
    /// `CombatState(write) -> EquippedWeapon(read)` edge in the global
    /// lock-order graph.
    ///
    /// Establishes the canonical reverse edge (`EquippedWeapon` read, then
    /// `CombatState` write — the order every OTHER `CombatState` writer in
    /// this file already uses, since none of them read a component while
    /// holding the guard) before driving the real system. Pre-fix, this
    /// would have closed the cycle and panicked here; post-fix the weapon
    /// read and the `CombatState` write never nest.
    #[test]
    fn combat_input_system_does_not_close_combat_state_equipped_weapon_lock_cycle() {
        if std::env::var_os("BYRO_LOCK_ORDER_CHECK").as_deref() != Some(std::ffi::OsStr::new("1"))
        {
            return;
        }

        let (mut world, aggressor) = attack_edge_fixture(PlayerMode::Character);
        world.insert(
            aggressor,
            EquippedWeapon {
                inventory_index: InventoryIndex(0),
                base_form_id: 0x1CB64,
                damage: 10.0,
                reach: 0.0,
                speed: 1.5,
            },
        );
        press_attack(&world);

        // EquippedWeapon(read) -> CombatState(write): the canonical order.
        {
            let _weapon = world.query::<EquippedWeapon>().unwrap();
            let _state = world.resource_mut::<CombatState>();
        }

        combat_input_system(&world, 1.0 / 60.0);
    }
}
