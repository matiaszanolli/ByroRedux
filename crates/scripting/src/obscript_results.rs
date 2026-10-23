//! M47.3 — *result scripts* (Oblivion / FO3 / FNV): the compiled statement
//! streams a quest stage item and a dialogue `INFO` carry inline, beside the
//! standalone `SCPT` scripts the quest and object hosts run.
//!
//! - A **stage item's** result script runs when its stage is set and the
//!   item's conditions pass — every passing item, not the first (GECK *Quest
//!   Stages Tab*: "each stage item whose conditions evaluate to true runs its
//!   result script"). It is a *non-reference* script (GECK *Non-reference
//!   scripts*): its host has the quest as owner and no implicit reference.
//! - An **INFO's** Begin script runs when the line is spoken and its End script
//!   when the line finishes. Those are *reference* scripts (GECK *Reference
//!   functions*): the speaker is the implicit reference, exactly as for the
//!   object script on a placed reference.
//!
//! This is how the Fallout-era quests move: VCG02 "Back in the Saddle" starts,
//! displays its first objective and sets its first stage from Sunny Smiles's
//! `INFO 0010A1E4` Begin script, and every later objective is displayed by a
//! stage item. The Skyrim+ twin is the `VMAD` fragment path
//! ([`crate::quest_fragment_dispatch_system`]), which has no table to dispatch
//! on these games.
//!
//! One deliberate difference from the original engine: `SetStage` there runs
//! the stage's result script *inside* the calling statement; here the calling
//! script finishes first and the stage's items run when this system next polls
//! the quest-event journal (the same frame for a stage script's own cascade,
//! the next Update for a dialogue line's). No vanilla result script reads back
//! state its own `SetStage` call's items write.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::condition::ConditionList;
use byroredux_plugin::esm::records::{QustRecord, ScriptRecord};

use crate::condition::{evaluate_modeled, ConditionContext};
use crate::obscript_quests::ObScriptCommandHost;
use crate::obscript_vm::{BlockOutcome, ObScriptProgram, VmState};
use crate::papyrus_demo::PapyrusPlayerEntity;
use crate::quest_stages::{QuestStageState, OBSCRIPT_STAGE_QUEST_EVENT_SUBSCRIBER};

/// Polls of the journal one system run makes: each poll after the first picks
/// up the stages the previous round's result scripts set. A cycle of stage
/// scripts that set each other stops here instead of spinning the frame.
const MAX_STAGE_SCRIPT_ROUNDS: usize = 16;

/// One stage item with a result script.
pub struct StageResultItem {
    /// The item's conditions (`CTDA` after its `QSDT`); empty passes.
    pub conditions: ConditionList,
    pub script: Arc<ScriptRecord>,
}

/// `(quest, stage)` → the stage's items that carry a result script, in
/// authored order. Built once per load order from the parsed `QUST` records.
#[derive(Default)]
pub struct QuestStageResultScripts {
    items: HashMap<(u32, u16), Arc<[StageResultItem]>>,
    /// Items run since install, for `quest.show`-style diagnostics and tests.
    pub runs: u64,
}

impl Resource for QuestStageResultScripts {}

impl QuestStageResultScripts {
    /// The stage's scripted items, if it has any.
    pub fn items(&self, quest: u32, stage: u16) -> Option<Arc<[StageResultItem]>> {
        self.items.get(&(quest, stage)).cloned()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Install every stage item's result script from the load order's quests.
/// Returns the number of stages that carry at least one. A no-op on
/// Skyrim+, whose stages author no embedded scripts.
pub fn install_stage_result_scripts(world: &mut World, quests: &HashMap<u32, QustRecord>) -> usize {
    let mut items: HashMap<(u32, u16), Arc<[StageResultItem]>> = HashMap::new();
    for quest in quests.values() {
        for stage in &quest.stages {
            let scripted: Vec<StageResultItem> = stage
                .log_entries
                .iter()
                .filter_map(|entry| {
                    let script = entry.result_script.clone()?;
                    Some(StageResultItem {
                        conditions: entry.conditions.clone(),
                        script,
                    })
                })
                .collect();
            if !scripted.is_empty() {
                items.insert((quest.form_id, stage.index), scripted.into());
            }
        }
    }
    let installed = items.len();
    if installed > 0 {
        world.insert_resource(QuestStageResultScripts { items, runs: 0 });
        log::info!("Installed result scripts on {installed} ObScript quest stages");
    }
    installed
}

/// Run one result script to its end. `owner` keys the effect log and the
/// decline census; `reference` is the implicit reference of a dialogue line's
/// script (the speaker) and `None` for a stage item's.
pub fn run_result_script(
    world: &World,
    owner: u32,
    reference: Option<EntityId>,
    script: &ScriptRecord,
) -> BlockOutcome {
    let mut state = VmState::default();
    let mut host = ObScriptCommandHost::new(world, owner, reference, 0.0, script);
    let outcome = ObScriptProgram::new(script).run_result_script(&mut state, &mut host);
    if let BlockOutcome::Malformed(at) = outcome {
        log::warn!(
            "ObScript result script on {:08X} (owner {owner:08X}): malformed bytecode at {at:#06x}",
            script.form_id
        );
    }
    outcome
}

/// Run a spoken INFO's Begin (`begin`) or End result script with the speaker
/// as its implicit reference. `speaker_form` is the speaker's reference
/// FormID, the script's owner. A no-op when the slot compiled nothing.
pub fn run_info_result_script(
    world: &World,
    info: &byroredux_plugin::esm::records::InfoRecord,
    begin: bool,
    speaker: EntityId,
    speaker_form: u32,
) {
    let script = if begin {
        &info.result_script_begin
    } else {
        &info.result_script_end
    };
    let Some(script) = script else {
        return;
    };
    let outcome = run_result_script(world, speaker_form, Some(speaker), script);
    log::info!(
        "ObScript INFO {:08X} {} result script ran on {speaker_form:08X}: {outcome:?}",
        info.form_id,
        if begin { "begin" } else { "end" },
    );
}

/// Run the result scripts of every stage set since the last run, then of
/// the stages those scripts set, up to [`MAX_STAGE_SCRIPT_ROUNDS`] rounds.
/// Registered beside the ObScript quest tick; a no-op without the table.
pub fn obscript_stage_result_system(world: &World, _dt: f32) {
    if world.try_resource::<QuestStageResultScripts>().is_none() {
        return;
    }
    let player = PapyrusPlayerEntity::get(world);
    for round in 0..MAX_STAGE_SCRIPT_ROUNDS {
        let events = {
            let Some(mut stages) = world.try_resource_mut::<QuestStageState>() else {
                return;
            };
            stages.poll_quest_events(OBSCRIPT_STAGE_QUEST_EVENT_SUBSCRIBER)
        };
        if events.missed_events > 0 {
            log::error!(
                "ObScript stage result scripts missed {} quest transition(s); their scripts did not run",
                events.missed_events
            );
        }
        if events.events.is_empty() {
            return;
        }
        // The guard is dropped before any script runs: the host takes the
        // quest and world resources itself.
        let due: Vec<(u32, u16, Arc<[StageResultItem]>)> = {
            let table = world.resource::<QuestStageResultScripts>();
            events
                .events
                .iter()
                .filter(|sequenced| sequenced.event.stage_set)
                .filter_map(|sequenced| {
                    let quest = sequenced.event.quest.0;
                    let stage = sequenced.event.new_stage;
                    Some((quest, stage, table.items(quest, stage)?))
                })
                .collect()
        };
        let mut ran = 0u64;
        for (quest, stage, items) in due {
            for item in items.iter() {
                // Stage conditions have no reference to run on (the stage is
                // not one; "Run on Target" is invalid there, GECK *Quest
                // Stages Tab*), so the player stands as the subject, and an
                // unreadable term fails the item closed (#5993).
                if !item.conditions.is_empty() {
                    let passes = player.is_some_and(|player| {
                        evaluate_modeled(&item.conditions, world, &ConditionContext::for_subject(player))
                    });
                    if !passes {
                        continue;
                    }
                }
                let outcome = run_result_script(world, quest, None, &item.script);
                log::info!("ObScript quest {quest:08X} stage {stage} result script ran: {outcome:?}");
                ran += 1;
            }
        }
        if let Some(mut table) = world.try_resource_mut::<QuestStageResultScripts>() {
            table.runs += ran;
        }
        if round + 1 == MAX_STAGE_SCRIPT_ROUNDS {
            log::warn!(
                "ObScript stage result scripts still setting stages after {MAX_STAGE_SCRIPT_ROUNDS} rounds; \
                 the rest run next frame"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obscript_quests::{ObScriptEffectLog, ObScriptQuestLocals, ObScriptQuestTimers};
    use crate::quest_stages::{QuestFormId, QuestObjectiveState};
    use byroredux_plugin::esm::records::ScriptRef;

    const VCG02: u32 = 0x0010_A214;
    const GECKOMAN: u32 = 0x0015_7C8F;

    fn fnv_world() -> World {
        let mut world = World::new();
        world.insert_resource(QuestStageState::default());
        world.insert_resource(QuestObjectiveState::default());
        world.insert_resource(ObScriptQuestTimers::default());
        world.insert_resource(ObScriptQuestLocals::default());
        world.insert_resource(ObScriptEffectLog::default());
        world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Fallout3NV));
        world
    }

    fn script(compiled: Vec<u8>, refs: Vec<ScriptRef>) -> ScriptRecord {
        ScriptRecord { compiled, refs, ..Default::default() }
    }

    /// `FalloutNV.esm` INFO `0010A1E4` (Sunny Smiles, "Yeah, I guess there's a
    /// thing or two I could show you."), Begin result script, bytes verbatim:
    /// `StartQuest VCG02; SetObjectiveCompleted VCG02 3 1; SetObjectiveDisplayed
    /// VCG02 5 1; SetStage VCG02 10; VESGSGeckomanNPC.Disable`, with its two
    /// `SCRO`s (Geckoman, VCG02).
    fn sunny_teach_begin() -> ScriptRecord {
        let compiled = vec![
            0x36, 0x10, 0x05, 0x00, 0x01, 0x00, 0x72, 0x02, 0x00, //
            0xa2, 0x11, 0x0f, 0x00, 0x03, 0x00, 0x72, 0x02, 0x00, 0x6e, 0x03, 0x00, 0x00, 0x00, 0x6e, 0x01, 0x00, 0x00, 0x00,
            0xa3, 0x11, 0x0f, 0x00, 0x03, 0x00, 0x72, 0x02, 0x00, 0x6e, 0x05, 0x00, 0x00, 0x00, 0x6e, 0x01, 0x00, 0x00, 0x00,
            0x39, 0x10, 0x0a, 0x00, 0x02, 0x00, 0x72, 0x02, 0x00, 0x6e, 0x0a, 0x00, 0x00, 0x00, //
            0x1c, 0x00, 0x01, 0x00, 0x22, 0x10, 0x02, 0x00, 0x00, 0x00,
        ];
        script(compiled, vec![ScriptRef::Object(GECKOMAN), ScriptRef::Object(VCG02)])
    }

    /// A result script is a bare statement stream: it runs from its first
    /// statement to the end of its bytes (no `Begin`/`End`), and the real
    /// line that opens "Back in the Saddle" starts the quest, completes and
    /// displays its objectives and sets its first stage.
    #[test]
    fn sunnys_teach_line_starts_back_in_the_saddle() {
        let world = fnv_world();
        let outcome = run_result_script(&world, 0x0010_4E85, None, &sunny_teach_begin());
        assert_eq!(outcome, BlockOutcome::Completed);
        let stages = world.resource::<QuestStageState>();
        assert!(stages.is_running(QuestFormId(VCG02)));
        assert_eq!(stages.get_stage(QuestFormId(VCG02)), 10);
        drop(stages);
        let objectives = world.resource::<QuestObjectiveState>();
        assert!(objectives.get(QuestFormId(VCG02), 3).completed);
        assert!(objectives.get(QuestFormId(VCG02), 5).displayed);
    }

    /// A stream that ends mid-statement is still malformed, not completed.
    #[test]
    fn a_truncated_result_script_is_malformed() {
        let world = fnv_world();
        let mut bytes = sunny_teach_begin().compiled;
        bytes.truncate(bytes.len() - 3);
        let truncated = script(bytes, sunny_teach_begin().refs);
        assert!(matches!(
            run_result_script(&world, 0x0010_4E85, None, &truncated),
            BlockOutcome::Malformed(_)
        ));
    }

    /// `SetStage <quest> <stage>` (`0x1039`) on ref #1.
    fn set_stage(stage: u8) -> Vec<u8> {
        vec![0x39, 0x10, 0x0a, 0x00, 0x02, 0x00, 0x72, 0x01, 0x00, 0x6e, stage, 0x00, 0x00, 0x00]
    }

    /// `SetObjectiveDisplayed <quest> <objective> 1` (`0x11a3`) on ref #1.
    fn display(objective: u8) -> Vec<u8> {
        vec![
            0xa3, 0x11, 0x0f, 0x00, 0x03, 0x00, 0x72, 0x01, 0x00, 0x6e, objective, 0x00, 0x00, 0x00, 0x6e, 0x01, 0x00,
            0x00, 0x00,
        ]
    }

    fn quest_with_stages(stages: &[(u16, Vec<u8>)]) -> QustRecord {
        use byroredux_plugin::esm::records::{QuestStage, QuestStageLogEntry};
        QustRecord {
            form_id: VCG02,
            stages: stages
                .iter()
                .map(|(index, compiled)| QuestStage {
                    index: *index,
                    log_entries: vec![QuestStageLogEntry {
                        result_script: Some(Arc::new(script(compiled.clone(), vec![ScriptRef::Object(VCG02)]))),
                        ..Default::default()
                    }],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Setting a stage runs its item's result script, and a stage that script
    /// sets runs its own in the same system run (VCG02 45 displays objective
    /// 30; here 10 sets 45).
    #[test]
    fn a_set_stage_runs_its_result_script_and_cascades() {
        let mut world = fnv_world();
        let quests = HashMap::from([(VCG02, quest_with_stages(&[(10, set_stage(45)), (45, display(30))]))]);
        assert_eq!(install_stage_result_scripts(&mut world, &quests), 2);
        world.resource_mut::<QuestStageState>().set_stage(QuestFormId(VCG02), 10);
        obscript_stage_result_system(&world, 0.0);
        assert_eq!(world.resource::<QuestStageState>().get_stage(QuestFormId(VCG02)), 45);
        assert!(world.resource::<QuestObjectiveState>().get(QuestFormId(VCG02), 30).displayed);
        assert_eq!(world.resource::<QuestStageResultScripts>().runs, 2);
        // The journal is consumed: a second run repeats nothing.
        obscript_stage_result_system(&world, 0.0);
        assert_eq!(world.resource::<QuestStageResultScripts>().runs, 2);
    }

    /// Two stages that set each other stop after the round cap instead of
    /// spinning the frame.
    #[test]
    fn a_stage_cycle_is_bounded() {
        let mut world = fnv_world();
        let quests = HashMap::from([(VCG02, quest_with_stages(&[(10, set_stage(20)), (20, set_stage(10))]))]);
        install_stage_result_scripts(&mut world, &quests);
        world.resource_mut::<QuestStageState>().set_stage(QuestFormId(VCG02), 10);
        obscript_stage_result_system(&world, 0.0);
        assert_eq!(world.resource::<QuestStageResultScripts>().runs, MAX_STAGE_SCRIPT_ROUNDS as u64);
    }
}
