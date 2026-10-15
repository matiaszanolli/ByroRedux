//! The player-control effect applier. (#5468, split from `effects.rs`)

use super::*;

/// Player control, chargen and presentation: the `PlayerControlState` /
/// `ActorControlState` arms plus the ones that only queue a deferred
/// presentation effect.
///
/// #4340 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays
/// named in `apply_effect`'s nested-lock residual list, which the #3949
/// scan now reads across these helpers too.
pub(super) fn apply_player_control_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::SetPlayerRestrained { restrained } => {
            let Some(player) = crate::papyrus_demo::PapyrusPlayerEntity::get(world)
            else {
                log::debug!("fragment SetRestrained skipped: player entity is unavailable");
                return None;
            };
            // #5017 — through the setter, so restraining clears unconscious.
            if !crate::update_actor_control(world, player, |state| {
                state.set_restrained(*restrained)
            }) {
                log::debug!("fragment SetRestrained skipped: actor-control storage is unavailable");
            }
            None
        }
        Effect::SetPlayerControls { enabled, selection } => {
            if let Some(mut controls) = world.try_resource_mut::<crate::PlayerControlState>() {
                controls.apply_selection(*enabled, *selection);
            }
            None
        }
        Effect::SetPlayerAiDriven { ai_driven } => {
            if let Some(mut controls) = world.try_resource_mut::<crate::PlayerControlState>() {
                controls.ai_driven = *ai_driven;
            }
            None
        }
        Effect::SetHudCartMode { cart_mode } => {
            if let Some(mut controls) = world.try_resource_mut::<crate::PlayerControlState>() {
                controls.hud_cart_mode = *cart_mode;
            }
            None
        }
        Effect::SetSittingRotation { degrees } => {
            deferred.cinematic_presentation.push(
                DeferredCinematicPresentationEffect::SetSittingRotation(*degrees),
            );
            None
        }
        Effect::SetInChargen {
            disable_saving,
            disable_waiting,
            show_controls_disabled_message,
        } => {
            deferred.cinematic_presentation.push(
                DeferredCinematicPresentationEffect::SetInChargen {
                    disable_saving: *disable_saving,
                    disable_waiting: *disable_waiting,
                    show_controls_disabled_message: *show_controls_disabled_message,
                },
            );
            None
        }
        Effect::ShowRaceMenu => {
            deferred
                .cinematic_presentation
                .push(DeferredCinematicPresentationEffect::ShowRaceMenu);
            None
        }
        Effect::RequestSave { auto } => {
            deferred
                .cinematic_presentation
                .push(DeferredCinematicPresentationEffect::RequestSave { auto: *auto });
            None
        }
        Effect::RegisterPlayerAnimationEvent { event } => {
            let image_space_modifiers = match event {
                crate::CinematicAnimationEvent::PlayImod => {
                    ["PlayerAlduinIMOD", "CGDragonAttackBlurLong"]
                        .into_iter()
                        .filter_map(|property| resolve_property_form_id(vmad, property))
                        .map(|form_id| crate::ImageSpaceModifierApplication {
                            form_id,
                            strength: 1.0,
                        })
                        .collect()
                }
                crate::CinematicAnimationEvent::IdleFurnitureExit
                | crate::CinematicAnimationEvent::ExitCartEnd => Vec::new(),
            };
            deferred.cinematic_presentation.push(
                DeferredCinematicPresentationEffect::RegisterPlayerAnimationEvent {
                    event: *event,
                    quest: context,
                    image_space_modifiers,
                },
            );
            None
        }
        _ => unreachable!("apply_player_control_effect received a non-player-control effect"),
    }
}
