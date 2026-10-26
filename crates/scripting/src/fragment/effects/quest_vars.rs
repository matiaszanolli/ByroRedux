//! Global, quest-script, local and reference variable writes. (#6142, split from `apply_effect`)

use super::*;

/// `SetGlobalValue`, `SetQuestVariable`, `CopyQuestVariable`, `ModQuestVariable`, `SetLocal` and `SetReferenceVariable`.
///
/// #6142 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays named in
/// `apply_effect`'s nested-lock residual list, which the #3949 scan reads
/// across these helpers too.
pub(super) fn apply_quest_vars_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::SetGlobalValue { global, value } => {
            // Read first: the value may read `Globals` itself (`G.Mod(x)`).
            let value = evaluate_value(value, context, vmad, world, stages, deferred)?;
            let form_id = resolve_property_form_id(vmad, global.property_name())?;
            if let Some(mut globals) = world.try_resource_mut::<crate::Globals>() {
                globals.set(form_id, value);
            } else {
                log::debug!(
                    "fragment SetValue skipped: Globals resource is unavailable for {form_id:08X}"
                );
            }
            None
        }
        Effect::SetReferenceVariable {
            object,
            variable,
            value,
        } => {
            let Some(entity) =
                resolve_object(vmad, world, context, object, &deferred.scene_actor_bindings)
            else {
                log::debug!("fragment reference variable write skipped: the reference is not loaded");
                return None;
            };
            deferred
                .reference_variables
                .push((entity, variable.clone(), *value));
            None
        }
        Effect::SetLocal { local, value } => {
            let value = evaluate_value(value, context, vmad, world, stages, deferred)?;
            deferred.locals.insert(local.clone(), value);
            None
        }
        Effect::SetQuestVariable {
            quest,
            variable,
            value,
        } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            deferred.quest_variables.push((
                quest,
                crate::QuestScriptVariables::property_key(variable),
                *value,
            ));
            None
        }
        Effect::CopyQuestVariable { quest, from, to } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let from = crate::QuestScriptVariables::property_key(from);
            let to = crate::QuestScriptVariables::property_key(to);
            // The same pending-then-written chain `ModQuestVariable` reads.
            let value = deferred
                .quest_variables
                .iter()
                .rev()
                .find(|(q, k, _)| *q == quest && *k == from)
                .map(|(_, _, value)| *value)
                .or_else(|| crate::quest_variable(world, quest, from))?;
            deferred.quest_variables.push((quest, to, value));
            None
        }
        Effect::ModQuestVariable {
            quest,
            variable,
            delta,
        } => {
            // Read through the same pending-then-written-then-authored chain
            // a guard uses, so an earlier write in this fragment counts.
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let key = crate::QuestScriptVariables::property_key(variable);
            let current = deferred
                .quest_variables
                .iter()
                .rev()
                .find(|(q, k, _)| *q == quest && *k == key)
                .map(|(_, _, value)| *value)
                .or_else(|| crate::quest_variable(world, quest, key))
                .unwrap_or(0.0);
            deferred.quest_variables.push((quest, key, current + delta));
            None
        }
        _ => unreachable!("`apply_effect` only routes quest vars effects here"),
    }
}
