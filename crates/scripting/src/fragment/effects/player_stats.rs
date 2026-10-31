//! Player stats, followers and economy. (#6142, split from `apply_effect`)

use super::*;

/// `RewardPlayerXp`, `SetPlayerTeammate`, `SetDoingFavor`, `AdvanceSkill`, `AddAchievement`, `IncrementStat` and `PlayerCaps`.
///
/// #6142 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays named in
/// `apply_effect`'s nested-lock residual list, which the #3949 scan reads
/// across these helpers too.
pub(super) fn apply_player_stats_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::RewardPlayerXp { amount, direct } => {
            let amount = resolve_item_count(amount, context, vmad, world, stages, deferred)?;
            deferred.player_xp.push((amount, *direct));
            None
        }
        Effect::SetPlayerTeammate {
            actor,
            teammate,
            can_do_favor,
        } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let form = crate::condition::entity_global_form_id(world, actor)?;
            deferred.follower_changes.push((
                form,
                FollowerChange::Teammate {
                    teammate: *teammate,
                    can_do_favor: *can_do_favor,
                },
            ));
            None
        }
        Effect::SetDoingFavor { actor, doing } => {
            let actor = guard_actor(actor, context, vmad, world, deferred)?;
            let form = crate::condition::entity_global_form_id(world, actor)?;
            deferred.follower_changes.push((form, FollowerChange::DoingFavor(*doing)));
            None
        }
        Effect::AdvanceSkill { skill, magnitude } => {
            deferred.skill_advances.push((skill.clone(), *magnitude));
            None
        }
        Effect::ObtainWord { word } => {
            let word = resolve_property_form_id(vmad, word.property_name())?;
            deferred.word_gains.push(word);
            None
        }
        Effect::AddAchievement { id } => {
            deferred.achievements.push(*id);
            None
        }
        Effect::IncrementStat { stat, amount } => {
            deferred.misc_stats.push((stat.clone(), *amount));
            None
        }
        Effect::PlayerCaps { delta, silent } => {
            // `Game.GetCaps()` is `Game.GetForm(15)` in FO4's `Game.psc`.
            const CAPS: u32 = 0x0000_000F;
            let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world)?;
            match delta.cmp(&0) {
                std::cmp::Ordering::Greater => {
                    add_items(world, player, CAPS, *delta as u32);
                    if !*silent {
                        deferred.messages.push(PendingMessage::ItemAdded(CAPS, *delta as u32));
                    }
                }
                std::cmp::Ordering::Less => {
                    remove_items(world, player, CAPS, delta.unsigned_abs(), None);
                }
                std::cmp::Ordering::Equal => {}
            }
            None
        }
        _ => unreachable!("`apply_effect` only routes player stats effects here"),
    }
}
