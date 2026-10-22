//! First playable melee-combat vertical slice.
//!
//! A physical Attack action casts from the active camera, resolves a hit
//! ragdoll bone through [`ActorColliderOwner`], and emits the canonical
//! scripting [`HitEvent`]. A same-frame consumer applies weapon/unarmed
//! damage to the target's Health actor value and owns the alive→dead
//! transition. Transient HitEvent cleanup remains in the scripting Late stage.

use byroredux_core::animation::AnimationPlayer;
use byroredux_core::character::{
    CharacterLevel, CharacterRuleset, DerivedOutput, DerivedScope, MeleeDamageConfig,
};
use byroredux_core::ecs::components::{
    ActorValues, ActorVitals, CreatureAttack, Dead, EquippedWeapon,
};
use byroredux_core::ecs::storage::{Component, EntityId};
use std::collections::HashMap;
use byroredux_core::ecs::{Resource, SparseSetStorage, World};

use crate::components::AnimationTarget;
use crate::interaction::{camera_ray, ActionState, InputAction};
use crate::systems::{PlayerEntity, PlayerMode};

/// Camera-forward melee reach in Bethesda units — the unarmed / no-weapon
/// baseline. `EquippedWeapon::reach` is an authored *multiplier* on this
/// baseline (1.0 = same reach as a longsword/mace; source: CS wiki's
/// `fCombatDistance * NPCScale * Reach` formula), not an absolute distance.
pub(crate) const MELEE_REACH_BU: f32 = 180.0;
/// #5160 — the swing's swept corridor radius (BU). A swing resolves through
/// a ball cast along the camera ray, not a zero-width line: authored actor
/// bone colliders are 16-18 BU boxes with real gaps, and a line aimed dead
/// centre on an actor can pass between two bones (the GSSettlerCM repro
/// where 1.6° of pitch decided hit vs miss). 12 BU ≈ an 18 cm blade sweep —
/// wide enough to bridge adjacent bone gaps, narrow enough that a genuinely
/// clear miss stays a miss.
pub(crate) const MELEE_SWING_CORRIDOR_RADIUS_BU: f32 = 12.0;
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
    /// The hit's damage before the target's defenses (Block, Damage
    /// Threshold) — what the aggressor's weapon and perks produced.
    pub(crate) incoming: f32,
    /// The damage applied.
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
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct CombatState {
    pub(crate) attacks_started: u64,
    pub(crate) hits_landed: u64,
    pub(crate) kills: u64,
    pub(crate) last: Option<CombatTraceEntry>,
}

impl Resource for CombatState {}

/// Per-combatant melee timing: this entity's own attack-cooldown clock and
/// block-held flag. #3709 (ECS-P2-06) — split out of [`CombatState`],
/// which could only ever represent one combatant since it was a
/// `Resource`. `SparseSetStorage` matches the sibling per-actor behavior
/// components in `crate::components` (e.g. `AnimationTarget`) — most
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

#[cfg(test)]
impl PendingDeathReconciliations {
    /// The actors awaiting the Late-stage sink, in queue order.
    pub(crate) fn queued(&self) -> &[EntityId] {
        &self.actors
    }
}

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
    let can_swing = world
        .try_resource::<PlayerMode>()
        .is_some_and(|mode| *mode == PlayerMode::Character)
        // #4701 — a dead player's attack edge is not a swing either.
        && crate::systems::player_can_act(world);

    // #3697 (ECS-P2-01) — resolved before `CombatState`'s write guard opens,
    // not from inside it. `attack_cooldown_seconds` reads `EquippedWeapon`,
    // and calling it while `try_resource_mut::<CombatState>()` is still held
    // records a `CombatState(write) -> EquippedWeapon(read)` edge in the
    // global lock-order graph — exactly the "snapshot into an owned local
    // and drop your guards before calling a helper that locks" pattern
    // `crates/core/src/ecs/world.rs`'s house rule forbids. Cheap either way
    // (one component lookup), so precomputing it unconditionally rather
    // than only on the frames that arm a cooldown is the simpler flattening.
    let armed_cooldown = aggressor.map_or(MELEE_COOLDOWN_SECONDS, |aggressor| {
        attack_cooldown_seconds(world, aggressor)
    });

    // #3709 (ECS-P2-06) — cooldown/blocking are per-combatant facts, tracked
    // on `MeleeState` (the aggressor entity's own component), not on the
    // session-global `CombatState` resource; without an aggressor entity
    // there is nowhere to attach that state, and no attack could resolve
    // past the `let Some(aggressor) = aggressor else { record_miss(...) }`
    // bail below regardless.
    let attack_ready = match aggressor {
        Some(aggressor) => world.query_mut::<MeleeState>().is_some_and(|mut melee| {
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
            if attack_pressed && can_swing && state.cooldown_remaining <= 0.0 {
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
        if attack_pressed {
            let cooldown = aggressor.and_then(|actor| {
                world
                    .query::<MeleeState>()
                    .and_then(|q| q.get(actor).map(|state| state.cooldown_remaining))
            });
            log::debug!(
                "combat.input: attack edge rejected player={aggressor:?} can_swing={can_swing} cooldown={cooldown:?} dt={dt}"
            );
        }
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
        .and_then(|physics| {
            physics.cast_ray_corridor(
                origin,
                direction,
                reach,
                MELEE_SWING_CORRIDOR_RADIUS_BU,
                excluded_body,
            )
        });
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

    let damage = attack_damage_against(world, aggressor, target);
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
            // target's defense, but no producer models a target's own guard
            // yet — neither this player path nor `npc_combat_ai_system`'s
            // NPC strikes (#4324). Wiring the aggressor's own hold here at least makes the
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
            incoming: damage,
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
        // than folded into the producer's `damage` (#2980); so is its
        // Damage Threshold (M47.2).
        // M47.2 — a ghost "cannot be harmed by a weapon or spell in any way"
        // (CS / GECK *SetGhost*): the hit lands for nothing.
        let damage = if event.blocked || byroredux_scripting::is_ghost(world, target) {
            0.0
        } else {
            apply_damage_threshold(world, event.aggressor, target, event.damage.max(0.0))
        };
        let Some((before, after)) = apply_health_damage(world, target, vitals.health, damage)
        else {
            continue;
        };
        // M47.2 — an essential actor (or a protected one, unless the player
        // struck) goes into bleedout instead of dying.
        let spared =
            after <= 0.0 && byroredux_scripting::spare_from_death(world, target, Some(event.aggressor));
        let killed = after <= 0.0 && !spared;
        let mut outcome = format!("health {:.1} -> {:.1}", before, after);
        if spared {
            outcome.push_str("; bleedout");
        }
        note_player_damage(world, event.aggressor, target, before - after.max(0.0));

        if killed {
            if let Some(mut dead) = world.query_mut::<Dead>() {
                dead.insert(target, Dead);
            }
            // #5366 / #5434 — KILL story event, raised on the slain actor
            // (R1 = victim, R2 = killer; corpus-verified — `WIKill06`
            // fills its `Victim` alias from R1 and `DA08KillFriendNode`'s
            // tutorial-documented killer conditions run on R2). The
            // Story Manager dispatcher consumes it same-frame or next
            // (Pattern B marker); on pre-Creation titles it is a no-op.
            crate::systems::story_events::raise_kill_story_event(
                world,
                target,
                Some(event.aggressor),
            );
            outcome.push_str(&reconcile_dead_actor(world, target));
            award_kill_experience(world, target, vitals.health);
        }

        if let Some(mut state) = world.try_resource_mut::<CombatState>() {
            state.hits_landed = state.hits_landed.saturating_add(1);
            if killed {
                state.kills = state.kills.saturating_add(1);
            }
            state.last = Some(CombatTraceEntry {
                target: Some(target),
                incoming: event.damage,
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

/// [`attack_damage`] composed with the aggressor's perk entry points
/// (`Calculate Weapon Damage` / `Mod Attack Damage`, M47.2) for a strike on
/// `target`. The target's defenses (Block, Damage Threshold) are resolved
/// where the hit lands, in [`combat_damage_system`] (#2980). Callers must
/// hold no storage guard: the composition evaluates perk conditions, which
/// lock arbitrary components.
pub(crate) fn attack_damage_against(world: &World, aggressor: EntityId, target: EntityId) -> f32 {
    let base = attack_damage(world, aggressor);
    byroredux_scripting::perk_entry_points::compose_outgoing_damage(world, aggressor, target, base)
        .max(0.0)
}

/// Fallout 3 / New Vegas kill experience (M47.2), from the GECK *Experience
/// Settings* page: a kill of an NPC or a creature rewards the player with the
/// `iXPRewardKillNPC*` / `iXPRewardKillOpponent*` of the difficulty band the
/// victim's level falls in (each band's upper level is `iXPLevelKillNPC*` /
/// `iXPLevelKillCreature*`; above the last, "very hard"), but only when the
/// player dealt at least `iXPDeathRewardHealthThreshold` percent of the
/// victim's health. Installed when the load order authors those settings.
///
/// Neither master authors the five `iXPRewardKillNPC*` rewards, so they take
/// the engine defaults [`NPC_KILL_REWARD_DEFAULTS`].
#[derive(Debug, Clone, Default)]
pub(crate) struct ExperienceRules {
    /// `(highest level, reward)` per band, easiest first.
    npc_bands: Vec<(f32, f32)>,
    creature_bands: Vec<(f32, f32)>,
    /// The share of a victim's health the player must have dealt.
    health_share: f32,
    /// Creature base records (`CREA`) — the other bases are NPCs.
    creature_bases: std::collections::HashSet<u32>,
}

impl Resource for ExperienceRules {}

impl ExperienceRules {
    fn reward(&self, level: f32, creature: bool) -> f32 {
        let bands = if creature {
            &self.creature_bands
        } else {
            &self.npc_bands
        };
        bands
            .iter()
            .find(|(highest, _)| level <= *highest)
            .or(bands.last())
            .map_or(0.0, |(_, reward)| *reward)
    }
}

/// The player's damage dealt to each actor still standing, for the kill
/// experience share. Session state, `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Default)]
pub(crate) struct PlayerDamageDealt(pub HashMap<EntityId, f32>);

impl Resource for PlayerDamageDealt {}

/// The engine defaults of `iXPRewardKillNPC{VeryEasy,Easy,Average,Hard,VeryHard}`
/// — 0 / 10 / 20 / 30 / 50 on the GECK *Experience Settings* page and in
/// `Fallout3.exe`'s static setting table (each value precedes a pointer to its
/// name). Read when a load order does not author the setting; FalloutNV.esm
/// and Fallout3.esm author none of the five.
const NPC_KILL_REWARD_DEFAULTS: [(&str, f32); 5] = [
    ("iXPRewardKillNPCVeryEasy", 0.0),
    ("iXPRewardKillNPCEasy", 10.0),
    ("iXPRewardKillNPCAverage", 20.0),
    ("iXPRewardKillNPCHard", 30.0),
    ("iXPRewardKillNPCVeryHard", 50.0),
];

/// Install (or remove) [`ExperienceRules`] for a load order.
pub(crate) fn install_experience_rules(
    world: &mut World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) -> bool {
    const DIFFICULTIES: [&str; 5] = ["VeryEasy", "Easy", "Average", "Hard", "VeryHard"];
    // The GECK page documents Fallout 3 / New Vegas. Skyrim.esm authors the
    // same `iXPLevelKill*` / `iXPRewardKillOpponent*` names but levels by
    // skill use and awards no kill experience — the profile's row (#5921).
    if !index.character_rules.awards_kill_experience() {
        world.remove_resource::<ExperienceRules>();
        return false;
    }
    let setting = |name: String| {
        index.game_setting_float(&name).or_else(|| {
            NPC_KILL_REWARD_DEFAULTS
                .iter()
                .find(|(default, _)| *default == name)
                .map(|(_, value)| *value)
        })
    };
    let bands = |reward: &str, level: &str| -> Option<Vec<(f32, f32)>> {
        DIFFICULTIES
            .iter()
            .map(|band| {
                Some((
                    setting(format!("iXPLevelKill{level}{band}"))?,
                    setting(format!("iXPRewardKill{reward}{band}"))?,
                ))
            })
            .collect()
    };
    let rules = (|| {
        Some(ExperienceRules {
            npc_bands: bands("NPC", "NPC")?,
            creature_bands: bands("Opponent", "Creature")?,
            health_share: setting("iXPDeathRewardHealthThreshold".into())? / 100.0,
            creature_bases: index.creatures.keys().copied().collect(),
        })
    })();
    match rules {
        Some(rules) => {
            world.insert_resource(rules);
            world.init_resource::<PlayerDamageDealt>();
            true
        }
        None => {
            world.remove_resource::<ExperienceRules>();
            false
        }
    }
}

/// Add the player's `dealt` damage on `target` to its kill-experience share.
fn note_player_damage(world: &World, aggressor: EntityId, target: EntityId, dealt: f32) {
    let is_player = world
        .try_resource::<crate::systems::PlayerEntity>()
        .is_some_and(|player| player.0 == Some(aggressor));
    if !is_player || dealt <= 0.0 || world.try_resource::<ExperienceRules>().is_none() {
        return;
    }
    if let Some(mut dealt_by_player) = world.try_resource_mut::<PlayerDamageDealt>() {
        *dealt_by_player.0.entry(target).or_default() += dealt;
    }
}

/// Award the player the kill experience for `victim` (see
/// [`ExperienceRules`]), composed by the player's `Adjust Experience Points`
/// perks (hook 9 — Swift Learner), and roll over any level it completes.
fn award_kill_experience(world: &World, victim: EntityId, health_avif: u32) {
    use byroredux_core::character::CharacterLevel;
    let Some(rules) = world
        .try_resource::<ExperienceRules>()
        .map(|rules| (*rules).clone())
    else {
        return;
    };
    let dealt = world
        .try_resource_mut::<PlayerDamageDealt>()
        .and_then(|mut dealt| dealt.0.remove(&victim))
        .unwrap_or(0.0);
    let health = world
        .get::<ActorValues>(victim)
        .and_then(|values| values.get(health_avif).map(|value| value.undamaged()))
        .unwrap_or(0.0);
    if health <= 0.0 || dealt < rules.health_share * health {
        return;
    }
    let Some(player) = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0)
    else {
        return;
    };
    let level = world
        .get::<CharacterLevel>(victim)
        .map_or(1.0, |level| f32::from(level.level));
    let creature = world
        .get::<byroredux_scripting::SceneAliasCandidate>(victim)
        .is_some_and(|candidate| rules.creature_bases.contains(&candidate.base_form_id));
    let reward = rules.reward(level, creature);
    if reward <= 0.0 {
        return;
    }
    let xp = byroredux_scripting::perk_entry_points::compose_entry_point(
        world,
        player,
        byroredux_scripting::perk_entry_points::fnv::ADJUST_EXPERIENCE_POINTS,
        reward,
        &[],
    )
    .max(0.0)
    .round() as u32;
    log::info!("Kill experience: +{xp} XP");
    grant_experience(world, player, xp);
}

/// Add `xp` to `player`'s progress, rolling levels by the ruleset's
/// `LevelingModel` up to its cap, and bank what each level grants (M47.2):
/// Whether the character-generation state was on at the last pass, so its end awards the levels the XP
/// earned meanwhile. `NOT_SAVED_BY_DESIGN`: derived from the saved ledger flag.
#[derive(Debug, Default)]
pub(crate) struct ChargenWatch(bool);

impl Resource for ChargenWatch {}

/// When `SetInChargen 0` ends the state, award the levels the accumulated XP is worth ("when the player
/// leaves this state, the appropriate number of levels will be immediately awarded" — GECK).
pub(crate) fn chargen_catch_up_system(world: &World, _dt: f32) {
    let now = byroredux_scripting::obscript_more::in_chargen(world);
    let was = match world.try_resource_mut::<ChargenWatch>() {
        Some(mut watch) => std::mem::replace(&mut watch.0, now),
        None => return,
    };
    if was && !now {
        if let Some(player) = world.try_resource::<PlayerEntity>().and_then(|player| player.0) {
            grant_experience(world, player, 0);
        }
    }
}

/// FO3 / FNV skill points (`base + int_mult·INT`, composed by the Adjust
/// Gained Skill Points hook 10 — `Educated`) and a perk on the cadence.
/// Callers hold no storage guard.
pub(crate) fn grant_experience(world: &World, player: EntityId, xp: u32) {
    use byroredux_core::character::{CharacterLevel, LevelUpPoints};
    // #5874 — progression reaches the player as notices, not only the log.
    if xp > 0 {
        crate::notifications::push(world, format!("+{xp} XP"));
    }
    let leveling = world
        .try_resource::<CharacterRuleset>()
        .map(|ruleset| ruleset.leveling);
    let mut reached = Vec::new();
    {
        let Some(mut levels) = world.query_mut::<CharacterLevel>() else {
            return;
        };
        let Some(progress) = levels.get_mut(player) else {
            return;
        };
        progress.xp = progress.xp.saturating_add(xp);
        // `SetInChargen 1`: XP accumulates and the levels wait for the state to end
        // ([`chargen_catch_up_system`] awards them then).
        if let Some(leveling) = leveling.filter(|_| !byroredux_scripting::obscript_more::in_chargen(world)) {
            // #5918 — the bounded roll lives on the model.
            reached = leveling.roll_levels(progress);
        }
    }
    for level in &reached {
        log::info!("Player reached level {level}");
        crate::notifications::push(world, format!("Level up! You reached level {level}"));
    }
    let Some(leveling) = leveling else {
        return;
    };
    if reached.is_empty() {
        return;
    }
    // #5917 — Skill Rate reads only the *base* INT at level-up: temporary INT
    // (apparel, chems) and damage do not change it (`charal-fnv-fo3-ruleset.md`
    // :78). `Base` is the AV's own base plus its set-override layer; the
    // permanent-modifier layer is left out because it also carries equipped
    // gear and cannot be told apart from implant-style bonuses.
    let intelligence = player_intelligence(world, player, byroredux_scripting::condition::ActorValueView::Base);
    let mut granted = (0.0f32, 0u32);
    let mut pool_picks = 0u32;
    for level in reached {
        let Some((points, perk)) = leveling.skill_point_reward(level, intelligence) else {
            // Skyrim's `SkillXp` model has no skill points (#5916): each level
            // grants a perk and one Health / Magicka / Stamina pick instead.
            // Dropping them here lost them for good — the level is saved, so
            // a later spend UI could not recover them.
            granted.1 += u32::from(leveling.grants_perk_at(level));
            pool_picks += u32::from(leveling.pool_pick_gain().is_some());
            continue;
        };
        // Hook 10 is Adjust Gained Skill Points on FO3 / FNV only; FO4's
        // level grants no skill points to adjust.
        if points > 0.0 {
            granted.0 += byroredux_scripting::perk_entry_points::compose_entry_point(
                world,
                player,
                byroredux_scripting::perk_entry_points::fnv::ADJUST_GAINED_SKILL_POINTS,
                points,
                &[],
            )
            .max(0.0);
        }
        granted.1 += u32::from(perk);
    }
    let Some(mut rewards) = world.query_mut::<LevelUpPoints>() else {
        return;
    };
    if rewards.get(player).is_none() {
        rewards.insert(player, LevelUpPoints::default());
    }
    let rewards = rewards.get_mut(player).expect("present or inserted");
    rewards.grant_skill_points(granted.0);
    rewards.perk_points = rewards.perk_points.saturating_add(granted.1);
    rewards.pool_picks = rewards.pool_picks.saturating_add(pool_picks);
    log::info!(
        "Level-up rewards: +{:.1} skill points, +{} perks, +{} pool picks (unspent {} / {} / {})",
        granted.0,
        granted.1,
        pool_picks,
        rewards.skill_points,
        rewards.perk_points,
        rewards.pool_picks
    );
    // Banked points can only be spent from the `levelup` console command
    // today, so say what is waiting.
    let (skill_points, perk_points, pool_total) =
        (rewards.skill_points, rewards.perk_points, rewards.pool_picks);
    if granted.0 > 0.0 {
        crate::notifications::push(world, format!("+{:.0} skill points ({skill_points} unspent)", granted.0));
    }
    if granted.1 > 0 {
        crate::notifications::push(world, format!("+{} perk point(s) ({perk_points} unspent)", granted.1));
    }
    if pool_picks > 0 {
        crate::notifications::push(
            world,
            format!("+{pool_picks} Health/Magicka/Stamina pick(s) ({pool_total} unspent)"),
        );
    }
}

/// New Vegas Damage Threshold (M47.2): the flat damage an actor's DT absorbs
/// from each hit. Installed when the load order authors both the
/// `AVDamageThreshold` actor value (the GECK *Stats List*: "the actor's base
/// Damage Threshold level") and `fMinDamMultiplier` — New Vegas; Fallout 3
/// authors neither.
#[derive(Debug, Clone, Default)]
pub(crate) struct DamageThresholdRules {
    /// The `AVDamageThreshold` AVIF FormID.
    pub actor_value: u32,
    /// `fMinDamMultiplier` — "the proportion of damage an actor takes from a
    /// shot that would be negated by their Damage Threshold" (GECK, 0.2).
    pub min_damage_mult: f32,
    /// Each armor's `DNAM` DT, for the armors that author one.
    pub armor_dt: HashMap<u32, f32>,
}

impl Resource for DamageThresholdRules {}

/// Install (or, for a load order without DT, remove) [`DamageThresholdRules`].
pub(crate) fn install_damage_threshold_rules(
    world: &mut World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) -> bool {
    let actor_value = index
        .actor_values
        .values()
        .find(|av| av.editor_id.eq_ignore_ascii_case("AVDamageThreshold"))
        .map(|av| av.form_id);
    let (Some(actor_value), Some(min_damage_mult)) =
        (actor_value, index.game_setting_float("fMinDamMultiplier"))
    else {
        world.remove_resource::<DamageThresholdRules>();
        return false;
    };
    let armor_dt = index
        .items
        .iter()
        .filter_map(|(&form_id, item)| match item.kind {
            byroredux_plugin::esm::records::ItemKind::Armor { dt, .. } if dt > 0.0 => {
                Some((form_id, dt))
            }
            _ => None,
        })
        .collect();
    world.insert_resource(DamageThresholdRules {
        actor_value,
        min_damage_mult,
        armor_dt,
    });
    true
}

/// `target`'s Damage Threshold: its `DamageThreshold` actor value plus the DT
/// of every armor it has equipped. Each guard is released before the next is
/// taken (#3580's order-independence).
fn damage_threshold(world: &World, rules: &DamageThresholdRules, target: EntityId) -> f32 {
    use byroredux_core::ecs::components::{ActorValues, EquipmentSlots, Inventory};
    let base = world
        .get::<ActorValues>(target)
        .map_or(0.0, |values| values.current(rules.actor_value));
    let Some(equipped) = world
        .get::<EquipmentSlots>(target)
        .map(|slots| slots.equipped_indices().collect::<Vec<_>>())
    else {
        return base;
    };
    let worn: Vec<u32> = world
        .get::<Inventory>(target)
        .map(|inventory| {
            equipped
                .iter()
                .filter_map(|index| inventory.items.get(index.0 as usize))
                .map(|stack| stack.base_form_id)
                .collect()
        })
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    base + worn
        .into_iter()
        .filter(|form_id| seen.insert(*form_id))
        .filter_map(|form_id| rules.armor_dt.get(&form_id))
        .sum::<f32>()
}

/// `damage` after the target's Damage Threshold, perk-composed
/// (`compose_damage_threshold`): `max(damage − DT, damage ·
/// fMinDamMultiplier)`, the GECK's floor on what DT can negate. Unchanged
/// where the load order has no DT.
fn apply_damage_threshold(world: &World, aggressor: EntityId, target: EntityId, damage: f32) -> f32 {
    let Some(rules) = world
        .try_resource::<DamageThresholdRules>()
        .map(|rules| (*rules).clone())
    else {
        return damage;
    };
    let base = damage_threshold(world, &rules, target);
    let threshold = byroredux_scripting::perk_entry_points::compose_damage_threshold(
        world, aggressor, target, base,
    )
    .max(0.0);
    (damage - threshold).max(damage * rules.min_damage_mult)
}

pub(crate) fn attack_damage(world: &World, aggressor: EntityId) -> f32 {
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
/// Weapon damage". #5240 — the GECK *Stats Tab — NPC* page says
/// "Melee Damage: Not used." for NPCs; if an in-engine check confirms the AV
/// never enters NPC attack math, this bonus and the row's `ActorGeneral`
/// scope both narrow to the player (today the bonus reaches every aggressor
/// with the row, including `npc_combat_ai_system`'s) — until then the
/// status quo stands, per the capture's CONFLICTED row. (matching how `resolve_inherited_stats`/similar CHARAL
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
    // outer lock, and the `GetActorValue` arm in
    // `crates/scripting/src/condition/eval.rs` — the only site that reads the
    // component first — clones and drops before touching the ruleset. Do not
    // invert these two.
    let Some(ruleset) = world.try_resource::<CharacterRuleset>() else {
        return 0.0;
    };
    let Some(avs) = world.get::<ActorValues>(aggressor) else {
        return 0.0;
    };
    let level = world
        .get::<CharacterLevel>(aggressor)
        .map_or(1, |level| level.level);
    // #4452 — DerivedScope's consumer contract, the same both-fields check
    // as `GetActorValue` in `crates/scripting/src/condition/eval.rs`: this
    // helper evaluates the row for an *arbitrary* aggressor, so a `PlayerOnly`
    // row must not leak its
    // player formula onto an NPC, and a `Multiplier` row's raw ratio must
    // not be added to weapon damage (#2933's leak shape). Tautologically
    // false for every shipped MeleeDamage row (FO3/FNV's is
    // ActorGeneral + Absolute) — the check exists so a future row cannot
    // silently violate the contract a copy of this helper would skip.
    let Some(formula) = ruleset.derived_formula(melee_damage_avif) else {
        return 0.0;
    };
    if formula.scope != DerivedScope::ActorGeneral
        || formula.kind != DerivedOutput::Absolute
    {
        return 0.0;
    }
    // #5042 — the composer `GetActorValue` shares: the formula plus the
    // modifier layers a constant ability left (FNV `MotorRunnerBuff` +10),
    // or an authored carried value. The bare `derived_value` dropped the
    // ability, so the two CHARAL consumers disagreed on the same actor.
    ruleset.actor_value(melee_damage_avif, &avs, level)
}

/// Weapon-scaled melee reach. `EquippedWeapon::reach` is a multiplier on
/// [`MELEE_REACH_BU`] (1.0 = same reach as a longsword/mace); `0.0` means
/// the source game's weapon layout isn't decoded yet (see
/// `ItemKind::Weapon::reach`), so it falls back to the flat baseline —
/// same rule the unarmed case already uses.
pub(crate) fn attack_reach_bu(world: &World, aggressor: EntityId) -> f32 {
    world
        .get::<EquippedWeapon>(aggressor)
        .filter(|weapon| weapon.reach > 0.0)
        .map_or(MELEE_REACH_BU, |weapon| MELEE_REACH_BU * weapon.reach)
}

/// Weapon-scaled swing cooldown. `EquippedWeapon::speed` is a cadence
/// multiplier (>1.0 = faster attacks) on [`MELEE_COOLDOWN_SECONDS`]; `0.0`
/// (unset/undecoded) falls back to the flat baseline.
pub(crate) fn attack_cooldown_seconds(world: &World, aggressor: EntityId) -> f32 {
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
    // #5432 — a corpse stops talking: cut any dialogue voice line (and other
    // entity-anchored sound) the actor was speaking. Death sounds are
    // detached one-shots, so they are not affected.
    crate::systems::dialogue_voice::stop_line_voice(world, actor);
    // M47.2 — a dead `Boss` can clear its location (idempotent, so the
    // save-load sweep re-reaching a corpse is harmless).
    byroredux_scripting::note_actor_death(world, actor);
    // A freshly respawned corpse can regain its authored weapon before the
    // save's empty inventory is overlaid. Absence of EquippedWeapon is not a
    // saved tombstone, so reconstruct the take-all result from saved inventory.
    if world
        .get::<byroredux_core::ecs::components::Inventory>(actor)
        .is_some_and(|inventory| inventory.is_empty())
    {
        if let Some(mut equipment) =
            world.query_mut::<byroredux_core::ecs::components::EquipmentSlots>()
        {
            if let Some(slots) = equipment.get_mut(actor) {
                *slots = byroredux_core::ecs::components::EquipmentSlots::new();
            }
        }
        remove_component::<EquippedWeapon>(world, actor);
    }
    disable_actor_ai(world, actor);
    // #5870 — the package the actor was running ends with it: report
    // `-> None` so its Change fragment runs and `GetCurrentPackage` stops
    // answering with a corpse's last package. A second reconcile (save-load
    // sweep) finds nothing reported and is a no-op.
    if let Some(Some(package)) = byroredux_scripting::reported_package(world, actor) {
        byroredux_scripting::report_package_transition(world, actor, Some(package), None);
    }
    // #5861 — a corpse's behavior-graph runtime goes with its animation
    // players. Left in place, the graph step, `graph_locomotion_system` (which
    // sends `moveStop` for a parked corpse) and the lazy-clip restart in
    // `resolve_wanted` (every actor sharing the graph) all put a clip back on
    // it through `apply_playback_for`. While the graph lives its clips fight
    // the ragdoll writeback on every rig, so it goes at death.
    remove_component::<byroredux_scripting::ObjectAnimationGraph>(world, actor);
    remove_component::<crate::systems::GraphLocomotion>(world, actor);
    remove_component::<crate::systems::ConsoleWalk>(world, actor);
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
    // #4693 (GAME-D4-2026-09-21-01) — `AiCombatState` was torn down only by
    // `npc_combat_ai_system`'s own dead-attacker branch, so it survived one
    // tick past death; that branch also wrote the corpse's `Transform` to
    // world origin as a side effect of the unconditional decision apply,
    // and `Transform` is a saved mutable delta — the corpse root landed at
    // (0,0,0) in any save taken after the kill. Death teardown owns the
    // removal now (same #3708 precedent as `AmbientPackageRuntime`), and
    // the combat system keeps its own branch as the one-tick fallback for
    // deaths its reconcile pass doesn't see.
    remove_component::<byroredux_scripting::AiCombatState>(world, actor);
    // NPC playback can be owned by the placed actor, with root_entity
    // targeting the skeleton. Removing only the skeleton's player leaves
    // that controller sampling over the corpse every frame.
    remove_component::<AnimationPlayer>(world, actor);
    remove_component::<byroredux_core::animation::AnimationStack>(world, actor);
    let Some(skeleton_root) = world
        .get::<AnimationTarget>(actor)
        .map(|target| target.skeleton_root)
    else {
        return "; no ragdoll target".to_owned();
    };
    remove_component::<AnimationPlayer>(world, skeleton_root);
    remove_component::<byroredux_core::animation::AnimationStack>(world, skeleton_root);
    // #4814 — reconciliation can reach one corpse twice (an authored
    // "Starts Dead" actor queued at spawn, then swept again by a save
    // load's `reconcile_dead_actor_runtime_state`). `activate_ragdoll`
    // builds a fresh multibody every call, so the second pass must not.
    if world
        .get::<crate::ragdoll::RagdollActive>(skeleton_root)
        .is_some()
    {
        return "; ragdoll already active".to_owned();
    }
    // #5015 — an authored corpse is posed before its bodies are seeded, so it
    // lies where the plugin placed it instead of collapsing from the spawn
    // pose. One-shot: the pose is consumed whether or not it applies, and a
    // refused pose (an unfamiliar skeleton) falls back to the collapse.
    let posed = remove_component_taking::<crate::ragdoll_pose::AuthoredRagdollPose>(world, actor)
        .map(|pose| crate::ragdoll_pose::pose_skeleton(world, skeleton_root, &pose.0));
    let pose_note = match posed {
        Some(Ok(bones)) => format!("; posed {bones} bones from XRGD"),
        Some(Err(reason)) => {
            log::debug!("actor {actor}: XRGD pose not applied: {reason}");
            format!("; XRGD pose not applied ({reason})")
        }
        None => String::new(),
    };
    match crate::ragdoll::activate_ragdoll(world, skeleton_root) {
        Ok(body_count) => format!("{pose_note}; ragdoll activated ({body_count} bodies)"),
        Err(error) => format!("{pose_note}; ragdoll unavailable: {error}"),
    }
}

/// Remove `T` from `entity` and hand it back (`remove_component` discards it).
fn remove_component_taking<T: Component>(world: &World, entity: EntityId) -> Option<T> {
    world.query_mut::<T>()?.remove(entity)
}

/// M47.2 — `Game.RewardPlayerXP`'s XP modifier (FO4): `fXPModBase +
/// fXPModMult · INT` (Fallout4.esm authors 1.0 / 0.03 — 3% per point, the
/// Fallout wiki's measured Intelligence rule). Installed where both settings
/// are authored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct XpModifierRules {
    base: f32,
    per_intelligence: f32,
}

impl Resource for XpModifierRules {}

pub(crate) fn install_xp_modifier_rules(
    world: &mut World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) -> bool {
    match (
        index.game_setting_float("fXPModBase"),
        index.game_setting_float("fXPModMult"),
    ) {
        (Some(base), Some(per_intelligence)) => {
            world.insert_resource(XpModifierRules {
                base,
                per_intelligence,
            });
            true
        }
        _ => {
            world.remove_resource::<XpModifierRules>();
            false
        }
    }
}

/// The player's Intelligence in `view`, through the ruleset's composition.
fn player_intelligence(
    world: &World,
    player: EntityId,
    view: byroredux_scripting::condition::ActorValueView,
) -> f32 {
    world
        .try_resource::<byroredux_scripting::ActorValueNames>()
        .and_then(|names| names.resolve("Intelligence"))
        .map_or(0.0, |intelligence| {
            byroredux_scripting::condition::actor_value_view(world, player, intelligence, view)
        })
}

/// M47.2 — script `Game.RewardPlayerXP` grants (`PendingPlayerXp`): a
/// non-direct grant takes the XP modifier, then the shared level roll.
pub(crate) fn scripted_experience_system(world: &World, _dt: f32) {
    let grants = world
        .try_resource_mut::<byroredux_scripting::story_manager::PendingPlayerXp>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    if grants.is_empty() {
        return;
    }
    let Some(player) = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0)
    else {
        return;
    };
    let modifier = world.try_resource::<XpModifierRules>().map(|rules| *rules);
    for (amount, direct) in grants {
        let xp = match modifier {
            Some(rules) if !direct => {
                let mult = rules.base
                    + rules.per_intelligence
                        * player_intelligence(world, player, byroredux_scripting::condition::ActorValueView::Current);
                (amount as f32 * mult).round().max(0.0) as u32
            }
            _ => amount,
        };
        log::info!("Script experience: +{xp} XP");
        grant_experience(world, player, xp);
    }
}

/// M47.2 — script `Game.AdvanceSkill` calls (`PendingSkillAdvances`): the
/// usage XP banks against the player's skill, and each skill rank it reaches
/// feeds character XP (`LevelingModel::xp_from_skill_rank`) through the shared
/// level roll. Skyrim's skill-XP model only: another ruleset's player has no
/// skill-use leveling, so the call changes nothing there.
pub(crate) fn scripted_skill_system(world: &World, _dt: f32) {
    let advances = world
        .try_resource_mut::<byroredux_scripting::PendingSkillAdvances>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    if advances.is_empty() {
        return;
    }
    let Some(player) = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0)
    else {
        return;
    };
    let leveling = world
        .try_resource::<CharacterRuleset>()
        .map(|ruleset| ruleset.leveling);
    for (skill, magnitude) in advances {
        let reached = byroredux_scripting::advance_skill(world, player, &skill, magnitude);
        for level in reached {
            log::info!("Script skill advance: {skill} reached {level}");
            if let Some(xp) = leveling.and_then(|leveling| leveling.xp_from_skill_rank(level)) {
                grant_experience(world, player, xp.round().max(0.0) as u32);
            }
        }
    }
}

/// M47.2 — script `Kill` requests (`PendingScriptKills`), through the same
/// death path as a lethal hit: an essential victim goes into bleedout
/// instead (the CK *Kill* console page: "Essential actors will be made
/// 'unconscious'"); otherwise `Dead`, the KILL story event with the script's
/// killer, and the queued teardown. `Kill` does not change Health.
pub(crate) fn scripted_kill_system(world: &World, _dt: f32) {
    let kills = world
        .try_resource_mut::<byroredux_scripting::story_manager::PendingScriptKills>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    for (victim, killer, silent) in kills {
        if world.get::<Dead>(victim).is_some()
            || byroredux_scripting::spare_from_death(world, victim, killer)
        {
            continue;
        }
        if let Some(mut dead) = world.query_mut::<Dead>() {
            dead.insert(victim, Dead);
        }
        queue_dead_actor_reconciliation(world, victim);
        if silent {
            // CK *KillSilent*: "without a kill event" — the Story Manager's
            // KILL is not raised; the victim's own `OnDeath` still is.
            crate::systems::story_events::raise_death_event(world, victim, killer);
        } else {
            crate::systems::story_events::raise_kill_story_event(world, victim, killer);
        }
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
            // #6070 — the one record of a queued death's outcome (an authored
            // corpse's XRGD pose, its ragdoll); a combat kill traces its own.
            let outcome = reconcile_dead_actor(world, actor);
            log::debug!("actor {actor}: death reconciled{outcome}");
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
    if !actors.is_empty() {
        // Save deltas just changed local placements, and fresh NIF bones
        // may still carry model-space globals. Seed physics only after the
        // full hierarchy reflects those placements. Once built, a dynamic
        // ragdoll owns its pose; a later ordinary propagation cannot repair
        // a body accidentally created at the world origin.
        byroredux_core::ecs::make_transform_propagation_system()(world, 0.0);
    }
    for actor in actors.iter().copied() {
        reconcile_dead_actor(world, actor);
    }
    actors.len()
}

fn record_miss(world: &World, outcome: &str) {
    if let Some(mut state) = world.try_resource_mut::<CombatState>() {
        state.last = Some(CombatTraceEntry {
            target: None,
            incoming: 0.0,
            damage: 0.0,
            health_before: None,
            health_after: None,
            killed: false,
            outcome: outcome.to_owned(),
        });
    }
}

/// #5432 — death teardown cuts the corpse's voice. A headless world has no
/// audio manager to observe, so the shared reconciler's call is pinned by
/// source (every live and restored death funnels through it).
#[cfg(test)]
mod death_voice_source_tests {
    #[test]
    fn dead_actor_reconciler_stops_the_actors_voice_line() {
        let src = include_str!("combat.rs");
        let start = src
            .find("pub(crate) fn reconcile_dead_actor(")
            .expect("the shared death reconciler must exist");
        let body = &src[start..];
        let body = &body[..body.find("\n}\n").expect("function closes at column 0")];
        assert!(
            body.contains("dialogue_voice::stop_line_voice(world, actor)"),
            "reconcile_dead_actor must cut the dead actor's voice line (#5432)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::AmbientPackageRuntime;
    use byroredux_core::ecs::components::InventoryIndex;
    use byroredux_core::ecs::components::{FollowBehavior, FollowState};
    use byroredux_scripting::EvaluatePackageRequest;

    /// #4452 — the melee-damage helper honours DerivedScope's consumer
    /// contract (same both-fields check as `GetActorValue`): only an
    /// ActorGeneral + Absolute row may contribute weapon damage for an
    /// arbitrary aggressor. PlayerOnly rows must not leak their player
    /// formula, and a Multiplier row's raw ratio is not a damage addend.
    #[test]
    fn melee_charal_bonus_honors_scope_and_output_contracts() {
        use byroredux_core::character::{DerivedInput, DerivedStatFormula, LevelingModel};
        const STR: u32 = 0x05;
        const MELEE: u32 = 0x2E0;
        let world_with = |formula: DerivedStatFormula| {
            let mut world = World::new();
            world.insert_resource(MeleeDamageConfig {
                melee_damage_avif: MELEE,
            });
            world.insert_resource(
                CharacterRuleset::new(LevelingModel::FO4).with_derived(MELEE, formula),
            );
            let actor = world.spawn();
            world.insert(actor, ActorValues::from_pairs([(STR, 6.0)]));
            world.insert(actor, CharacterLevel { level: 3, xp: 0 });
            (world, actor)
        };
        let affine = || DerivedStatFormula::affine(DerivedInput::actor_value(STR), 0.5, 2.0);

        let (world, actor) = world_with(affine());
        assert_eq!(melee_damage_charal_bonus(&world, actor), 5.0);

        let (world, actor) = world_with(affine().player_only());
        assert_eq!(
            melee_damage_charal_bonus(&world, actor),
            0.0,
            "PlayerOnly MeleeDamage row must not apply to an arbitrary aggressor"
        );

        let (world, actor) = world_with(affine().as_multiplier());
        assert_eq!(
            melee_damage_charal_bonus(&world, actor),
            0.0,
            "a Multiplier row's raw ratio is not a damage addend"
        );

        // #5042 — FNV `MotorRunnerBuff` shape: a constant MeleeDamage +10 on
        // an actor whose derivation carries no MeleeDamage adds to the
        // formula instead of being dropped.
        let (mut world, actor) = world_with(affine());
        world
            .get_mut::<ActorValues>(actor)
            .unwrap()
            .mod_permanent(MELEE, 10.0);
        assert_eq!(melee_damage_charal_bonus(&world, actor), 15.0);
    }

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

    /// M47.2 — a strike carries the damage the aggressor's perks compose:
    /// vanilla FalloutNV `BloodyMess` (hook 0, ×1.05) at rank 1 turns an
    /// 18-damage weapon into 18.9, and an actor without the perk is
    /// unchanged.
    #[test]
    fn attack_damage_against_composes_the_aggressors_perk_entry_points() {
        use byroredux_core::character::Perks;
        use byroredux_plugin::esm::reader::GameKind;
        use byroredux_plugin::esm::records::{
            EsmIndex, PerkEntry, PerkEntryBody, PerkFunctionData, PerkRecord,
        };

        const BLOODY_MESS: u32 = 0x94EBA;
        let mut index = EsmIndex {
            game: GameKind::Fallout3NV,
            character_rules: byroredux_core::character::CharacterRulesProfile::FALLOUT_NEW_VEGAS,
            ..Default::default()
        };
        index.perks.insert(
            BLOODY_MESS,
            PerkRecord {
                form_id: BLOODY_MESS,
                entries: vec![PerkEntry {
                    rank: 0,
                    priority: 0,
                    body: PerkEntryBody::EntryPoint {
                        entry_point_index: 0,
                        function: 3,
                        tab_count: 3,
                        param_type: 1,
                        function_data: PerkFunctionData::Float(1.05),
                        formatter: Vec::new(),
                        extra_flags: Vec::new(),
                    },
                    condition_tabs: Vec::new(),
                }],
                ..Default::default()
            },
        );
        let (mut world, aggressor, target) = damage_fixture(50.0, Some(18.0), false);
        world.register::<Perks>();
        byroredux_scripting::perk_entry_points::install_perk_entry_points(&mut world, &index);

        // No perk yet: the authored weapon damage.
        assert_eq!(attack_damage_against(&world, aggressor, target), 18.0);
        let mut perks = Perks::default();
        perks.set_rank(BLOODY_MESS, 1);
        world.insert(aggressor, perks);
        let composed = attack_damage_against(&world, aggressor, target);
        assert!((composed - 18.9).abs() < 1e-4, "{composed}");
        // The unperked target hits for the baseline.
        assert_eq!(attack_damage_against(&world, target, aggressor), UNARMED_DAMAGE);
    }

    /// M47.2 — New Vegas Damage Threshold: the target's armor DT absorbs a
    /// hit, a `Modify Damage Threshold (defender)` perk (vanilla `Toughness`,
    /// hook 56, Add 3) raises it, and `fMinDamMultiplier` floors what DT can
    /// negate.
    #[test]
    fn damage_threshold_absorbs_hits_down_to_the_floor() {
        use byroredux_core::character::Perks;
        use byroredux_core::ecs::components::{EquipmentSlots, Inventory, ItemStack};
        use byroredux_plugin::esm::reader::GameKind;
        use byroredux_plugin::esm::records::{
            EsmIndex, PerkEntry, PerkEntryBody, PerkFunctionData, PerkRecord,
        };
        const LEGATE_ARMOR: u32 = 0x0010_0001;
        const TOUGHNESS: u32 = 0x0003_1DB9;
        let (mut world, aggressor, target) = damage_fixture(50.0, Some(20.0), false);
        world.register::<Perks>();
        world.register::<Inventory>();
        world.register::<EquipmentSlots>();
        world.insert_resource(DamageThresholdRules {
            actor_value: 0x5FA,
            min_damage_mult: 0.2,
            armor_dt: HashMap::from([(LEGATE_ARMOR, 15.0)]),
        });
        assert_eq!(apply_damage_threshold(&world, aggressor, target, 20.0), 20.0, "no armor");

        let mut inventory = Inventory::new();
        inventory.push(ItemStack::new(LEGATE_ARMOR, 1));
        world.insert(target, inventory);
        let mut slots = EquipmentSlots::new();
        slots.equip(0x4, InventoryIndex(0));
        world.insert(target, slots);
        assert_eq!(apply_damage_threshold(&world, aggressor, target, 20.0), 5.0, "20 − DT 15");

        let mut index = EsmIndex {
            game: GameKind::Fallout3NV,
            character_rules: byroredux_core::character::CharacterRulesProfile::FALLOUT_NEW_VEGAS,
            ..Default::default()
        };
        index.perks.insert(
            TOUGHNESS,
            PerkRecord {
                form_id: TOUGHNESS,
                entries: vec![PerkEntry {
                    rank: 0,
                    priority: 0,
                    body: PerkEntryBody::EntryPoint {
                        entry_point_index: 56,
                        function: 2,
                        tab_count: 3,
                        param_type: 1,
                        function_data: PerkFunctionData::Float(3.0),
                        formatter: Vec::new(),
                        extra_flags: Vec::new(),
                    },
                    condition_tabs: Vec::new(),
                }],
                ..Default::default()
            },
        );
        byroredux_scripting::perk_entry_points::install_perk_entry_points(&mut world, &index);
        let mut perks = Perks::default();
        perks.set_rank(TOUGHNESS, 1);
        world.insert(target, perks);
        // DT 18 would leave 2; the floor keeps 20 × 0.2 = 4.
        assert_eq!(apply_damage_threshold(&world, aggressor, target, 20.0), 4.0);
        // Where the hit lands: the trace keeps the incoming 20 and applies 4.
        combat_damage_system(&world, 0.0);
        let last = world.resource::<CombatState>().last.clone().unwrap();
        assert_eq!((last.incoming, last.damage), (20.0, 4.0));
    }

    /// M47.2 — a script `Kill` takes a plain actor through the death path
    /// (Dead, the KILL story event, the queued teardown) and puts an
    /// essential one into bleedout without touching Health.
    #[test]
    fn a_scripted_kill_takes_the_death_path_or_bleeds_out() {
        let (mut world, killer, target) = damage_fixture(50.0, None, false);
        world.insert_resource(PendingDeathReconciliations::default());
        world.register::<byroredux_scripting::story_manager::StoryEvent>();
        let essential = world.spawn();
        world.insert(essential, ActorValues::from_pairs([(0x2D4, 50.0)]));
        world.insert(essential, ActorVitals { health: 0x2D4 });
        world.insert(
            essential,
            byroredux_scripting::ActorProtection {
                essential: true,
                protected: false,
            },
        );
        world
            .resource_mut::<byroredux_scripting::story_manager::PendingScriptKills>()
            .0
            .extend([(target, Some(killer), false), (essential, None, false)]);

        scripted_kill_system(&world, 0.0);
        assert!(world.get::<Dead>(target).is_some());
        assert_eq!(world.resource::<PendingDeathReconciliations>().queued(), &[target]);
        assert!(world.get::<Dead>(essential).is_none());
        assert!(byroredux_scripting::is_bleeding_out(&world, essential));
        assert_eq!(world.get::<ActorValues>(essential).unwrap().current(0x2D4), 50.0);
        assert!(world
            .resource::<byroredux_scripting::story_manager::PendingScriptKills>()
            .0
            .is_empty());
    }

    /// M47.2 — `KillSilent` is the same death "without a kill event" (CK
    /// *KillSilent - Actor*): Dead and the queued teardown, the victim's own
    /// `OnDeath`, but no KILL story event.
    #[test]
    fn a_silent_scripted_kill_raises_no_kill_story_event() {
        let (mut world, killer, target) = damage_fixture(50.0, None, false);
        world.insert_resource(PendingDeathReconciliations::default());
        world.register::<byroredux_scripting::story_manager::StoryEvent>();
        world.register::<byroredux_scripting::OnDeathEvent>();
        world
            .resource_mut::<byroredux_scripting::story_manager::PendingScriptKills>()
            .0
            .push((target, Some(killer), true));
        scripted_kill_system(&world, 0.0);
        assert!(world.get::<Dead>(target).is_some());
        assert_eq!(world.resource::<PendingDeathReconciliations>().queued(), &[target]);
        assert!(world.get::<byroredux_scripting::story_manager::StoryEvent>(target).is_none());
        assert!(world.get::<byroredux_scripting::OnDeathEvent>(target).is_some());
    }

    /// M47.2 — a lethal hit on an essential actor leaves it in bleedout, not
    /// dead: no `Dead`, no KILL, no kill counted.
    #[test]
    fn a_lethal_hit_on_an_essential_actor_starts_bleedout() {
        let (mut world, _aggressor, target) = damage_fixture(10.0, Some(25.0), false);
        world.insert(
            target,
            byroredux_scripting::ActorProtection {
                essential: true,
                protected: false,
            },
        );
        combat_damage_system(&world, 0.0);
        assert!(world.get::<Dead>(target).is_none());
        assert!(byroredux_scripting::is_bleeding_out(&world, target));
        let state = world.resource::<CombatState>();
        let last = state.last.clone().unwrap();
        assert!(!last.killed);
        assert!(last.outcome.ends_with("; bleedout"), "{}", last.outcome);
        assert_eq!(state.kills, 0);
    }

    /// M47.2 — Fallout kill experience: a kill the player made (≥ the health
    /// share) pays the victim's difficulty band, and a level's worth rolls over.
    #[test]
    fn a_player_kill_pays_its_band_and_rolls_levels() {
        use byroredux_core::character::{CharacterLevel, LevelingModel};
        let rules = ExperienceRules {
            npc_bands: vec![(0.0, 0.0), (1.0, 10.0), (7.0, 20.0), (10.0, 30.0), (13.0, 50.0)],
            creature_bands: vec![(1.0, 1.0), (3.0, 5.0), (6.0, 10.0), (9.0, 25.0), (12.0, 50.0)],
            health_share: 0.4,
            creature_bases: std::collections::HashSet::from([0xC0]),
        };
        let kill = |victim_level: u16, damage: f32, player_xp: u32| {
            let (mut world, aggressor, target) = damage_fixture(50.0, Some(damage), false);
            world.register::<CharacterLevel>();
            world.insert_resource(crate::systems::PlayerEntity(Some(aggressor)));
            world.insert_resource(rules.clone());
            world.insert_resource(PlayerDamageDealt::default());
            world.insert_resource(CharacterRuleset::new(LevelingModel::FNV));
            world.insert(aggressor, CharacterLevel { level: 1, xp: player_xp });
            world.insert(target, CharacterLevel { level: victim_level, xp: 0 });
            combat_damage_system(&world, 0.0);
            let level = *world.get::<CharacterLevel>(aggressor).unwrap();
            (level.level, level.xp)
        };
        assert_eq!(kill(8, 60.0, 0), (1, 30), "level 8 is the Hard band");
        assert_eq!(kill(20, 60.0, 0), (1, 50), "above the last band pays Very Hard");
        assert_eq!(kill(8, 10.0, 0), (1, 0), "no kill, no experience");
        let to_next = LevelingModel::FNV.xp_to_next(1) as u32;
        assert_eq!(kill(8, 60.0, to_next - 10), (2, 20), "rolls over into level 2");
    }

    #[test]
    fn experience_roll_stops_at_the_fallout_new_vegas_level_cap() {
        use byroredux_core::character::{CharacterLevel, LevelingModel};

        let advance = |level| {
            let mut world = World::new();
            world.register::<CharacterLevel>();
            world.insert_resource(CharacterRuleset::new(LevelingModel::FNV));
            let player = world.spawn();
            world.insert(player, CharacterLevel { level, xp: 0 });
            grant_experience(&world, player, 10_000);
            let progress = *world
                .get::<CharacterLevel>(player)
                .expect("level remains present");
            progress
        };

        assert_eq!(
            advance(29),
            CharacterLevel {
                level: 30,
                xp: 5_600,
            },
            "level 29 crosses once and stops at the cap, retaining unused XP"
        );
        assert_eq!(
            advance(30),
            CharacterLevel {
                level: 30,
                xp: 10_000,
            },
            "a capped character may accumulate XP but cannot advance beyond 30"
        );
    }

    /// The rules install only where the load order authors both the
    /// `AVDamageThreshold` actor value and `fMinDamMultiplier`.
    #[test]
    fn damage_threshold_rules_install_only_with_their_records() {
        use byroredux_plugin::esm::records::{AvifRecord, EsmIndex, GameSetting, SettingValue};
        let mut world = World::new();
        let mut index = EsmIndex::default();
        index.actor_values.insert(
            0x5FA,
            AvifRecord {
                form_id: 0x5FA,
                editor_id: "AVDamageThreshold".into(),
                ..Default::default()
            },
        );
        assert!(!install_damage_threshold_rules(&mut world, &index), "no GMST");
        index.game_settings.insert(
            0x10,
            GameSetting {
                form_id: 0x10,
                editor_id: "fMinDamMultiplier".into(),
                value: SettingValue::Float(0.2),
            },
        );
        assert!(install_damage_threshold_rules(&mut world, &index));
        assert_eq!(world.resource::<DamageThresholdRules>().actor_value, 0x5FA);
        assert!(!install_damage_threshold_rules(&mut world, &EsmIndex::default()));
        assert!(world.try_resource::<DamageThresholdRules>().is_none());
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
        let (mut world, aggressor, target) = damage_fixture(10.0, Some(18.0), false);
        world.register::<byroredux_scripting::story_manager::StoryEvent>();
        combat_damage_system(&world, 0.0);

        assert!(world.get::<Dead>(target).is_some());
        // #5434 — the combat producer routes through the shared KILL
        // helper: R1 = victim, R2 = the aggressor.
        let kill = world
            .query::<byroredux_scripting::story_manager::StoryEvent>()
            .and_then(|events| events.get(target).copied())
            .expect("a lethal hit raises KILL on the victim");
        assert_eq!(kill.reference_1, target);
        assert_eq!(kill.reference_2, Some(aggressor));
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

    /// M47.2 — a ghost "cannot be harmed by a weapon or spell in any way"
    /// (CS / GECK *SetGhost*): the lethal hit lands for nothing.
    #[test]
    fn a_ghost_takes_no_weapon_damage() {
        let (mut world, _aggressor, target) = damage_fixture(10.0, Some(18.0), false);
        world.register::<byroredux_scripting::ActorControlState>();
        assert!(byroredux_scripting::update_actor_control(&world, target, |state| {
            state.ghost = true
        }));
        combat_damage_system(&world, 0.0);
        assert_eq!(world.get::<ActorValues>(target).unwrap().current(0x2D4), 10.0);
        assert!(world.get::<Dead>(target).is_none());
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
    fn dead_reconciliation_clears_respawned_equipment_only_for_empty_inventory() {
        use byroredux_core::ecs::components::{EquipmentSlots, Inventory, ItemStack};
        for looted in [false, true] {
            let (mut world, _, actor) = damage_fixture(10.0, None, false);
            world.insert(actor, Dead);
            let mut slots = EquipmentSlots::new();
            slots.equip_weapon(InventoryIndex(0));
            world.insert(actor, slots);
            world.insert(
                actor,
                EquippedWeapon {
                    inventory_index: InventoryIndex(0),
                    base_form_id: 0x1234,
                    damage: 12.0,
                    reach: 1.0,
                    speed: 1.0,
                },
            );
            world.insert(
                actor,
                Inventory {
                    items: if looted {
                        vec![]
                    } else {
                        vec![ItemStack::new(0x1234, 1)]
                    },
                },
            );
            reconcile_dead_actor_runtime_state(&world);
            assert_eq!(world.get::<EquippedWeapon>(actor).is_none(), looted);
            assert_eq!(
                world.get::<EquipmentSlots>(actor).unwrap().weapon.is_none(),
                looted
            );
        }
    }

    #[test]
    fn dead_state_reconciliation_removes_respawned_ai() {
        let mut world = World::new();
        world.register::<Dead>();
        world.register::<FollowBehavior>();
        world.register::<FollowState>();
        world.register::<AmbientPackageRuntime>();
        world.register::<EvaluatePackageRequest>();
        world.register::<AnimationPlayer>();
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
        world.insert(actor, AnimationPlayer::new(0));

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
        assert!(world.get::<AnimationPlayer>(actor).is_none());
    }

    #[test]
    fn restored_corpse_seeds_physics_from_saved_placement_not_stale_globals() {
        use crate::ragdoll::{RagdollActive, RagdollTemplate, RagdollTemplateBody};
        use byroredux_core::ecs::{Children, GlobalTransform, Parent, Transform};
        use byroredux_core::math::{Quat, Vec3};
        use byroredux_physics::{PhysicsWorld, Ragdoll};

        let mut world = World::new();
        world.register::<Dead>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<Parent>();
        world.register::<Children>();
        world.register::<AnimationTarget>();
        world.register::<RagdollTemplate>();
        world.register::<Ragdoll>();
        world.register::<RagdollActive>();
        world.insert_resource(PhysicsWorld::new());
        world.register::<AnimationPlayer>();
        world.register::<byroredux_core::animation::AnimationStack>();
        let actor = world.spawn();
        let skeleton = world.spawn();
        for owner in [actor, skeleton] {
            let mut player = AnimationPlayer::new(0);
            player.root_entity = Some(skeleton);
            world.insert(owner, player);
            let mut stack = byroredux_core::animation::AnimationStack::new();
            stack.root_entity = Some(skeleton);
            stack.play(0, 0.0);
            world.insert(owner, stack);
        }
        let placement = Transform::new(
            Vec3::new(1200.0, 300.0, -700.0),
            Quat::from_rotation_y(0.7),
            1.5,
        );
        let local = Vec3::new(10.0, 50.0, 0.0);
        world.insert(actor, placement);
        world.insert(actor, Dead);
        world.insert(actor, Children(vec![skeleton]));
        world.insert(
            actor,
            AnimationTarget {
                skeleton_root: skeleton,
                consumed_idle_serial: 0,
            },
        );
        world.insert(skeleton, Parent(actor));
        world.insert(skeleton, Transform::new(local, Quat::IDENTITY, 1.0));
        // A freshly rebuilt hierarchy / just-applied save delta has not
        // reached PostUpdate propagation yet.
        world.insert(actor, GlobalTransform::IDENTITY);
        world.insert(skeleton, GlobalTransform::IDENTITY);
        world.insert(
            skeleton,
            RagdollTemplate {
                bodies: vec![RagdollTemplateBody {
                    bone: skeleton,
                    local_translation: Vec3::ZERO,
                    local_rotation: Quat::IDENTITY,
                    shape: byroredux_core::ecs::components::CollisionShape::Ball { radius: 5.0 },
                    mass: 1.0,
                    linear_damping: 0.05,
                    angular_damping: 0.05,
                    friction: 0.5,
                    restitution: 0.0,
                    pose_bone_id: None,
                }],
                constraints: vec![],
            },
        );
        assert_eq!(reconcile_dead_actor_runtime_state(&world), 1);
        let handle = world.get::<Ragdoll>(skeleton).unwrap().bodies[0].1;
        for owner in [actor, skeleton] {
            assert!(world.get::<AnimationPlayer>(owner).is_none());
            assert!(world
                .get::<byroredux_core::animation::AnimationStack>(owner)
                .is_none());
        }
        let physics = world.resource::<PhysicsWorld>();
        let (position, rotation) = byroredux_physics::ragdoll::body_pose(&physics, handle).unwrap();
        let expected = placement.translation + placement.rotation * (local * placement.scale);
        assert!(
            (position - expected).length() < 0.001,
            "{position:?} != {expected:?}"
        );
        assert!(rotation.abs_diff_eq(placement.rotation, 0.0001));
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

    /// #4701 — a `Dead` player's attack edge is refused exactly like a
    /// fly-cam press: no swing counted, no cooldown armed, no `HitEvent`.
    #[test]
    fn dead_player_attack_press_is_not_a_swing() {
        let (mut world, aggressor) = attack_edge_fixture(PlayerMode::Character);
        world.insert(aggressor, Dead);
        press_attack(&world);

        combat_input_system(&world, 1.0 / 60.0);

        let state = world.resource::<CombatState>();
        assert_eq!(state.attacks_started, 0);
        assert!(state.last.is_none());
        drop(state);
        assert_eq!(melee_state_of(&world, aggressor).cooldown_remaining, 0.0);
        assert!(world
            .query::<byroredux_scripting::HitEvent>()
            .is_none_or(|events| events.iter().next().is_none()));
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
        if std::env::var_os("BYRO_LOCK_ORDER_CHECK").as_deref() != Some(std::ffi::OsStr::new("1")) {
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
