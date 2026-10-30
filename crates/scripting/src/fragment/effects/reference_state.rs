//! Reference, actor-record and relationship state changes. (#6142, split from `apply_effect`)

use super::*;

/// `SetLinkedRef`, `SetLocRefType`, `BlockActivation`, `DeleteWhenAble`, `ChangeKeyword`, `SetBaseProtection`, `Kill` and `SetRelationshipRank`.
///
/// #6142 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays named in
/// `apply_effect`'s nested-lock residual list, which the #3949 scan reads
/// across these helpers too.
pub(super) fn apply_reference_state_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::DeleteWhenAble { object } => {
            let form_id = object_form_id(vmad, world, context, object, deferred)?;
            deferred.deletions.push(form_id);
            None
        }
        Effect::ChangeKeyword {
            object,
            keyword,
            change,
        } => {
            let object = guard_actor(object, context, vmad, world, deferred)?;
            let keyword = resolve_property_form_id(vmad, keyword.property_name())?;
            deferred.keyword_changes.push((object, keyword, *change));
            None
        }
        Effect::SetLinkedRef {
            object,
            target,
            keyword,
        } => {
            let source = object_form_id(vmad, world, context, object, deferred)?;
            let target = match target {
                Some(target) => object_form_id(vmad, world, context, target, deferred)?,
                None => 0,
            };
            let keyword = match keyword {
                Some(keyword) => resolve_property_form_id(vmad, keyword.property_name())?,
                None => 0,
            };
            deferred.linked_ref_changes.push((source, keyword, target));
            None
        }
        Effect::SetLocRefType { object, ref_type } => {
            let reference = object_form_id(vmad, world, context, object, deferred)?;
            let ref_type = match ref_type {
                Some(ref_type) => resolve_property_form_id(vmad, ref_type.property_name())?,
                None => 0,
            };
            deferred.loc_ref_type_changes.push((reference, ref_type));
            None
        }
        Effect::SetRelationshipRank { actor, other, rank } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let other = guard_actor(other, context, vmad, world, deferred)?;
            let (Some(actor_base), Some(other_base)) = (
                crate::relationships::actor_base(world, actor),
                crate::relationships::actor_base(world, other),
            ) else {
                log::debug!("fragment SetRelationshipRank skipped: an actor has no base");
                return None;
            };
            let pair = crate::relationships::pair(actor_base, other_base);
            // A change earlier in the same fragment is the "old" rank here.
            let old = deferred
                .relationship_changes
                .iter()
                .rev()
                .find(|(changed, _)| *changed == pair)
                .map_or_else(|| crate::relationships::relationship_rank(world, actor, other), |&(_, rank)| rank);
            deferred.relationship_changes.push((pair, *rank));
            crate::relationships::raise_relationship_change(world, actor, other, old, *rank);
            None
        }
        Effect::Kill { actor, killer, silent } => {
            let victim = guard_actor(actor, context, vmad, world, deferred)?;
            let killer = killer
                .as_ref()
                .and_then(|killer| guard_actor(killer, context, vmad, world, deferred));
            deferred.kills.push((victim, killer, *silent));
            None
        }
        Effect::SetBaseProtection {
            base,
            protected,
            value,
        } => {
            let base = match base {
                crate::translate::effects::ActorBaseRef::OfActor(actor) => {
                    let actor = guard_actor(actor, context, vmad, world, deferred)?;
                    world
                        .get::<crate::scene::SceneAliasCandidate>(actor)
                        .map(|candidate| candidate.base_form_id)
                }
                crate::translate::effects::ActorBaseRef::Form(form) => {
                    resolve_property_form_id(vmad, form.property_name())
                }
            };
            let Some(base) = base else {
                log::debug!("fragment SetEssential / SetProtected skipped: no actor base");
                return None;
            };
            deferred.base_protection.push((base, *protected, *value));
            None
        }
        Effect::BlockActivation { object, blocked } => {
            let Some(reference) = object_form_id(vmad, world, context, object, deferred) else {
                log::debug!("fragment BlockActivation skipped: the reference does not resolve");
                return None;
            };
            deferred.activation_blocks.push((reference, *blocked));
            None
        }
        _ => unreachable!("`apply_effect` only routes reference state effects here"),
    }
}
