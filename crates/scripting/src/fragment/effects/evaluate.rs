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
        ValueExpr::ItemCount(container, item) => {
            let container = guard_actor(container, context, vmad, world, deferred)?;
            let item = resolve_property_form_id(vmad, item.property_name())?;
            let count: u32 = world
                .get::<Inventory>(container)
                .map(|inventory| {
                    inventory
                        .items
                        .iter()
                        .filter(|stack| stack.base_form_id == item)
                        .map(|stack| stack.count)
                        .sum()
                })
                .unwrap_or(0);
            count as f32
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
    let quest = |via: &QuestRef| resolve_quest_logged(via, context, vmad);
    Some(match guard {
        FragmentGuard::StageDone { quest: via, stage, done } => {
            stages.get_stage_done(quest(via)?, *stage) == *done
        }
        FragmentGuard::QuestRunning { quest: via, running } => {
            stages.is_running(quest(via)?) == *running
        }
        FragmentGuard::Stage {
            quest: via,
            comparison,
            value,
        } => comparison.apply(
            evaluate_value(&ValueExpr::Stage(via.clone()), context, vmad, world, stages, deferred)?,
            *value,
        ),
        FragmentGuard::ObjectiveDisplayed {
            quest: via,
            objective,
            expected,
        } => objectives.get(quest(via)?, *objective).displayed == *expected,
        FragmentGuard::ObjectiveCompleted {
            quest: via,
            objective,
            expected,
        } => objectives.get(quest(via)?, *objective).completed == *expected,
        FragmentGuard::Global {
            global,
            comparison,
            value,
        } => comparison.apply(
            evaluate_value(&ValueExpr::Global(global.clone()), context, vmad, world, stages, deferred)?,
            *value,
        ),
        FragmentGuard::QuestVariable {
            quest: via,
            variable,
            comparison,
            value,
        } => comparison.apply(
            evaluate_value(
                &ValueExpr::QuestVariable(via.clone(), variable.clone()),
                context,
                vmad,
                world,
                stages,
                deferred,
            )?,
            *value,
        ),
        FragmentGuard::ActorDead { actor, dead } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            world.has::<byroredux_core::ecs::components::Dead>(actor) == *dead
        }
        FragmentGuard::ActorBleedingOut { actor, expected } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            crate::bleedout::is_bleeding_out(world, actor) == *expected
        }
        FragmentGuard::ActorInCombat { actor, expected } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            crate::is_in_combat(world, actor) == *expected
        }
        FragmentGuard::ActorFollower { actor, flag, expected } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let form = crate::condition::entity_global_form_id(world, actor)?;
            // A write earlier in the same run is visible.
            let mut value = world
                .try_resource::<crate::FollowerState>()
                .is_some_and(|state| state.read(form, *flag));
            for (changed, change) in &deferred.follower_changes {
                if *changed != form {
                    continue;
                }
                match (change, flag) {
                    (FollowerChange::Teammate { teammate, .. }, crate::FollowerFlag::Teammate) => value = *teammate,
                    (FollowerChange::DoingFavor(doing), crate::FollowerFlag::DoingFavor) => value = *doing,
                    _ => {}
                }
            }
            value == *expected
        }
        FragmentGuard::ActorEssential { actor, expected } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            crate::bleedout::is_essential(world, actor) == *expected
        }
        FragmentGuard::ReferenceEnabled { object, expected } => {
            let form_id = object_form_id(vmad, world, context, object, deferred)?;
            // An `Enable()` / `Disable()` earlier in the same fragment is
            // visible.
            let enabled = match deferred
                .reference_enable_changes
                .iter()
                .rev()
                .find(|(changed, _)| *changed == form_id)
            {
                Some(&(_, enabled)) => enabled,
                None => super::reference_enabled(world, form_id),
            };
            enabled == *expected
        }
        FragmentGuard::InFaction {
            actor,
            faction,
            expected,
        } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let faction = resolve_property_form_id(vmad, faction.property_name())?;
            let member = crate::condition::evaluate_function(
                crate::condition::ConditionFunction::GetInFaction,
                &byroredux_plugin::esm::records::condition::Condition {
                    param_1: faction,
                    ..Default::default()
                },
                actor,
                world,
            ) == 1.0;
            member == *expected
        }
        FragmentGuard::HasLocRefType {
            actor,
            ref_type,
            expected,
        } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let ref_type = resolve_property_form_id(vmad, ref_type.property_name())?;
            let reference = crate::condition::entity_global_form_id(world, actor).unwrap_or(0);
            let has = crate::has_loc_ref_type(world, actor, reference, ref_type);
            has == *expected
        }
        FragmentGuard::Loaded3D { object, expected } => {
            // A reference with no live entity is not loaded.
            let form_id = object_form_id(vmad, world, context, object, deferred)?;
            let loaded = crate::condition::resolve_entity_by_global_form_id(world, form_id)
                .is_some_and(|entity| {
                    crate::condition::evaluate_function(
                        crate::condition::ConditionFunction::HasLoaded3D,
                        &byroredux_plugin::esm::records::condition::Condition::default(),
                        entity,
                        world,
                    ) == 1.0
                });
            loaded == *expected
        }
        FragmentGuard::RelationshipRank {
            actor,
            other,
            comparison,
            value,
        } => comparison.apply(
            evaluate_value(
                &ValueExpr::RelationshipRank(actor.clone(), other.clone()),
                context,
                vmad,
                world,
                stages,
                deferred,
            )?,
            *value,
        ),
        FragmentGuard::Distance {
            from,
            to,
            comparison,
            value,
        } => comparison.apply(
            evaluate_value(
                &ValueExpr::Distance(from.clone(), to.clone()),
                context,
                vmad,
                world,
                stages,
                deferred,
            )?,
            *value,
        ),
        FragmentGuard::Compare {
            left,
            comparison,
            right,
        } => {
            let left = evaluate_value(left, context, vmad, world, stages, deferred)?;
            let right = evaluate_value(right, context, vmad, world, stages, deferred)?;
            comparison.apply(left, right)
        }
        FragmentGuard::HasPerk {
            actor,
            perk,
            expected,
        } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let perk = resolve_property_form_id(vmad, perk.property_name())?;
            let owned = world
                .get::<byroredux_core::character::Perks>(actor)
                .is_some_and(|perks| perks.rank(perk) > 0);
            owned == *expected
        }
        FragmentGuard::ItemCount {
            container,
            item,
            comparison,
            value,
        } => comparison.apply(
            evaluate_value(
                &ValueExpr::ItemCount(container.clone(), item.clone()),
                context,
                vmad,
                world,
                stages,
                deferred,
            )?,
            *value,
        ),
        FragmentGuard::QuestCompleted { quest: via, completed } => {
            stages.is_completed(quest(via)?) == *completed
        }
        FragmentGuard::ScenePlaying { scene, playing } => {
            let scene = resolve_property_form_id(vmad, scene.property_name())?;
            deferred.running_scenes.contains(&scene) == *playing
        }
        FragmentGuard::SameReference {
            left,
            right,
            expected,
        } => {
            let left = reference_identity(left, context, vmad, world, deferred);
            let right = right
                .as_ref()
                .and_then(|right| reference_identity(right, context, vmad, world, deferred));
            (left == right) == *expected
        }
        FragmentGuard::HasKeyword {
            object,
            keyword,
            expected,
        } => {
            let object = guard_actor(object, context, vmad, world, deferred)?;
            let keyword = resolve_property_form_id(vmad, keyword.property_name())?;
            // This fragment's own pending change wins, as it will once applied.
            let pending = deferred
                .keyword_changes
                .iter()
                .rev()
                .find(|(changed, k, _)| *changed == object && *k == keyword)
                .map(|(_, _, change)| *change);
            let has = match pending {
                Some(crate::keywords::KeywordChange::Add) => true,
                Some(crate::keywords::KeywordChange::Remove) => false,
                Some(crate::keywords::KeywordChange::Reset) | None => {
                    crate::keywords::has_keyword(world, object, keyword)
                }
            };
            has == *expected
        }
        FragmentGuard::LocationKeyword {
            actor,
            keyword,
            expected,
        } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let keyword = resolve_property_form_id(vmad, keyword.property_name())?;
            let location = crate::location::current_location(world, actor)?;
            let has = world
                .try_resource::<crate::location::LocationTable>()
                .is_some_and(|table| table.has_keyword(location, keyword));
            has == *expected
        }
        FragmentGuard::MapMarkerVisible { marker, expected } => {
            let marker = object_form_id(vmad, world, context, marker, deferred)?;
            let pending = deferred.map_markers.iter().any(|(added, _)| *added == marker);
            (pending || crate::map_marker_visible(world, marker)) == *expected
        }
        FragmentGuard::MapMarkerFastTravel { marker, expected } => {
            let marker = object_form_id(vmad, world, context, marker, deferred)?;
            let pending = deferred
                .map_markers
                .iter()
                .any(|(added, travel)| *added == marker && *travel);
            (pending || crate::map_marker_fast_travel(world, marker)) == *expected
        }
        FragmentGuard::LocationCleared { location, expected } => {
            let location = resolve_location(vmad, world, context, location)?;
            // A `SetCleared` earlier in the same fragment is visible.
            let cleared = match deferred.location_clears.iter().rev().find(|(set, _)| *set == location) {
                Some(&(_, cleared)) => cleared,
                None => crate::location_cleared(world, location),
            };
            cleared == *expected
        }
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

/// #5862 — a script's actor-value write may have taken `actor`'s Health to
/// zero or below. Nothing else sweeps actors at Health <= 0, so queue the
/// same death a scripted `Kill` does: `scripted_kill_system` spares an
/// essential / protected actor (bleedout) and otherwise marks it `Dead`,
/// raises KILL and queues the teardown. Idempotent for an actor that is
/// already dead or still above zero; the caller holds no `ActorValues` guard.
pub(super) fn queue_health_death(world: &World, actor: EntityId) {
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
            pending.0.push((actor, None, false));
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
