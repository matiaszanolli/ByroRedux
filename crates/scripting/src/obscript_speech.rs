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
#[derive(Debug, Clone, PartialEq)]
pub struct Speaking {
    pub speaker: EntityId,
    pub topic_form_id: u32,
    pub remaining: f32,
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
    crate::note_info_spoken(world, speaker, line.info.form_id);
    let mut lines = world.try_resource_mut::<PendingScriptedLines>()?;
    // One line at a time per speaker: a new one replaces the one being spoken.
    lines.speaking.retain(|speaking| speaking.speaker != speaker);
    lines.speaking.push(Speaking { speaker, topic_form_id: line.topic_form_id, remaining: seconds });
    lines.pending.push(line);
    Some(seconds)
}

/// Tick the lines in progress and run `SayToDone` for each that finished.
pub fn obscript_speech_system(world: &World, dt: f32) {
    let done: Vec<Speaking> = {
        let Some(mut lines) = world.try_resource_mut::<PendingScriptedLines>() else {
            return;
        };
        if lines.speaking.is_empty() {
            return;
        }
        for line in &mut lines.speaking {
            line.remaining -= dt;
        }
        let (done, going): (Vec<_>, Vec<_>) = std::mem::take(&mut lines.speaking).into_iter().partition(|line| line.remaining <= 0.0);
        lines.speaking = going;
        done
    };
    for line in done {
        crate::obscript_objects::deliver_say_to_done(world, line.speaker, line.topic_form_id, dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_line_replaces_the_one_the_speaker_is_saying_and_the_voice_duration_replaces_the_estimate() {
        let mut lines = PendingScriptedLines::default();
        let mut world = World::new();
        let speaker = world.spawn();
        lines.speaking.push(Speaking { speaker, topic_form_id: 7, remaining: 3.0 });
        lines.set_duration(speaker, 7, 1.25);
        assert_eq!(lines.speaking[0].remaining, 1.25);
        // A line of another topic is not the one the voice player timed.
        lines.set_duration(speaker, 8, 9.0);
        assert_eq!(lines.speaking[0].remaining, 1.25);
    }
}
