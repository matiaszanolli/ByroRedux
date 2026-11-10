//! PERK-HOOK-007 / PERK-HOOK-012: the player's activation and the *Activate* entries of their perks.
//!
//! When the player activates a reference ([`crate::interaction`]), [`compose_player_activation`] composes the
//! activate entries of the player's perks first
//! ([`byroredux_scripting::perk_entry_points::compose_activate_choices`]; only the player's activations, CK
//! *Perk Fragments*: "NPCs won't run activate entry point perk entries"):
//!
//! * A passing *Run Immediately* entry runs at once: "This is run in addition to the default activation", or
//!   with *Replace Default*, "in place of the default activation" (CK *Perk Entry Point*).
//! * A passing choice holds the activation on the engine-native message-box page (§10 Q5). Button 0 is the
//!   default activation, under the verb the prompt showed. Each later button is one entry, under its `EPF2`
//!   label. Picking the default runs the activation as if no perk were involved. Picking an entry runs it
//!   ([`byroredux_scripting::perk_entry_points::run_activate_choice`]) instead of the activation.
//!
//! An entry that runs in place of the default activation replaces the whole activation, so the reference
//! receives no `OnActivate` either. The CK offers the choice "in addition to the normal activation prompt",
//! so picking it is not the normal activation. No source states whether the original engine sends
//! `OnActivate` in that case. The page lists the choices only while the default activation is still to run:
//! a *Replace Default* entry that already ran in its place leaves no activation for the choices to stand
//! beside, and the sources do not say what the prompt shows then.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Resource, World};
use byroredux_scripting::perk_entry_points::{compose_activate_choices, run_activate_choice, ActivateChoiceOffer};
use byroredux_scripting::{MessageAnswer, PendingScriptMessages, ScriptMessage};

use crate::interaction::InteractionTarget;

/// The activation held by the activate-choice box: the target, the entry behind each button after the
/// first, and the answer once the player gave it. `NOT_SAVED_BY_DESIGN`, like the box it waits on (a load
/// discards both).
#[derive(Default)]
pub(crate) struct PendingActivateChoice {
    pub(crate) held: Option<(InteractionTarget, Vec<ActivateChoiceOffer>)>,
    /// The button picked (`None` inside: dismissed with no answer). Resolved by [`resolve_answered_activation`]
    /// on the next run of the interaction system, so an activation it releases is raised in `Stage::Update`
    /// before every `OnActivate` consumer, as a fresh key press is, whichever stage the answer came from
    /// (the page, or `ui.answer` in the Late debug drain).
    pub(crate) answer: Option<Option<usize>>,
}

impl Resource for PendingActivateChoice {}

/// What the player's perks make of one activation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PerkActivation {
    /// The default activation goes ahead (after any *Run Immediately* entry ran beside it).
    Default,
    /// A *Run Immediately* entry with *Replace Default* ran in place of the default activation.
    Replaced,
    /// The activation is held by the activate-choice box until the player answers it.
    Offered,
}

/// Compose `player`'s activate entries for activating `target` (PERK-HOOK-007): run the passing *Run
/// Immediately* entries, and hold the activation on the activate-choice box when a passing entry is a choice.
pub(crate) fn compose_player_activation(world: &World, player: EntityId, target: InteractionTarget) -> PerkActivation {
    let offers = compose_activate_choices(world, player, target.entity);
    if offers.is_empty() {
        return PerkActivation::Default;
    }
    let (immediate, choices): (Vec<_>, Vec<_>) = offers.into_iter().partition(|offer| offer.choice.run_immediately);
    let mut replaced = false;
    for offer in &immediate {
        let run = run_activate_choice(world, offer, player, target.entity);
        log::info!(
            "perk {:08X}: a Run Immediately activate entry ran on entity {} ({run:?}){}",
            offer.perk,
            target.entity,
            if offer.choice.replace_default { ", in place of the default activation" } else { "" }
        );
        replaced |= offer.choice.replace_default;
    }
    if replaced {
        return PerkActivation::Replaced;
    }
    if choices.is_empty() {
        return PerkActivation::Default;
    }
    let mut buttons = Vec::with_capacity(choices.len() + 1);
    buttons.push(target.kind.verb().to_owned());
    buttons.extend(choices.iter().map(|offer| offer.choice.label.clone()));
    let Some(mut pending) = world.try_resource_mut::<PendingScriptMessages>() else {
        // No page to show the choice on: the activation goes ahead as if no perk were involved.
        return PerkActivation::Default;
    };
    pending.0.push(ScriptMessage {
        title: String::new(),
        text: String::new(),
        message_box: true,
        buttons,
        answer: MessageAnswer::ActivateChoice,
    });
    drop(pending);
    if let Some(mut pending) = world.try_resource_mut::<PendingActivateChoice>() {
        pending.held = Some((target, choices));
        pending.answer = None;
    }
    PerkActivation::Offered
}

/// The player answered the activate-choice box with `button`; [`resolve_answered_activation`] acts on it.
pub(crate) fn answer_activate_choice(world: &World, button: Option<usize>) {
    if let Some(mut pending) = world.try_resource_mut::<PendingActivateChoice>() {
        if pending.held.is_some() {
            pending.answer = Some(button);
        }
    }
}

/// Act on the answered activate-choice box (PERK-HOOK-012). Button 0, a button past the last, or no answer
/// (no page to pick on) is the default activation. Button `k` runs the `k`-th choice in place of it.
pub(crate) fn resolve_answered_activation(world: &World) {
    let answered = {
        let Some(mut pending) = world.try_resource_mut::<PendingActivateChoice>() else {
            return;
        };
        let Some(button) = pending.answer.take() else {
            return;
        };
        pending.held.take().map(|(target, choices)| (target, choices, button))
    };
    let Some((target, choices, button)) = answered else {
        return;
    };
    if byroredux_scripting::condition::entity_global_form_id(world, target.entity).is_none() {
        log::warn!("activate choice: entity {} is gone; its held activation is dropped", target.entity);
        return;
    }
    match button.and_then(|button| button.checked_sub(1)).and_then(|k| choices.get(k)) {
        Some(offer) => {
            let Some(player) = byroredux_scripting::papyrus_demo::PapyrusPlayerEntity::get(world) else {
                return;
            };
            let run = run_activate_choice(world, offer, player, target.entity);
            log::info!(
                "perk {:08X}: picked activate choice {:?} on entity {} ({run:?})",
                offer.perk,
                offer.choice.label,
                target.entity
            );
            crate::interaction::record_perk_activation(world, target, &offer.choice.label);
        }
        None => crate::interaction::activate_target(world, target, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{activate_target, InteractionKind, InteractionTrace};
    use byroredux_core::character::{CharacterRulesProfile, Perks};
    use byroredux_core::ecs::components::{Dead, FormIdComponent};
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::{EsmIndex, PerkEntry, PerkEntryBody, PerkEntryDetails, PerkFunctionData, PerkRecord};
    use byroredux_scripting::papyrus_demo::PapyrusPlayerEntity;
    use byroredux_scripting::perk_entry_points::{fnv, skyrim};
    use byroredux_scripting::{ActivateEvent, PendingPerkEntries};

    const PERK: u32 = 0x0003_1DC2;
    const ROBOT: u32 = 0x0001_2345;

    fn activate_entry(hook: u8, details: PerkEntryDetails) -> PerkEntry {
        PerkEntry {
            rank: 0,
            priority: 0,
            body: PerkEntryBody::EntryPoint {
                entry_point_index: hook,
                function: 9,
                tab_count: 2,
                param_type: 4,
                function_data: PerkFunctionData::None,
                formatter: Vec::new(),
                extra_flags: Vec::new(),
                details,
            },
            condition_tabs: Vec::new(),
        }
    }

    fn label(text: &str) -> PerkEntryDetails {
        PerkEntryDetails { button_label: text.to_owned(), ..Default::default() }
    }

    /// The player holds `PERK` with `entries`; the target is a placed reference `ROBOT` the prompt offers to
    /// "Activate".
    fn activation_world(game: GameKind, entries: Vec<PerkEntry>) -> (World, InteractionTarget) {
        let character_rules =
            if game == GameKind::Skyrim { CharacterRulesProfile::SKYRIM } else { CharacterRulesProfile::FALLOUT_NEW_VEGAS };
        let mut index = EsmIndex { game, character_rules, ..Default::default() };
        index.perks.insert(PERK, PerkRecord { form_id: PERK, entries, ..Default::default() });
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<Perks>();
        world.register::<Dead>();
        world.register::<FormIdComponent>();
        byroredux_scripting::perk_entry_points::install_perk_entry_points(&mut world, &index);
        world.insert_resource(PendingScriptMessages::default());
        world.insert_resource(PendingActivateChoice::default());
        world.insert_resource(InteractionTrace::default());
        world.insert_resource(crate::notifications::MessageBoxQueue::default());
        world.insert_resource(crate::notifications::PlayerNotifications::default());
        world.insert_resource(PendingPerkEntries::default());
        let player = world.spawn();
        let mut perks = Perks::default();
        perks.set_rank(PERK, 1);
        world.insert(player, perks);
        world.insert_resource(PapyrusPlayerEntity(player));
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        let mut pool = FormIdPool::new();
        let form = pool.intern(FormIdPair { plugin: PluginId::from_filename("Test.esm"), local: LocalFormId(ROBOT) });
        world.insert_resource(pool);
        let entity = world.spawn();
        world.insert(entity, FormIdComponent(form));
        (world, InteractionTarget { entity, kind: InteractionKind::Activate, distance: 1.0 })
    }

    fn activated(world: &World, target: InteractionTarget) -> bool {
        world.get::<ActivateEvent>(target.entity).is_some()
    }

    /// Route the raised box onto the page queue, answer it as the page would, and let the interaction system's
    /// next run act on the answer.
    fn answer(world: &World, button: usize) {
        assert!(crate::notifications::route_script_messages(world), "the choice box is waiting");
        crate::notifications::answer_message_box(world, Some(button));
        resolve_answered_activation(world);
    }

    /// PERK-HOOK-007 (A8) — a passing choice holds the activation on the message-box page: the default verb
    /// first, then the entry's `EPF2` label. Picking the entry runs it in place of the activation, so the
    /// reference receives no `ActivateEvent`.
    #[test]
    fn activate_choice_runs_its_entry() {
        let mut details = label("Feed");
        details.fragment_index = Some(0);
        let (mut world, target) = activation_world(GameKind::Skyrim, vec![activate_entry(skyrim::ACTIVATE, details)]);
        let mut fragments = byroredux_scripting::PerkFragments::default();
        fragments.insert(
            PERK,
            byroredux_scripting::FragmentSet {
                vmad: None,
                fragments: vec![(byroredux_scripting::PerkEntryId(0), "PRKF_Test".into(), "Fragment_0".into())],
            },
        );
        world.insert_resource(fragments);

        activate_target(&world, target, true);
        assert!(!activated(&world, target), "the activation waits on the answer");
        let raised = world.resource::<PendingScriptMessages>().0.clone();
        assert_eq!(raised.len(), 1);
        assert_eq!(raised[0].buttons, vec!["Activate".to_owned(), "Feed".to_owned()]);
        assert_eq!(raised[0].answer, MessageAnswer::ActivateChoice);
        assert!(world.resource::<InteractionTrace>().last.as_ref().unwrap().outcome.contains("offered"));

        answer(&world, 1);
        assert!(!activated(&world, target), "the entry ran in place of the activation");
        let reported = world.resource::<PendingPerkEntries>().0.clone();
        assert_eq!(reported.len(), 1);
        assert_eq!((reported[0].perk, reported[0].entry, reported[0].target), (PERK, 0, Some(ROBOT)));
        assert!(world.resource::<PendingActivateChoice>().held.is_none(), "the held activation is spent");
        assert!(world.resource::<InteractionTrace>().last.as_ref().unwrap().outcome.contains("\"Feed\""));
    }

    /// Button 0 is the default activation: the `ActivateEvent` the activation would have raised without the
    /// perk, and nothing of the entry runs.
    #[test]
    fn the_default_button_activates_as_without_the_perk() {
        let (world, target) = activation_world(GameKind::Fallout3NV, vec![activate_entry(fnv::ACTIVATE, label("Deactivate"))]);
        activate_target(&world, target, true);
        assert!(!activated(&world, target));
        answer(&world, 0);
        assert!(activated(&world, target));
        assert!(world.resource::<InteractionTrace>().last.as_ref().unwrap().event_emitted);
        // An answer arrives once: a second resolve finds nothing held.
        resolve_answered_activation(&world);
    }

    /// No passing entry: the activation goes ahead at once, with no box.
    #[test]
    fn an_activation_with_no_passing_entry_is_not_held() {
        let (world, target) = activation_world(GameKind::Fallout3NV, Vec::new());
        activate_target(&world, target, true);
        assert!(activated(&world, target));
        assert!(world.resource::<PendingScriptMessages>().0.is_empty());
    }

    /// A Run Immediately entry runs with no box, beside the activation; with Replace Default it runs in its
    /// place (vanilla Skyrim `PlayerWerewolfFeed` is authored both).
    #[test]
    fn run_immediately_runs_beside_the_activation_or_in_its_place() {
        for replace_default in [false, true] {
            let mut details = label("");
            details.run_immediately = true;
            details.replace_default = replace_default;
            let (world, target) = activation_world(GameKind::Skyrim, vec![activate_entry(skyrim::ACTIVATE, details)]);
            activate_target(&world, target, true);
            assert!(world.resource::<PendingScriptMessages>().0.is_empty(), "no selection menu");
            assert_eq!(activated(&world, target), !replace_default, "replace_default {replace_default}");
        }
    }
}
