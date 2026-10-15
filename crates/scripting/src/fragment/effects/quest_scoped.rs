//! The quest-scoped effect applier. (#5468, split from `effects.rs`)

use super::*;

/// The original quest-scoped effect arms (`SetStage`/objectives), split
/// out so [`apply_effect`]'s object-targeting arms — which need `world`
/// but no `QuestRef` resolution — read cleanly above.
pub(super) fn apply_quest_scoped_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::SetStage { quest, stage } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let allow_repeated = deferred
                .quest_definitions
                .as_ref()
                .is_some_and(|definitions| definitions.allows_repeated_stages(quest));
            if !allow_repeated && stages.get_stage_done(quest, *stage) {
                return None;
            }
            let was_started = stages.is_started(quest);
            let previous_stage = stages.set_stage(quest, *stage);
            if !was_started {
                deferred.scene_actor_bindings_dirty = true;
            }
            Some(QuestStageAdvanced {
                quest,
                previous_stage,
                new_stage: *stage,
                stage_set: true,
            })
        }
        Effect::StartQuest { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let start_up_stage = deferred
                .quest_definitions
                .as_ref()
                .and_then(|definitions| definitions.start_up_stage(quest));
            let event = stages.start_quest(quest, start_up_stage);
            if event.is_some() {
                deferred.scene_actor_bindings_dirty = true;
            }
            event
        }
        Effect::StopQuest { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let shut_down_stage = deferred
                .quest_definitions
                .as_ref()
                .and_then(|definitions| definitions.shut_down_stage(quest));
            // #5812 — the shared stop lifecycle (running-only shut-down
            // stage advance + the stop itself).
            let (stopped, event) = stages.stop_with_shutdown(quest, shut_down_stage);
            if stopped {
                deferred.scene_actor_bindings_dirty = true;
            }
            event
        }
        Effect::CompleteQuest { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            if stages.complete(quest) {
                deferred.scene_actor_bindings_dirty = true;
            }
            None
        }
        Effect::ResetQuest { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            stages.reset(quest);
            objectives.reset(quest);
            deferred.scene_actor_bindings_dirty = true;
            None
        }
        Effect::SetQuestActive { quest, active } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            stages.set_active(quest, *active);
            None
        }
        Effect::SetObjectiveDisplayed {
            quest,
            objective,
            displayed,
        } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            objectives.set_displayed(quest, *objective, *displayed);
            None
        }
        Effect::SetObjectiveCompleted {
            quest,
            objective,
            completed,
        } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            objectives.set_completed(quest, *objective, *completed);
            None
        }
        Effect::SetObjectiveFailed {
            quest,
            objective,
            failed,
        } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            objectives.set_failed(quest, *objective, *failed);
            None
        }
        Effect::CompleteAllObjectives { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let authored = deferred
                .quest_definitions
                .as_ref()
                .map(|definitions| definitions.objectives(quest).to_vec())
                .unwrap_or_default();
            if authored.is_empty() {
                objectives.complete_all(quest);
            } else {
                objectives.complete_all_authored(quest, authored);
            }
            None
        }
        Effect::FailAllObjectives { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let authored = deferred
                .quest_definitions
                .as_ref()
                .map(|definitions| definitions.objectives(quest).to_vec())
                .unwrap_or_default();
            if authored.is_empty() {
                objectives.fail_all(quest);
            } else {
                objectives.fail_all_authored(quest, authored);
            }
            None
        }
        Effect::Conditional { .. }
        | Effect::QuestScope { .. }
        | Effect::SetGlobalValue { .. }
        | Effect::AddItem { .. }
        | Effect::RemoveItem { .. }
        | Effect::EquipItem { .. }
        | Effect::MoveTo { .. }
        | Effect::MovePlayerTo { .. }
        | Effect::Disable { .. }
        | Effect::Enable { .. }
        | Effect::StartScene { .. }
        | Effect::StopScene { .. }
        | Effect::Activate { .. }
        | Effect::SetOpen { .. }
        | Effect::SetLocked { .. }
        | Effect::SetLockLevel { .. }
        | Effect::SetPlayerRestrained { .. }
        | Effect::SetPlayerControls { .. }
        | Effect::SetPlayerAiDriven { .. }
        | Effect::SetHudCartMode { .. }
        | Effect::SetInChargen { .. }
        | Effect::ShowRaceMenu
        | Effect::RequestSave { .. }
        | Effect::SetQuestVariable { .. }
        | Effect::ModQuestVariable { .. }
        | Effect::CopyQuestVariable { .. }
        | Effect::SetActorValueNamed { .. }
        | Effect::DamageActorValue { .. }
        | Effect::ChangeFaction { .. }
        | Effect::SetActorValue { .. }
        | Effect::ForceAlias { .. }
        | Effect::BlockActivation { .. }
        | Effect::AddToMap { .. }
        | Effect::SetLocationCleared { .. }
        | Effect::SetBaseProtection { .. }
        | Effect::Kill { .. }
        | Effect::SetRelationshipRank { .. }
        | Effect::RewardPlayerXp { .. }
        | Effect::IncrementStat { .. }
        | Effect::AddAchievement { .. }
        | Effect::AdvanceSkill { .. }
        | Effect::SetLinkedRef { .. }
        | Effect::SetLocRefType { .. }
        | Effect::MoveToEditorLocation { .. }
        | Effect::SetLocationKeywordData { .. }
        | Effect::ForceLocationTo { .. }
        | Effect::SetPlayerTeammate { .. }
        | Effect::SetDoingFavor { .. }
        | Effect::PlayerCaps { .. }
        | Effect::SetLocal { .. }
        | Effect::SetReferenceVariable { .. }
        | Effect::ShowMessage { .. }
        | Effect::ShowHelpMessage { .. }
        | Effect::Notification { .. }
        | Effect::ShowBarterMenu { .. }
        | Effect::DeleteWhenAble { .. }
        | Effect::ChangeKeyword { .. }
        | Effect::SendStoryEvent { .. }
        | Effect::ChangePerk { .. }
        | Effect::SetEnemy { .. }
        | Effect::StartCombat { .. }
        | Effect::StopCombat { .. }
        | Effect::StopCombatAlarm { .. }
        | Effect::SetCrimeFaction { .. }
        | Effect::SetUnconscious { .. }
        | Effect::SetGhost { .. }
        | Effect::SetAlly { .. }
        | Effect::AddSpell { .. }
        | Effect::RemoveSpell { .. }
        | Effect::PlayIdle { .. }
        | Effect::SetVehicle { .. }
        | Effect::TetherToHorse { .. }
        | Effect::SetMotionType { .. }
        | Effect::SetSittingRotation { .. }
        | Effect::ExitCart { .. }
        | Effect::RegisterPlayerAnimationEvent { .. }
        | Effect::EvaluatePackage { .. }
        | Effect::Wait { .. }
        | Effect::WaitForActors3DLoaded { .. }
        | Effect::WaitWhile { .. }
        | Effect::Interpret { .. }
        | Effect::AddRaceSpells { .. }
        | Effect::RemoveRaceSpells { .. }
        | Effect::ProviderCall(_) => {
            unreachable!("object-targeting effects are handled by apply_effect directly")
        }
    }
}
