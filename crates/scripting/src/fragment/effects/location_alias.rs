//! Location state and alias forcing. (#6142, split from `apply_effect`)

use super::*;

/// `SetLocationCleared`, `ForceLocationTo`, `ForceAlias` and `SetLocationKeywordData`.
///
/// #6142 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays named in
/// `apply_effect`'s nested-lock residual list, which the #3949 scan reads
/// across these helpers too.
pub(super) fn apply_location_alias_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::SetLocationCleared { location, cleared } => {
            let Some(location) = resolve_location(vmad, world, context, location) else {
                log::debug!("fragment SetCleared skipped: the location does not resolve");
                return None;
            };
            deferred.location_clears.push((location, *cleared));
            None
        }
        Effect::ForceLocationTo { alias, location } => {
            let name = alias.property_name();
            let Some((quest, alias_id)) = alias_property(vmad, context, name) else {
                log::debug!("fragment ForceLocationTo skipped: '{name}' is not an alias property");
                return None;
            };
            let location = resolve_property_form_id(vmad, location.property_name())?;
            deferred
                .alias_overrides
                .push((quest, alias_id, crate::AliasOverride::ForcedLocation(location)));
            deferred.scene_actor_bindings_dirty = true;
            None
        }
        Effect::ForceAlias { alias, target } => {
            let name = alias.property_name();
            let Some((quest, alias_id)) = alias_property(vmad, context, name) else {
                log::debug!("fragment ForceRefTo/Clear skipped: '{name}' is not an alias property");
                return None;
            };
            let value = match target {
                None => crate::AliasOverride::Cleared,
                Some(target) => {
                    crate::AliasOverride::Forced(actor_reference_form_id(target, context, vmad, world, deferred)?)
                }
            };
            deferred.alias_overrides.push((quest, alias_id, value));
            deferred.scene_actor_bindings_dirty = true;
            None
        }
        Effect::SetLocationKeywordData { location, keyword, value } => {
            let location = resolve_property_form_id(vmad, location.property_name())?;
            let keyword = resolve_property_form_id(vmad, keyword.property_name())?;
            let value = evaluate_value(value, context, vmad, world, stages, deferred)?;
            deferred.location_keyword_changes.push((location, keyword, value));
            None
        }
        _ => unreachable!("`apply_effect` only routes location alias effects here"),
    }
}
