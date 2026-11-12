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
use byroredux_core::ecs::{Resource, World};
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

/// A line being spoken: the actor, the topic, and the seconds it has left.
#[derive(Debug, Clone)]
pub struct Speaking {
    pub speaker: EntityId,
    pub topic_form_id: u32,
    pub remaining: f32,
    /// The INFO being said, whose result scripts run at the start and end of the line;
    /// `None` for a line with no record behind it.
    pub info: Option<std::sync::Arc<InfoRecord>>,
    /// Whether the Begin result script has run.
    pub started: bool,
}

/// The lines scripts have made actors say. `NOT_SAVED_BY_DESIGN`: a line in flight is a
/// moment of speech, not gameplay state, and ends with the session.
#[derive(Debug, Default)]
pub struct PendingScriptedLines {
    /// Lines the voice player has not picked up yet.
    pub pending: Vec<ScriptedLine>,
    /// Lines in progress, ticked by [`obscript_speech_system`].
    pub speaking: Vec<Speaking>,
    /// `StartConversation player <topic>`: the actor and the topic its force-greet opens with
    /// (`None` for the generic greeting), waiting for the dialogue runtime to install it.
    pub conversations: Vec<(EntityId, Option<u32>)>,
}

impl Resource for PendingScriptedLines {}

impl PendingScriptedLines {
    /// The voice player resolved a recording for `speaker`'s line of `topic`: its duration
    /// replaces the estimate the call answered with.
    pub fn set_duration(&mut self, speaker: EntityId, topic_form_id: u32, seconds: f32) {
        if let Some(line) = self.speaking.iter_mut().find(|line| line.speaker == speaker && line.topic_form_id == topic_form_id) {
            line.remaining = seconds.max(0.0);
        }
    }
}

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
    let mut lines = world.try_resource_mut::<PendingScriptedLines>()?;
    // One line at a time per speaker: a new one replaces the one being spoken. A Say-Once line is
    // spent when it STARTS ([`obscript_speech_system`]), not when it is asked for: two `SayTo`s in
    // one frame (New Vegas's shooting-range trigger fires `OnTriggerEnter` and `OnTrigger` together)
    // choose the same line and the second replaces the first, instead of the first being spent
    // without its Begin result script ever running.
    lines.speaking.retain(|speaking| speaking.speaker != speaker);
    let info = std::sync::Arc::new(line.info.clone());
    lines.speaking.push(Speaking { speaker, topic_form_id: line.topic_form_id, remaining: seconds, info: Some(info), started: false });
    lines.pending.push(line);
    Some(seconds)
}

/// Tick the lines in progress: run each new line's Begin result script, and for each that
/// finished its End result script and then its `SayToDone` blocks.
pub fn obscript_speech_system(world: &World, dt: f32) {
    let (started, done): (Vec<Speaking>, Vec<Speaking>) = {
        let Some(mut lines) = world.try_resource_mut::<PendingScriptedLines>() else {
            return;
        };
        if lines.speaking.is_empty() {
            return;
        }
        let mut started = Vec::new();
        for line in &mut lines.speaking {
            if !line.started {
                line.started = true;
                started.push(line.clone());
            }
            line.remaining -= dt;
        }
        let (done, going): (Vec<_>, Vec<_>) = std::mem::take(&mut lines.speaking).into_iter().partition(|line| line.remaining <= 0.0);
        lines.speaking = going;
        (started, done)
    };
    // The guard is dropped: a result script may `SayTo` again, which pushes a new line.
    for line in &started {
        // The line is being said now: a Say-Once INFO stops qualifying before its own Begin script
        // (which may `SayTo` the same topic again) runs.
        if let Some(info) = &line.info {
            crate::note_info_spoken(world, line.speaker, info.form_id);
        }
        run_line_result_script(world, line, true);
    }
    for line in done {
        run_line_result_script(world, &line, false);
        crate::obscript_objects::deliver_say_to_done(world, line.speaker, line.topic_form_id, dt);
    }
}

/// Run the Begin or End result script of the INFO `line` is saying, on its speaker.
fn run_line_result_script(world: &World, line: &Speaking, begin: bool) {
    let Some(info) = &line.info else {
        return;
    };
    if (if begin { &info.result_script_begin } else { &info.result_script_end }).is_none() {
        return;
    }
    let Some(speaker_form) = crate::condition::entity_global_form_id(world, line.speaker) else {
        log::warn!("SayTo: INFO {:08X}'s result script has no speaker reference to run on", info.form_id);
        return;
    };
    crate::obscript_results::run_info_result_script(world, info, begin, line.speaker, speaker_form);
}

/// #6706 — this subsystem's world setup, owned here instead of the crate's `register` hub.
pub fn install(world: &mut byroredux_core::ecs::World) {
    // M47.3 — the lines scripts made actors say (`Say` / `SayTo`; not saved).
    world.init_resource::<PendingScriptedLines>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_line_replaces_the_one_the_speaker_is_saying_and_the_voice_duration_replaces_the_estimate() {
        let mut lines = PendingScriptedLines::default();
        let mut world = World::new();
        let speaker = world.spawn();
        lines.speaking.push(Speaking { speaker, topic_form_id: 7, remaining: 3.0, info: None, started: false });
        lines.set_duration(speaker, 7, 1.25);
        assert_eq!(lines.speaking[0].remaining, 1.25);
        // A line of another topic is not the one the voice player timed.
        lines.set_duration(speaker, 8, 9.0);
        assert_eq!(lines.speaking[0].remaining, 1.25);
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
        let speaker = world.spawn();

        assert!(say(&world, speaker, 0x10A1E0, None).is_some());
        assert!(say(&world, speaker, 0x10A1E0, None).is_some());
        {
            let lines = world.resource::<PendingScriptedLines>();
            assert_eq!(lines.speaking.len(), 1, "the second call replaces the first");
            assert_eq!(lines.speaking[0].info.as_ref().map(|info| info.form_id), Some(0x10A1E6), "both chose the first line");
        }
        assert!(world.resource::<crate::dialogue::DialogueSpokenInfoForms>().0.is_empty(), "nothing is spent before a line starts");

        obscript_speech_system(&world, 0.1);
        assert_eq!(world.resource::<crate::dialogue::DialogueSpokenInfoForms>().0.len(), 1, "the started line is spent");

        // The line is under way; a later call finds the next one.
        assert!(say(&world, speaker, 0x10A1E0, None).is_some());
        let lines = world.resource::<PendingScriptedLines>();
        assert_eq!(lines.speaking.last().and_then(|line| line.info.as_ref().map(|info| info.form_id)), Some(0x10A1E7));
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
        let mut lines = PendingScriptedLines::default();
        lines.speaking.push(Speaking { speaker, topic_form_id: 7, remaining: 1.0, info: Some(std::sync::Arc::new(info)), started: false });
        world.insert_resource(lines);

        obscript_speech_system(&world, 0.25);
        assert_eq!(world.resource::<QuestStageState>().get_stage(QuestFormId(VCG01)), 3, "Begin runs as the line starts");
        obscript_speech_system(&world, 0.25);
        assert_eq!(world.resource::<QuestStageState>().get_stage(QuestFormId(VCG01)), 3, "End waits for the line to finish");
        obscript_speech_system(&world, 1.0);
        assert_eq!(world.resource::<QuestStageState>().get_stage(QuestFormId(VCG01)), 5, "End runs when it finishes");
        assert!(world.resource::<PendingScriptedLines>().speaking.is_empty());
    }
}
