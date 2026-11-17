//! `Say` and `SayTo` (M47.3): an actor speaks one line of a topic.
//!
//! CS *Say*: "have an NPC speak a single dialogue item on the specified topic ... returns
//! the time in seconds that the NPC will take to finish speaking, which can be useful for
//! timing followups in scripts"; *SayTo* is the same "to the player or another NPC"; and the
//! `SayToDone` block "is triggered when a Say or SayTo is complete" (GECK, with an optional
//! topic parameter).
//!
//! The line is the topic's first eligible INFO ([`select_first_info`]: its speaker filter,
//! its conditions with the speaker as subject and the target — the player by default — as
//! the other party, the Say-Once and quest rules), exactly the pick an activation greeting
//! makes. The call answers its length in seconds: the subtitle estimate, which the voice
//! player replaces with the real duration when a recording resolves
//! ([`PendingScriptedLines`] is what the binary's voice system drains). When that time has
//! elapsed the speaker's `SayToDone` blocks run, each for the topic it names or for any.
//!
//! The line's own result scripts run as for any INFO played (GECK *Category:Dialogue*:
//! "Result Script (Begin): This script runs when the Info is played, at the moment the actor
//! starts saying his line. Result Script (End): This script runs at the moment the actor
//! finishes speaking"): Begin on the first tick of the line, End when it finishes, before the
//! `SayToDone` blocks. Begin waits for the tick rather than running inside `say`, because
//! `say` is called from a running script whose variables a nested script must not race.
//! New Vegas's opening is chained this way: each of Doc Mitchell's `VCG01Intro` lines ends
//! with `SetStage VCG01 <n>` or the next `SayTo`.
//!
//! Not modelled: the optional speaker form (a creature or activator speaking with an NPC's
//! voice), the voice-position flag, `SayTo`'s head tracking, and subtitles — an ambient line
//! has no presenter here, only the conversation page has one.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, Resource, SparseSetStorage, World};
use byroredux_plugin::esm::records::misc::dialogue::InfoRecord;

use crate::dialogue::{estimate_dialogue_duration, select_first_info, DialogueRegistry};

/// One line a script made an actor say, waiting for the voice player.
#[derive(Debug, Clone)]
pub struct ScriptedLine {
    pub speaker: EntityId,
    pub topic_form_id: u32,
    pub topic_editor_id: String,
    pub owning_quest: Option<u32>,
    pub info: InfoRecord,
}

/// The line an actor is saying: the topic, the INFO, and the seconds it has left. One per speaker:
/// a new line replaces the one being said. Ticked by [`obscript_speech_system`].
///
/// Saved, keyed by the speaker's reference (#6944): the line's Begin result script has run and its
/// Say-Once mark is spent and saved, so a save taken mid-line that dropped the line lost its End
/// script — the `SetStage` or next `SayTo` a chain like Doc Mitchell's `VCG01Intro` moves on with.
/// After a load the line finishes its remaining seconds (silently: the voice is not resumed) and
/// runs its End script and `SayToDone` blocks once. A live load drops the outgoing session's lines
/// before the save's are overlaid.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct SpeakingLine {
    pub topic_form_id: u32,
    /// The INFO being said, one of the topic's, whose result scripts run at the start and end of
    /// the line; `None` for a line with no record behind it.
    pub info_form_id: Option<u32>,
    pub remaining: f32,
    /// Whether the Begin result script has run.
    pub started: bool,
}

impl Component for SpeakingLine {
    type Storage = SparseSetStorage<Self>;
}

/// The voice player resolved a recording for `speaker`'s line of `topic`: its duration replaces
/// the estimate the call answered with.
pub fn set_line_duration(world: &World, speaker: EntityId, topic_form_id: u32, seconds: f32) {
    let Some(mut lines) = world.query_mut::<SpeakingLine>() else {
        return;
    };
    if let Some(line) = lines.get_mut(speaker).filter(|line| line.topic_form_id == topic_form_id) {
        line.remaining = seconds.max(0.0);
    }
}

/// The INFO `speaker` is saying, if it is saying one.
pub fn speaking_info(world: &World, speaker: EntityId) -> Option<u32> {
    world.get::<SpeakingLine>(speaker).and_then(|line| line.info_form_id)
}

/// The requests scripts have made of the voice player and the dialogue runtime.
/// `NOT_SAVED_BY_DESIGN`, and discarded on a live load: each is drained within a frame. The lines
/// themselves are [`SpeakingLine`]s on their speakers, which are saved.
#[derive(Debug, Default)]
pub struct PendingScriptedLines {
    /// Lines the voice player has not picked up yet.
    pub pending: Vec<ScriptedLine>,
    /// `StartConversation player <topic>`: the actor and the topic its force-greet opens with
    /// (`None` for the generic greeting), waiting for the dialogue runtime to install it.
    pub conversations: Vec<(EntityId, Option<u32>)>,
}

impl Resource for PendingScriptedLines {}

/// Make `speaker` say a line of `topic` to `target` (the player when none is named).
/// Returns the seconds it will take, or `None` when there is nothing to say: no such topic,
/// no eligible INFO, or no resource to carry the line.
pub fn say(world: &World, speaker: EntityId, topic: u32, target: Option<EntityId>) -> Option<f32> {
    let target = target.or_else(|| crate::papyrus_demo::PapyrusPlayerEntity::get(world));
    let (line, seconds) = {
        let registry = world.try_resource::<DialogueRegistry>()?;
        let record = registry.topic(topic)?;
        let info = select_first_info(record, world, Some(speaker), target)?;
        let seconds = estimate_dialogue_duration(&info.response_text);
        let owning_quest = if info.quest != 0 { Some(info.quest) } else { record.quest_refs.first().copied() };
        (
            ScriptedLine { speaker, topic_form_id: record.form_id, topic_editor_id: record.editor_id.clone(), owning_quest, info: info.clone() },
            seconds,
        )
    };
    let speaking = SpeakingLine { topic_form_id: line.topic_form_id, info_form_id: Some(line.info.form_id), remaining: seconds, started: false };
    world.try_resource::<PendingScriptedLines>()?;
    // One line at a time per speaker: a new one replaces the one being spoken. A Say-Once line is
    // spent when it STARTS ([`obscript_speech_system`]), not when it is asked for: two `SayTo`s in
    // one frame (New Vegas's shooting-range trigger fires `OnTriggerEnter` and `OnTrigger` together)
    // choose the same line and the second replaces the first, instead of the first being spent
    // without its Begin result script ever running.
    world.query_mut::<SpeakingLine>()?.insert(speaker, speaking);
    world.try_resource_mut::<PendingScriptedLines>()?.pending.push(line);
    Some(seconds)
}

/// Tick the lines in progress: run each new line's Begin result script, and for each that
/// finished its End result script and then its `SayToDone` blocks.
pub fn obscript_speech_system(world: &World, dt: f32) {
    let (started, done) = {
        let Some(mut lines) = world.query_mut::<SpeakingLine>() else {
            return;
        };
        let mut started = Vec::new();
        let mut done = Vec::new();
        for (speaker, line) in lines.iter_mut() {
            if !line.started {
                line.started = true;
                started.push((speaker, line.clone()));
            }
            line.remaining -= dt;
            if line.remaining <= 0.0 {
                done.push((speaker, line.clone()));
            }
        }
        for (speaker, _) in &done {
            lines.remove(*speaker);
        }
        (started, done)
    };
    // The guard is dropped: a result script may `SayTo` again, which gives the speaker a new line.
    for (speaker, line) in &started {
        // The line is being said now: a Say-Once INFO stops qualifying before its own Begin script
        // (which may `SayTo` the same topic again) runs.
        if let Some(info) = line.info_form_id {
            crate::note_info_spoken(world, *speaker, info);
        }
        run_line_result_script(world, *speaker, line, true);
    }
    for (speaker, line) in done {
        run_line_result_script(world, speaker, &line, false);
        crate::obscript_objects::deliver_say_to_done(world, speaker, line.topic_form_id, dt);
    }
}

/// Run the Begin or End result script of the INFO `line` is saying, on its speaker. The INFO is
/// looked up among its topic's records, which a load rebuilds, so a line restored from a save
/// runs the same script.
fn run_line_result_script(world: &World, speaker: EntityId, line: &SpeakingLine, begin: bool) {
    let Some(info_form_id) = line.info_form_id else {
        return;
    };
    let info = {
        let Some(registry) = world.try_resource::<DialogueRegistry>() else {
            return;
        };
        let Some(info) = registry.topic(line.topic_form_id).and_then(|topic| topic.infos.iter().find(|info| info.form_id == info_form_id)) else {
            log::warn!("SayTo: INFO {info_form_id:08X} is not a line of topic {:08X}; its result script does not run", line.topic_form_id);
            return;
        };
        if (if begin { &info.result_script_begin } else { &info.result_script_end }).is_none() {
            return;
        }
        info.clone()
    };
    let Some(speaker_form) = crate::condition::entity_global_form_id(world, speaker) else {
        log::warn!("SayTo: INFO {info_form_id:08X}'s result script has no speaker reference to run on");
        return;
    };
    crate::obscript_results::run_info_result_script(world, &info, begin, speaker, speaker_form);
}

/// This subsystem's saved column. Called by the binary's `build_save_registry` in its row order.
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_replacing_component::<SpeakingLine>("SpeakingLine");
}

/// #6706 — this subsystem's world setup, owned here instead of the crate's `register` hub.
pub fn install(world: &mut byroredux_core::ecs::World) {
    // M47.3 — the voice and conversation requests `Say` / `SayTo` / `StartConversation` make (not
    // saved), and the lines being said (saved, #6944).
    world.init_resource::<PendingScriptedLines>();
    world.register::<SpeakingLine>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_voice_duration_replaces_the_estimate() {
        let mut world = World::new();
        world.register::<SpeakingLine>();
        let speaker = world.spawn();
        world.insert(speaker, SpeakingLine { topic_form_id: 7, info_form_id: None, remaining: 3.0, started: false });
        set_line_duration(&world, speaker, 7, 1.25);
        assert_eq!(world.get::<SpeakingLine>(speaker).unwrap().remaining, 1.25);
        // A line of another topic is not the one the voice player timed.
        set_line_duration(&world, speaker, 8, 9.0);
        assert_eq!(world.get::<SpeakingLine>(speaker).unwrap().remaining, 1.25);
    }

    /// A Say-Once line is spent when it starts, not when it is asked for. New Vegas's VCG02
    /// shooting-range trigger runs `SunnyREF.SayTo Player VCG02SunnyBark` from two blocks in the
    /// same frame; the topic's first INFO (`10A1E6`, "see those sarsaparilla bottles", whose Begin
    /// script displays objective 10 and enables the bottles) used to be spent by the first call
    /// and replaced by the second call's line (`10A1E7`, "Okay, you're on") before it ever ran.
    #[test]
    fn two_same_frame_says_choose_the_same_say_once_line_and_it_is_spent_when_it_starts() {
        use byroredux_plugin::esm::records::{DialRecord, InfoDataHeader};
        let say_once = |form_id: u32| InfoRecord {
            form_id,
            data: Some(InfoDataHeader { flags1: 0x04, ..Default::default() }),
            ..Default::default()
        };
        let mut world = World::new();
        world.insert_resource(crate::dialogue::DialogueSpokenInfoForms::default());
        world.insert_resource(DialogueRegistry::from_records([DialRecord {
            form_id: 0x10A1E0,
            infos: vec![say_once(0x10A1E6), say_once(0x10A1E7)],
            ..Default::default()
        }]));
        world.insert_resource(PendingScriptedLines::default());
        world.register::<SpeakingLine>();
        let speaker = world.spawn();

        assert!(say(&world, speaker, 0x10A1E0, None).is_some());
        assert!(say(&world, speaker, 0x10A1E0, None).is_some());
        assert_eq!(world.query::<SpeakingLine>().unwrap().len(), 1, "the second call replaces the first");
        assert_eq!(speaking_info(&world, speaker), Some(0x10A1E6), "both chose the first line");
        assert!(world.resource::<crate::dialogue::DialogueSpokenInfoForms>().0.is_empty(), "nothing is spent before a line starts");

        obscript_speech_system(&world, 0.1);
        assert_eq!(world.resource::<crate::dialogue::DialogueSpokenInfoForms>().0.len(), 1, "the started line is spent");

        // The line is under way; a later call finds the next one.
        assert!(say(&world, speaker, 0x10A1E0, None).is_some());
        assert_eq!(speaking_info(&world, speaker), Some(0x10A1E7));
    }

    /// A scripted line runs its INFO's Begin result script when it starts and its End
    /// result script when it finishes. `FalloutNV.esm` INFO `00104BEE` (Doc Mitchell, "You're
    /// awake. How about that.") ends with `SetStage VCG01 5`, which is how New Vegas's opening
    /// moves on; before this ran, VCG01 sat at stage 3 for good.
    #[test]
    fn a_said_line_runs_its_begin_and_end_result_scripts() {
        use crate::quest_stages::{QuestFormId, QuestStageState};
        use byroredux_plugin::esm::records::{ScriptRecord, ScriptRef};
        const VCG01: u32 = 0x0010_4C1C;
        // `SetStage <ref #1> <stage>` (`0x1039`).
        let set_stage = |stage: u8| ScriptRecord {
            compiled: vec![0x39, 0x10, 0x0a, 0x00, 0x02, 0x00, 0x72, 0x01, 0x00, 0x6e, stage, 0x00, 0x00, 0x00],
            refs: vec![ScriptRef::Object(VCG01)],
            ..Default::default()
        };
        let mut world = World::new();
        world.insert_resource(QuestStageState::default());
        world.insert_resource(crate::obscript_quests::ObScriptQuestTimers::default());
        world.insert_resource(crate::obscript_quests::ObScriptEffectLog::default());
        world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Fallout3NV));
        // The speaker needs a reference form to run the scripts on; the player has one by definition.
        let speaker = world.spawn();
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(speaker));
        let info = InfoRecord {
            form_id: 0x0010_4BEE,
            result_script_begin: Some(std::sync::Arc::new(set_stage(3))),
            result_script_end: Some(std::sync::Arc::new(set_stage(5))),
            ..Default::default()
        };
        world.insert_resource(DialogueRegistry::from_records([byroredux_plugin::esm::records::DialRecord {
            form_id: 7,
            infos: vec![info],
            ..Default::default()
        }]));
        world.register::<SpeakingLine>();
        world.insert(speaker, SpeakingLine { topic_form_id: 7, info_form_id: Some(0x0010_4BEE), remaining: 1.0, started: false });

        obscript_speech_system(&world, 0.25);
        assert_eq!(world.resource::<QuestStageState>().get_stage(QuestFormId(VCG01)), 3, "Begin runs as the line starts");
        obscript_speech_system(&world, 0.25);
        assert_eq!(world.resource::<QuestStageState>().get_stage(QuestFormId(VCG01)), 3, "End waits for the line to finish");
        obscript_speech_system(&world, 1.0);
        assert_eq!(world.resource::<QuestStageState>().get_stage(QuestFormId(VCG01)), 5, "End runs when it finishes");
        assert!(world.get::<SpeakingLine>(speaker).is_none());
    }

    /// #6944 — a line restored from a save taken mid-line: its Begin script ran before the save
    /// (`started`), so after the load only the End script runs, once, when the remaining seconds
    /// are spent. VCG01's chain depends on that End `SetStage`.
    #[test]
    fn a_line_restored_mid_way_runs_only_its_end_result_script_once() {
        use crate::quest_stages::{QuestFormId, QuestStageState};
        use byroredux_plugin::esm::records::{ScriptRecord, ScriptRef};
        const VCG01: u32 = 0x0010_4C1C;
        let set_stage = |stage: u8| ScriptRecord {
            compiled: vec![0x39, 0x10, 0x0a, 0x00, 0x02, 0x00, 0x72, 0x01, 0x00, 0x6e, stage, 0x00, 0x00, 0x00],
            refs: vec![ScriptRef::Object(VCG01)],
            ..Default::default()
        };
        let mut world = World::new();
        world.insert_resource(QuestStageState::default());
        world.insert_resource(crate::obscript_quests::ObScriptQuestTimers::default());
        world.insert_resource(crate::obscript_quests::ObScriptEffectLog::default());
        world.insert_resource(crate::ScriptGame(byroredux_plugin::esm::reader::GameKind::Fallout3NV));
        let speaker = world.spawn();
        world.insert_resource(crate::papyrus_demo::PapyrusPlayerEntity(speaker));
        world.insert_resource(DialogueRegistry::from_records([byroredux_plugin::esm::records::DialRecord {
            form_id: 7,
            infos: vec![InfoRecord {
                form_id: 0x0010_4BEE,
                result_script_begin: Some(std::sync::Arc::new(set_stage(3))),
                result_script_end: Some(std::sync::Arc::new(set_stage(5))),
                ..Default::default()
            }],
            ..Default::default()
        }]));
        world.register::<SpeakingLine>();
        // As the save holds it: begun, 0.5 s left.
        world.insert(speaker, SpeakingLine { topic_form_id: 7, info_form_id: Some(0x0010_4BEE), remaining: 0.5, started: true });

        obscript_speech_system(&world, 0.25);
        assert_eq!(world.resource::<QuestStageState>().get_stage(QuestFormId(VCG01)), 0, "Begin does not run again");
        obscript_speech_system(&world, 0.25);
        assert_eq!(world.resource::<QuestStageState>().get_stage(QuestFormId(VCG01)), 5, "End runs when the line finishes");
        assert!(world.get::<SpeakingLine>(speaker).is_none());
    }

}
