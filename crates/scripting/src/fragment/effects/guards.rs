//! Guard evaluation by family: the arms of `evaluate_guard`. (#6142, split from `evaluate.rs`)

use super::*;

/// Quest, objective, global and quest-variable guards. Arms moved verbatim out of [`evaluate_guard`] (#6142); every
/// lock they take stays covered by the #3949 scan, which reads this helper too.
pub(super) fn evaluate_quest_guard(
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
        FragmentGuard::QuestCompleted { quest: via, completed } => {
            stages.is_completed(quest(via)?) == *completed
        }
        _ => unreachable!("`evaluate_guard` only routes quest guards here"),
    })
}

/// Actor-state guards. Arms moved verbatim out of [`evaluate_guard`] (#6142); every
/// lock they take stays covered by the #3949 scan, which reads this helper too.
pub(super) fn evaluate_actor_guard(
    guard: &FragmentGuard,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &QuestStageState,
    deferred: &DeferredFragmentEffects,
) -> Option<bool> {
    Some(match guard {
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
        _ => unreachable!("`evaluate_guard` only routes actor guards here"),
    })
}

/// Reference, location, scene and numeric-comparison guards. Arms moved verbatim out of [`evaluate_guard`] (#6142); every
/// lock they take stays covered by the #3949 scan, which reads this helper too.
pub(super) fn evaluate_world_guard(
    guard: &FragmentGuard,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &QuestStageState,
    deferred: &DeferredFragmentEffects,
) -> Option<bool> {
    Some(match guard {
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
        _ => unreachable!("`evaluate_guard` only routes world guards here"),
    })
}
