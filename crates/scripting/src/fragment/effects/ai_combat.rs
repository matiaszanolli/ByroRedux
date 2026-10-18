//! The AI / combat effect applier. (#5468, split from `effects.rs`)

use super::*;

/// `SetEnemy` / `StartCombat` / `StopCombat` / `EvaluatePackage` — faction
/// relations, `AiCombatState` and package re-evaluation.
///
/// #4340 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays
/// named in `apply_effect`'s nested-lock residual list, which the #3949
/// scan now reads across these helpers too.
pub(super) fn apply_ai_combat_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::SetEnemy {
            faction,
            other_faction,
            self_neutral,
            other_neutral,
        } => {
            use byroredux_sdk::relationships::CombatReaction;
            // #4318 — a `true` flag makes that direction neutral, not hostile.
            let reaction = |neutral: bool| {
                if neutral {
                    CombatReaction::Neutral
                } else {
                    CombatReaction::Enemy
                }
            };
            let faction = resolve_property_form_id(vmad, faction.property_name())?;
            let other_faction = resolve_property_form_id(vmad, other_faction.property_name())?;
            deferred.faction_relations.push((
                faction,
                other_faction,
                reaction(*self_neutral),
                reaction(*other_neutral),
            ));
            None
        }
        Effect::SetAlly {
            faction,
            other_faction,
            self_friend,
            other_friend,
        } => {
            use byroredux_sdk::relationships::CombatReaction;
            // CK *SetAlly - Faction*: a `true` flag makes that direction a
            // friend, `false` an ally.
            let reaction = |friend: bool| {
                if friend {
                    CombatReaction::Friend
                } else {
                    CombatReaction::Ally
                }
            };
            let faction = resolve_property_form_id(vmad, faction.property_name())?;
            let other_faction = resolve_property_form_id(vmad, other_faction.property_name())?;
            deferred.faction_relations.push((
                faction,
                other_faction,
                reaction(*self_friend),
                reaction(*other_friend),
            ));
            None
        }
        Effect::StartCombat { actor, target } => {
            // #5298 — the combatant must be an NPC; the target may be the
            // player (`Attacker.StartCombat(PlayerRef)` is the common shape).
            let actor =
                resolve_npc_actor(vmad, world, context, actor, &deferred.scene_actor_bindings, deferred.speaker)?;
            // M47.2 — a ghost "will not enter combat" (CS / GECK *SetGhost*).
            if crate::is_ghost(world, actor) {
                log::debug!("fragment StartCombat skipped: the actor is a ghost");
                return None;
            }
            let target =
                resolve_actor(vmad, world, context, target, &deferred.scene_actor_bindings, deferred.speaker)?;
            if let Some(mut states) = world.query_mut::<crate::AiCombatState>() {
                states.insert(
                    actor,
                    crate::AiCombatState {
                        target,
                        attack_cooldown_remaining: 0.0,
                    },
                );
            }
            // #5046 — a scripted combat is never dropped for lost contact
            // (#4816). The ambient pass keys "this fight is mine" on its
            // marker's target matching the current one, so re-issuing combat
            // against the target it already chose must clear the marker, or
            // the fight stays on the ambient grace timer and auto-drops.
            if let Some(mut markers) = world.query_mut::<crate::AmbientEngagement>() {
                markers.remove(actor);
            }
            None
        }
        Effect::StopCombat { actor } => {
            let actor =
                resolve_npc_actor(vmad, world, context, actor, &deferred.scene_actor_bindings, deferred.speaker)?;
            if let Some(mut states) = world.query_mut::<crate::AiCombatState>() {
                states.remove(actor);
            }
            None
        }
        Effect::SetCrimeFaction { actor, faction } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings, deferred.speaker)?;
            let reference = if crate::papyrus_demo::PapyrusPlayerEntity::get(world).is_some_and(|player| player == actor)
            {
                0x14
            } else {
                crate::condition::entity_global_form_id(world, actor)?
            };
            let faction = match faction {
                Some(faction) => resolve_property_form_id(vmad, faction.property_name())?,
                None => 0,
            };
            if faction != 0 {
                // "If the actor does not currently belong to the crime faction, it
                // will be added to that faction."
                deferred.faction_changes.push((actor, faction, crate::FactionChange::Add));
            }
            deferred.crime_faction_changes.push((reference, faction));
            None
        }
        Effect::StopCombatAlarm { actor } => {
            let target = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings, deferred.speaker)?;
            if let Some(mut states) = world.query_mut::<crate::AiCombatState>() {
                let against: Vec<EntityId> = states
                    .iter()
                    .filter(|(_, state)| state.target == target)
                    .map(|(fighter, _)| fighter)
                    .collect();
                for fighter in against {
                    states.remove(fighter);
                }
            }
            None
        }
        Effect::SetGhost { actor, ghost } => {
            let bindings = &deferred.scene_actor_bindings;
            let actor = resolve_actor(vmad, world, context, actor, bindings, deferred.speaker)?;
            if !crate::update_actor_control(world, actor, |state| state.ghost = *ghost) {
                log::debug!("fragment SetGhost skipped: actor-control storage is unavailable");
                return None;
            }
            // A ghost "will not enter combat".
            if *ghost {
                if let Some(mut states) = world.query_mut::<crate::AiCombatState>() {
                    states.remove(actor);
                }
            }
            None
        }
        Effect::SetUnconscious { actor, unconscious } => {
            let actor =
                resolve_npc_actor(vmad, world, context, actor, &deferred.scene_actor_bindings, deferred.speaker)?;
            if !crate::update_actor_control(world, actor, |state| {
                state.set_unconscious(*unconscious)
            }) {
                log::debug!(
                    "fragment SetUnconscious skipped: actor-control storage is unavailable"
                );
                return None;
            }
            // An unconscious actor cannot fight (#5017). Its ambient package
            // is suspended by the engine's AI pass; waking leaves both to
            // re-derive (faction hostility, package selection).
            if *unconscious {
                if let Some(mut states) = world.query_mut::<crate::AiCombatState>() {
                    states.remove(actor);
                }
            }
            None
        }
        Effect::EvaluatePackage { actor } => {
            let bindings = &deferred.scene_actor_bindings;
            let actor = resolve_actor(vmad, world, context, actor, bindings, deferred.speaker)?;
            crate::package::request_package_evaluation(world, actor);
            None
        }
        _ => unreachable!("apply_ai_combat_effect received a non-ai/combat effect"),
    }
}
