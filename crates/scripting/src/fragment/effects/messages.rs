//! Story events, map markers, messages and the barter menu. (#6142, split from `apply_effect`)

use super::*;

/// `SendStoryEvent`, `AddToMap`, `ShowMessage`, `ShowHelpMessage`, `Notification` and `ShowBarterMenu`.
///
/// #6142 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays named in
/// `apply_effect`'s nested-lock residual list, which the #3949 scan reads
/// across these helpers too.
pub(super) fn apply_messages_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::SendStoryEvent {
            keyword,
            location,
            references,
            values,
        } => {
            let keyword = resolve_property_form_id(vmad, keyword.property_name())?;
            log::debug!(
                "fragment SendStoryEvent: quest {:08X} raises SCPT keyword {keyword:08X}",
                context.0
            );
            // A location property, or a location alias's bound location.
            let location = match location {
                None => None,
                Some(location) => resolve_location(vmad, world, context, location),
            };
            let [first, second] = references
                .clone()
                .map(|actor| actor.and_then(|actor| guard_actor(&actor, context, vmad, world, deferred)));
            // `R1` holds an entity: the first reference, else the player
            // (flagged absent).
            let carrier = first.or_else(|| {
                crate::papyrus_demo::PapyrusPlayerEntity::get(world)
            })?;
            deferred.story_events.push(
                crate::story_manager::StoryEvent {
                    mnemonic: *b"SCPT",
                    reference_1: carrier,
                    reference_2: second,
                    location_1: location,
                    location_2: None,
                    scripted: Some(crate::story_manager::ScriptedStoryEvent {
                        keyword,
                        values: *values,
                        has_reference_1: first.is_some(),
                    }),
                },
            );
            None
        }
        Effect::AddToMap { marker, fast_travel } => {
            let Some(reference) = object_form_id(vmad, world, context, marker, deferred) else {
                log::debug!("fragment AddToMap skipped: the marker does not resolve");
                return None;
            };
            deferred.map_markers.push((reference, *fast_travel));
            None
        }
        Effect::ShowMessage { message, args } => {
            let form_id = resolve_property_form_id(vmad, message.property_name())?;
            deferred.messages.push(PendingMessage::Show(form_id, args.clone()));
            None
        }
        Effect::ShowHelpMessage { message } => {
            let form_id = resolve_property_form_id(vmad, message.property_name())?;
            deferred.messages.push(PendingMessage::Show(form_id, Vec::new()));
            None
        }
        Effect::Notification { text } => {
            deferred.messages.push(PendingMessage::Notify(text.clone()));
            None
        }
        Effect::ShowBarterMenu { vendor } => {
            let vendor = guard_actor(vendor, context, vmad, world, deferred)?;
            deferred.barter = Some(vendor);
            None
        }
        _ => unreachable!("`apply_effect` only routes messages effects here"),
    }
}
