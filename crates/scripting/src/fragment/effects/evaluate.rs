//! Value and guard evaluation plus the small actor-value helpers they use. (#5468, split from `effects.rs`)

use super::*;

/// Evaluate one [`FragmentGuard`] (M47.2) against live quest, objective,
/// global and quest-variable state. `None` when its quest or global does not
/// resolve — the caller then declines the whole `Conditional`.
///
/// A quest-variable read sees a write queued earlier in the same fragment
/// (`deferred.quest_variables`, applied only once the quest guards drop) —
/// Papyrus assignments are immediate. `Globals` and the quest-variable
/// resources are read under the quest-state guards, the same nested reads
/// the `SetGlobalValue` arm already takes.
/// A message an effect raised, shown once the quest guards drop.
#[derive(Debug)]
pub(super) enum PendingMessage {
    Show(u32, Vec<f32>),
    Notify(String),
    /// A non-silent scripted add to the player: `(item base FormID, count)`,
    /// named and worded when it is shown (#5913).
    ItemAdded(u32, u32),
}

/// M47.2 — a [`ValueExpr`]'s run-time number; `None` when something it reads
/// does not resolve (the guard then declines the conditional). Quest-variable
/// and relationship reads see a write made earlier in the same fragment.
/// An actor's current value of `avif` — the `GetActorValue` condition read.
pub(super) fn actor_value_now(world: &World, actor: EntityId, avif: u32) -> f32 {
    crate::condition::evaluate_function(
        crate::condition::ConditionFunction::GetActorValue,
        &byroredux_plugin::esm::records::condition::Condition {
            param_1: avif,
            ..Default::default()
        },
        actor,
        world,
    )
}

/// The (quest, alias id) the script property `name` is bound to, a quest-less
/// binding (form 0) meaning the running quest `context`.
pub(super) fn alias_property(vmad: Option<&ScriptInstanceData>, context: QuestFormId, name: &str) -> Option<(QuestFormId, i32)> {
    vmad?
        .scripts
        .iter()
        .find_map(|script| script.property(name))
        .and_then(|property| match property.value {
            PropertyValue::Object { form_id, alias } if alias >= 0 => Some((
                if form_id != 0 { QuestFormId(form_id) } else { context },
                i32::from(alias),
            )),
            _ => None,
        })
}

pub(in crate::fragment) fn evaluate_value(
    value: &ValueExpr,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &QuestStageState,
    deferred: &DeferredFragmentEffects,
) -> Option<f32> {
    Some(match value {
        ValueExpr::Literal(value) => *value,
        ValueExpr::Local(local) => deferred.locals.get(local).copied().unwrap_or(0.0),
        ValueExpr::Stage(via) => f32::from(stages.get_stage(resolve_quest_logged(via, context, vmad)?)),
        ValueExpr::Global(global) => {
            let form_id = resolve_property_form_id(vmad, global.property_name())?;
            world
                .try_resource::<crate::Globals>()
                .and_then(|globals| globals.get(form_id))?
        }
        ValueExpr::QuestVariable(via, variable) => {
            let quest = resolve_quest_logged(via, context, vmad)?;
            let key = crate::QuestScriptVariables::property_key(variable);
            let pending = deferred
                .quest_variables
                .iter()
                .rev()
                .find(|(q, k, _)| *q == quest && *k == key)
                .map(|(_, _, value)| *value);
            pending
                .or_else(|| crate::quest_variable(world, quest, key))
                .unwrap_or(0.0)
        }
        // CK *GetItemCount*: a form list "will count how many of each item in
        // the form list is in the container and sum it all up" — the one
        // count the CTDA and ObScript getter share (#6251).
        ValueExpr::ItemCount(container, item) => {
            let container = guard_actor(container, context, vmad, world, deferred)?;
            let item = resolve_property_form_id(vmad, item.property_name())?;
            crate::condition::item_count(world, container, item)
        }
        ValueExpr::Distance(from, to) => {
            use byroredux_core::ecs::components::GlobalTransform;
            let from = guard_actor(from, context, vmad, world, deferred)?;
            let to = guard_actor(to, context, vmad, world, deferred)?;
            // Like the GetDistance condition: a reference with no placement
            // reads as infinitely far.
            let from = world.get::<GlobalTransform>(from).map(|t| t.translation);
            let to = world.get::<GlobalTransform>(to).map(|t| t.translation);
            match (from, to) {
                (Some(from), Some(to)) => (from - to).length(),
                _ => f32::MAX,
            }
        }
        ValueExpr::RelationshipRank(actor, other) => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let other = guard_actor(other, context, vmad, world, deferred)?;
            let pair = crate::relationships::actor_base(world, actor)
                .zip(crate::relationships::actor_base(world, other))
                .map(|(a, b)| crate::relationships::pair(a, b));
            // A `SetRelationshipRank` earlier in the same fragment is visible.
            let pending = pair.and_then(|pair| {
                deferred
                    .relationship_changes
                    .iter()
                    .rev()
                    .find(|(changed, _)| *changed == pair)
                    .map(|&(_, rank)| rank)
            });
            f32::from(
                pending.unwrap_or_else(|| crate::relationships::relationship_rank(world, actor, other)),
            )
        }
        ValueExpr::ActorValue(actor, actor_value) => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let avif = resolve_property_form_id(vmad, actor_value.property_name())?;
            actor_value_now(world, actor, avif)
        }
        ValueExpr::ActorValueNamed(actor, name) => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let avif = world
                .try_resource::<crate::ActorValueNames>()
                .and_then(|names| names.resolve(name))?;
            actor_value_now(world, actor, avif)
        }
        ValueExpr::ActorValueReading(reading, actor, arg) => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let avif = match arg {
                crate::translate::effects::ActorValueArg::Form(form) => {
                    resolve_property_form_id(vmad, form.property_name())?
                }
                crate::translate::effects::ActorValueArg::Named(name) => world
                    .try_resource::<crate::ActorValueNames>()
                    .and_then(|names| names.resolve(name))?,
            };
            use crate::condition::{actor_value_view, ActorValueView};
            match reading {
                crate::translate::effects::ActorValueReading::Base => {
                    actor_value_view(world, actor, avif, ActorValueView::Base)
                }
                // Current over the undamaged ceiling; an actor value the
                // actor lacks reads 0, the absent-AV default.
                crate::translate::effects::ActorValueReading::Percent => {
                    let ceiling = actor_value_view(world, actor, avif, ActorValueView::Undamaged);
                    if ceiling > 0.0 {
                        (actor_value_view(world, actor, avif, ActorValueView::Current) / ceiling).clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                }
            }
        }
        ValueExpr::LocationKeywordData(location, keyword) => {
            let location = resolve_property_form_id(vmad, location.property_name())?;
            let keyword = resolve_property_form_id(vmad, keyword.property_name())?;
            // A write earlier in the same run is visible.
            deferred
                .location_keyword_changes
                .iter()
                .rev()
                .find(|(changed, key, _)| *changed == location && *key == keyword)
                .map(|(_, _, value)| *value)
                .unwrap_or_else(|| crate::location_keyword_data(world, location, keyword))
        }
        // A read sees the increments queued earlier in the same run.
        ValueExpr::MiscStat(stat) => {
            let stored = world.try_resource::<crate::MiscStats>().map_or(0, |stats| stats.get(stat));
            let pending: i32 = deferred
                .misc_stats
                .iter()
                .filter(|(queued, _)| queued.eq_ignore_ascii_case(stat))
                .map(|(_, amount)| *amount)
                .sum();
            stored.saturating_add(pending) as f32
        }
        ValueExpr::Level(actor) => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            crate::condition::evaluate_function(
                crate::condition::ConditionFunction::GetLevel,
                &byroredux_plugin::esm::records::condition::Condition::default(),
                actor,
                world,
            )
        }
        ValueExpr::RandomInt(min, max) => {
            // `Utility.RandomInt` draws from the seeded quest RNG the Story
            // Manager and `GetRandomPercent` share; unresolved without one.
            if max < min {
                return Some(*min as f32);
            }
            let span = (i64::from(*max) - i64::from(*min) + 1) as usize;
            let pick = world
                .try_resource_mut::<crate::story_manager::StoryManagerRng>()
                .map(|mut rng| rng.pick(span))?;
            (i64::from(*min) + pick as i64) as f32
        }
        ValueExpr::Arith(left, op, right) => op.apply(
            evaluate_value(left, context, vmad, world, stages, deferred)?,
            evaluate_value(right, context, vmad, world, stages, deferred)?,
        )?,
        ValueExpr::Abs(inner) => {
            evaluate_value(inner, context, vmad, world, stages, deferred)?.abs()
        }
    })
}

pub(in crate::fragment) fn evaluate_guard(
    guard: &FragmentGuard,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &QuestStageState,
    objectives: &QuestObjectiveState,
    deferred: &DeferredFragmentEffects,
) -> Option<bool> {
    Some(match guard {
        FragmentGuard::StageDone { .. }
        | FragmentGuard::QuestRunning { .. }
        | FragmentGuard::Stage { .. }
        | FragmentGuard::ObjectiveDisplayed { .. }
        | FragmentGuard::ObjectiveCompleted { .. }
        | FragmentGuard::Global { .. }
        | FragmentGuard::QuestVariable { .. }
        | FragmentGuard::QuestCompleted { .. } => evaluate_quest_guard(guard, context, vmad, world, stages, objectives, deferred)?,
        FragmentGuard::ActorDead { .. }
        | FragmentGuard::ActorBleedingOut { .. }
        | FragmentGuard::ActorInCombat { .. }
        | FragmentGuard::ActorFollower { .. }
        | FragmentGuard::ActorEssential { .. }
        | FragmentGuard::InFaction { .. }
        | FragmentGuard::HasLocRefType { .. }
        | FragmentGuard::RelationshipRank { .. }
        | FragmentGuard::HasPerk { .. }
        | FragmentGuard::HasKeyword { .. }
        | FragmentGuard::LocationKeyword { .. } => evaluate_actor_guard(guard, context, vmad, world, stages, deferred)?,
        FragmentGuard::ReferenceEnabled { .. }
        | FragmentGuard::Loaded3D { .. }
        | FragmentGuard::Distance { .. }
        | FragmentGuard::Compare { .. }
        | FragmentGuard::ItemCount { .. }
        | FragmentGuard::ScenePlaying { .. }
        | FragmentGuard::SameReference { .. }
        | FragmentGuard::MapMarkerVisible { .. }
        | FragmentGuard::MapMarkerFastTravel { .. }
        | FragmentGuard::LocationCleared { .. } => evaluate_world_guard(guard, context, vmad, world, stages, deferred)?,
        FragmentGuard::Any(alternatives) => {
            // Every guard is evaluated (no short-circuit), so an unresolvable
            // one declines the conditional exactly as it does in a
            // conjunction.
            let mut any = false;
            for alternative in alternatives {
                let mut all = true;
                for guard in alternative {
                    all &= evaluate_guard(guard, context, vmad, world, stages, objectives, deferred)?;
                }
                any |= all;
            }
            any
        }
    })
}

/// The reference FormID an [`ActorRef`] names: `0x14` for the player, else the
/// reference's own FormID (a resident entity's identity, or a remote alias
/// stub's candidate FormID).
pub(super) fn actor_reference_form_id(
    actor: &ActorRef,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    deferred: &DeferredFragmentEffects,
) -> Option<u32> {
    if matches!(actor, ActorRef::Player) {
        return Some(crate::PLAYER_REFERENCE);
    }
    if let ActorRef::Object(object) = actor {
        if let Some(form_id) = resolve_property_form_id(vmad, object.property_name()) {
            return Some(form_id);
        }
    }
    let entity = guard_actor(actor, context, vmad, world, deferred)?;
    entity_global_form_id(world, entity).or_else(|| {
        world
            .get::<crate::scene::SceneAliasCandidate>(entity)
            .map(|candidate| candidate.reference_form_id)
    })
}

/// Set `actor`'s `avif`, creating `ActorValues` if it has none. A player's
/// player-only derived pool is an override over its ruleset formula; writing
/// its base would be lost on the next derived-stat refresh. Read the player
/// and ruleset resources before taking the `ActorValues` write guard (#3441).
pub(super) fn set_actor_value_base(world: &World, actor: EntityId, avif: u32, value: f32) {
    let is_player_derived_pool = crate::papyrus_demo::PapyrusPlayerEntity::get(world) == Some(actor)
        && world
            .try_resource::<byroredux_core::character::CharacterRuleset>()
            .is_some_and(|ruleset| ruleset.is_player_derived_pool(avif));
    let Some(mut values) = world.query_mut::<byroredux_core::ecs::components::ActorValues>()
    else {
        return;
    };
    if values.get_mut(actor).is_none() {
        values.insert(actor, byroredux_core::ecs::components::ActorValues::new());
    }
    let values = values.get_mut(actor).expect("present or inserted");
    if is_player_derived_pool {
        values.set_override(avif, value);
    } else {
        values.set_base(avif, value);
    }
}

/// Read `value` with no quest guard held by the caller (M47.3: an ObScript
/// getter lowered onto the Papyrus read model): the stage resource is
/// read-locked for the call. `None` when the value cannot be read.
pub(crate) fn evaluate_value_guard_free(
    world: &World,
    value: &ValueExpr,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
) -> Option<f32> {
    let deferred = DeferredFragmentEffects::new(world);
    let stages = world.try_resource::<QuestStageState>()?;
    evaluate_value(value, context, vmad, world, &stages, &deferred)
}

/// [`evaluate_value_guard_free`] for a guard: the stage and objective resources
/// are read-locked together, in the `TypeId` order the effect applier's
/// `resource_2_mut` takes them.
pub(crate) fn evaluate_guard_guard_free(
    world: &World,
    guard: &FragmentGuard,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
) -> Option<bool> {
    let deferred = DeferredFragmentEffects::new(world);
    // #6101 - one TypeId-sorted acquisition, like every `resource_2_mut` site
    // that takes this pair; two `try_resource` calls in code order closed an
    // ABBA cycle in whichever build hashed Objective below Stage.
    let (stages, objectives) = world.try_resource_2::<QuestStageState, QuestObjectiveState>()?;
    evaluate_guard(guard, context, vmad, world, &stages, &objectives, &deferred)
}

/// ObScript `ModAV` / `ForceAV` (M47.3): shift `actor`'s `avif` by `delta` in the
/// permanent layer ("permanently alters the actor value"), creating `ActorValues`
/// if it has none, and queue the death a Health write to zero or below owes.
pub(crate) fn mod_actor_value_permanent(world: &World, actor: EntityId, avif: u32, delta: f32) {
    {
        let Some(mut values) = world.query_mut::<byroredux_core::ecs::components::ActorValues>() else {
            return;
        };
        if values.get_mut(actor).is_none() {
            values.insert(actor, byroredux_core::ecs::components::ActorValues::new());
        }
        values.get_mut(actor).expect("present or inserted").mod_permanent(avif, delta);
    }
    queue_health_death(world, actor);
}

/// ObScript `ResetHealth` (M47.3): take every damage point off `avifs` ("restores
/// the actor's health and cures crippling damage").
pub(crate) fn clear_actor_value_damage(world: &World, actor: EntityId, avifs: &[u32]) {
    let Some(mut values) = world.query_mut::<byroredux_core::ecs::components::ActorValues>() else {
        return;
    };
    let Some(values) = values.get_mut(actor) else {
        return;
    };
    for &avif in avifs {
        values.restore(avif, f32::MAX);
    }
}

/// #5862 — a script's actor-value write may have taken `actor`'s Health to
/// zero or below. Nothing else sweeps actors at Health <= 0, so queue the
/// same death a scripted `Kill` does: `scripted_kill_system` spares an
/// essential / protected actor (bleedout) and otherwise marks it `Dead`,
/// raises KILL and queues the teardown. Idempotent for an actor that is
/// already dead or still above zero; the caller holds no `ActorValues` guard.
pub(super) fn queue_health_death(world: &World, actor: EntityId) {
    queue_health_death_by(world, actor, || None);
}

/// [`queue_health_death`] crediting a killer, resolved only when the death is
/// queued (#6232: a magic effect's caster).
pub(crate) fn queue_health_death_by(
    world: &World,
    actor: EntityId,
    killer: impl FnOnce() -> Option<EntityId>,
) {
    use byroredux_core::ecs::components::{ActorValues, ActorVitals, Dead};
    if world.get::<Dead>(actor).is_some() {
        return;
    }
    let Some(health) = world.get::<ActorVitals>(actor).map(|vitals| vitals.health) else {
        return;
    };
    let lethal = world
        .get::<ActorValues>(actor)
        .is_some_and(|values| values.current(health) <= 0.0);
    if lethal {
        if let Some(mut pending) = world.try_resource_mut::<crate::story_manager::PendingScriptKills>() {
            pending.0.push((actor, killer(), false));
        }
    }
}

/// The `LCTN` a location-typed [`ObjectRef`] names: a direct property's
/// FormID, or the location bound to the location alias the property points
/// at.
pub(super) fn resolve_location(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    location: &ObjectRef,
) -> Option<u32> {
    let value = vmad?
        .scripts
        .iter()
        .find_map(|script| script.property(location.property_name()))
        .map(|property| &property.value)?;
    match value {
        PropertyValue::Object { form_id, alias: -1 } => Some(*form_id),
        PropertyValue::Object { form_id, alias } if *alias >= 0 => {
            let quest = if *form_id != 0 {
                QuestFormId(*form_id)
            } else {
                context
            };
            world
                .try_resource::<crate::LocationAliasBindings>()?
                .resolve(quest, i32::from(*alias))
        }
        _ => None,
    }
}

/// The actor a guard reads, resolved like an effect's receiver.
pub(super) fn guard_actor(
    actor: &ActorRef,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    deferred: &DeferredFragmentEffects,
) -> Option<EntityId> {
    resolve_actor(
        vmad,
        world,
        context,
        actor,
        &deferred.scene_actor_bindings,
        deferred.speaker,
    )
}
