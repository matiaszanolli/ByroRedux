//! M48 Phase 4 GUI-PAGE-004 (#6666) — Skyrim's dialogue menu on its own movie (S1).
//!
//! PRD Q1: a menu runs on the game's own movie once its host calls are served, the dialogue menu
//! first; the native page stays the fallback (S3). When the registry's game names a movie for its
//! dialogue menu and the interface archive is beside the ESM (Skyrim's `interface\dialoguemenu.swf`),
//! a conversation opens that movie instead of the native page, and this driver stands in for the
//! native code on both sides of it:
//!
//! - **Engine → movie** (measured from the movie's own `InitExtensions`, which registers these as
//!   GameDelegate callbacks on `DialogueMenu`): `SetSpeakerName(strName)`,
//!   `ShowDialogueText(astrText)` / `HideDialogueText()`, `PopulateDialogueList` (method
//!   `PopulateDialogueLists`: `text, topicIsNew, topicIndex` per topic, then the selected index,
//!   `-1` for none), `ShowDialogueList` (method `DoShowDialogueList(abNewList, abHideExitButton)`)
//!   and `NotifyVoiceReady` (method `OnVoiceReady`, which lets the player skip or pick after its
//!   timer). The driver calls the methods on `_root.DialogueMenu_mc` through the path service when
//!   the conversation's snapshot changes.
//! - **Movie → engine** (the movie's `GameDelegate.call` sites, Phase 0's H1 rows): `TopicClicked
//!   [topicIndex]` selects that topic through the same `select_topic_by_form_id` the native page's
//!   click reaches; `SkipText` cuts the line (its voice stops and the speaking cue ends);
//!   `CloseMenu` ends the conversation as the page's Close does; `FadeDone` (the fade-out played)
//!   closes the movie. Each is registered with the bridge, so `ui.host` reports them H1.
//!
//! The conversation itself (selection, speaking cue, said-markers, Goodbye) is unchanged:
//! `dialogue_snapshot` is what both surfaces present.

use byroredux_core::ecs::{EntityId, World};
use byroredux_native_ui::DialogueTopicSnapshot;
use byroredux_ui::{PathOp, ScaleformHostCall, ScaleformValue, UiLayer, WhenAbsent};

/// The movie's root clip, the `DialogueMenu` instance.
const ROOT: &str = "_root.DialogueMenu_mc";

/// The host methods this driver serves (H1).
pub(crate) const SERVED_METHODS: [&str; 4] = ["TopicClicked", "SkipText", "CloseMenu", "FadeDone"];

/// The open dialogue movie.
#[derive(Debug)]
pub(crate) struct DialogueMovie {
    /// The movie's layer in the `UiManager` stack (`Dialogue Menu`).
    pub(crate) layer: String,
    /// The last snapshot fed, so an unchanged conversation is not re-sent.
    signature: Option<u64>,
    /// The line whose voice-ready notice was sent.
    notified_info: Option<u32>,
    /// The movie asked to close (`CloseMenu`); the conversation has ended.
    pub(crate) closing: bool,
    /// Frames since `closing`, so a movie that never reports `FadeDone` still closes.
    pub(crate) closing_frames: u32,
}

/// Frames a closing movie gets to play its fade-out before the engine closes it anyway.
pub(crate) const MAX_FADE_FRAMES: u32 = 240;

/// What the movie asked of the engine.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DialogueCall {
    /// `TopicClicked`: the topic list's `topicIndex`.
    SelectTopic(usize),
    Skip,
    Close,
    FadeDone,
}

/// The engine action a drained host call asks for, if it is one of [`SERVED_METHODS`].
pub(crate) fn decode(call: &ScaleformHostCall) -> Option<DialogueCall> {
    Some(match call.method.as_str() {
        "TopicClicked" => match call.arguments.first() {
            Some(ScaleformValue::Number(index)) if *index >= 0.0 => DialogueCall::SelectTopic(*index as usize),
            _ => return None,
        },
        "SkipText" => DialogueCall::Skip,
        "CloseMenu" => DialogueCall::Close,
        "FadeDone" => DialogueCall::FadeDone,
        _ => return None,
    })
}

/// A cheap fingerprint of what the movie shows.
fn signature(snapshot: &DialogueTopicSnapshot) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    snapshot.speaker_name.hash(&mut hasher);
    snapshot.info_form_id.hash(&mut hasher);
    snapshot.response_text.hash(&mut hasher);
    snapshot.choices.hash(&mut hasher);
    for entry in &snapshot.topics {
        entry.topic_form_id.hash(&mut hasher);
        entry.name.hash(&mut hasher);
        entry.said.hash(&mut hasher);
    }
    hasher.finish()
}

/// The calls that present `snapshot` on the movie, in the order native code makes them.
pub(crate) fn feed_ops(snapshot: &DialogueTopicSnapshot) -> Vec<PathOp> {
    let call = |method: &str, args: Vec<ScaleformValue>| PathOp::Call(format!("{ROOT}.{method}"), args);
    let text = |value: &str| ScaleformValue::String(value.to_owned());
    let mut list = Vec::with_capacity(snapshot.topics.len() * 3 + 1);
    for (index, entry) in snapshot.topics.iter().enumerate() {
        list.push(text(&entry.name));
        // A topic the NPC has not said yet is drawn as new (white), a said one greyed.
        list.push(ScaleformValue::Bool(!entry.said));
        list.push(ScaleformValue::Number(index as f64));
    }
    list.push(ScaleformValue::Number(-1.0));
    let subtitle = if snapshot.response_text.trim().is_empty() {
        call("HideDialogueText", Vec::new())
    } else {
        call("ShowDialogueText", vec![text(&snapshot.response_text)])
    };
    vec![
        call("SetSpeakerName", vec![text(&snapshot.speaker_name)]),
        subtitle,
        call("PopulateDialogueLists", list),
        call("DoShowDialogueList", vec![ScaleformValue::Bool(true), ScaleformValue::Bool(false)]),
    ]
}

impl DialogueMovie {
    pub(crate) fn new(layer: String) -> Self {
        Self { layer, signature: None, notified_info: None, closing: false, closing_frames: 0 }
    }

    /// Present `snapshot` when it changed since the last feed.
    pub(crate) fn feed(&mut self, layer: &mut UiLayer, snapshot: &DialogueTopicSnapshot) {
        let signature = signature(snapshot);
        if self.signature == Some(signature) {
            return;
        }
        self.signature = Some(signature);
        let mut ops = feed_ops(snapshot);
        if self.notified_info != Some(snapshot.info_form_id) {
            self.notified_info = Some(snapshot.info_form_id);
            ops.push(PathOp::Call(format!("{ROOT}.OnVoiceReady"), Vec::new()));
        }
        for op in ops {
            layer.path_op(op, WhenAbsent::Queue);
        }
    }
}

/// Apply one of the movie's requests to the conversation with `npc`; `true` when the movie is
/// done (`FadeDone`) and its layer should close.
pub(crate) fn apply(world: &World, npc: Option<EntityId>, snapshot: Option<&DialogueTopicSnapshot>, call: DialogueCall) -> bool {
    match call {
        DialogueCall::SelectTopic(index) => {
            let (Some(npc), Some(entry)) = (npc, snapshot.and_then(|s| s.topics.get(index))) else {
                log::warn!("dialogue movie: TopicClicked {index} matches no offered topic");
                return false;
            };
            if let Err(error) = crate::systems::npc_dialogue::select_topic_by_form_id(world, npc, entry.topic_form_id) {
                log::warn!("dialogue movie: TopicClicked {index} was unavailable: {error}");
            }
            false
        }
        DialogueCall::Skip => {
            if let Some(npc) = npc {
                crate::systems::dialogue_voice::stop_line_voice(world, npc);
                let now = world.try_resource::<byroredux_core::ecs::resources::TotalTime>().map(|time| time.0).unwrap_or_default();
                if let Some(mut surface) = world.try_resource_mut::<crate::systems::npc_dialogue::DialogueSurfaceState>() {
                    surface.line_ends_at = surface.line_ends_at.map(|ends| ends.min(now));
                }
            }
            false
        }
        DialogueCall::Close => {
            crate::systems::npc_dialogue::end_open_conversation(world);
            false
        }
        DialogueCall::FadeDone => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_native_ui::DialogueTopicEntryView;
    use byroredux_ui::{HostClass, ScaleformHostDispatch, ScaleformProfile};

    fn snapshot(said_second: bool) -> DialogueTopicSnapshot {
        DialogueTopicSnapshot {
            npc: 1,
            speaker_name: "Hulda".into(),
            topic_form_id: 0x10,
            topic_name: "Greeting".into(),
            info_form_id: 0x20,
            response_text: "What'll it be?".into(),
            topics: vec![
                DialogueTopicEntryView { topic_form_id: 0x30, name: "Rent a room".into(), said: false },
                DialogueTopicEntryView { topic_form_id: 0x31, name: "Rumors".into(), said: said_second },
            ],
            speaking: true,
            voiced: true,
            choices: false,
        }
    }

    /// The movie is fed the way native code feeds it: speaker, subtitle, the topic triples (text,
    /// new, index) closed by `-1`, then the list shown with its exit button.
    #[test]
    fn the_feed_follows_the_movies_callback_shapes() {
        let ops = feed_ops(&snapshot(true));
        let text = |s: &str| ScaleformValue::String(s.into());
        assert_eq!(ops[0], PathOp::Call(format!("{ROOT}.SetSpeakerName"), vec![text("Hulda")]));
        assert_eq!(ops[1], PathOp::Call(format!("{ROOT}.ShowDialogueText"), vec![text("What'll it be?")]));
        assert_eq!(
            ops[2],
            PathOp::Call(
                format!("{ROOT}.PopulateDialogueLists"),
                vec![
                    text("Rent a room"),
                    ScaleformValue::Bool(true),
                    ScaleformValue::Number(0.0),
                    text("Rumors"),
                    ScaleformValue::Bool(false),
                    ScaleformValue::Number(1.0),
                    ScaleformValue::Number(-1.0),
                ]
            )
        );
        assert_eq!(ops[3], PathOp::Call(format!("{ROOT}.DoShowDialogueList"), vec![ScaleformValue::Bool(true), ScaleformValue::Bool(false)]));
        let mut silent = snapshot(false);
        silent.response_text = " ".into();
        assert_eq!(feed_ops(&silent)[1], PathOp::Call(format!("{ROOT}.HideDialogueText"), Vec::new()));
    }

    #[test]
    fn host_calls_decode_to_conversation_actions() {
        let call = |method: &str, arguments: Vec<ScaleformValue>| ScaleformHostCall {
            sequence: 0,
            profile: ScaleformProfile::SkyrimAvm1,
            transport_method: method.into(),
            method: method.into(),
            host_object: None,
            request_id: Some(1),
            arguments,
            dispatch: ScaleformHostDispatch::Queued,
            class: HostClass::Served,
        };
        assert_eq!(decode(&call("TopicClicked", vec![ScaleformValue::Number(1.0)])), Some(DialogueCall::SelectTopic(1)));
        assert_eq!(decode(&call("TopicClicked", Vec::new())), None);
        assert_eq!(decode(&call("SkipText", Vec::new())), Some(DialogueCall::Skip));
        assert_eq!(decode(&call("CloseMenu", Vec::new())), Some(DialogueCall::Close));
        assert_eq!(decode(&call("FadeDone", Vec::new())), Some(DialogueCall::FadeDone));
        assert_eq!(decode(&call("PlaySound", Vec::new())), None);
        // Every served method is a Skyrim catalog command the movie calls.
        let catalog = byroredux_ui::ScaleformHostCatalog::for_profile(ScaleformProfile::SkyrimAvm1);
        for method in SERVED_METHODS {
            assert_eq!(catalog.behaviour(method).map(|b| b.target), Some(HostClass::Served), "{method}");
        }
    }
}
