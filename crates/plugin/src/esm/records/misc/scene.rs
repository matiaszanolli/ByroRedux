//! Skyrim+ `SCEN` records — authored cinematic/dialogue orchestration.
//!
//! A scene is an ordered set of condition-gated phases. Actor aliases take
//! part through dialogue, package, or timer actions whose start/end phase
//! ranges may overlap. `SCEN` is marker-delimited rather than count-delimited:
//! `HNAM` opens/closes phases and non-empty/empty `ANAM` records open/close
//! actions. The state machine below preserves those boundaries instead of
//! flattening repeated subrecord names.
//!
//! Layout source: xEdit's Skyrim definition (`wbRecord(SCEN, ...)` and
//! `wbScriptFragmentsScen`) in `Core/wbDefinitionsTES5.pas`, cross-checked
//! against all 1,706 vanilla Skyrim SE scenes with
//! `examples/dump_scen_subs.rs`.

use super::super::common::read_zstring;
use super::super::condition::{push_ctda, ConditionList};
use super::super::script_instance::{
    parse_scene_fragments, SceneScriptFragment, ScriptInstanceData,
};
use crate::esm::reader::{FormIdRemap, SubRecord};
use crate::esm::sub_reader::SubReader;

/// Scene-level flags from the first `FNAM` subrecord.
pub const SCENE_BEGIN_ON_QUEST_START: u32 = 1 << 0;
pub const SCENE_STOP_QUEST_ON_END: u32 = 1 << 1;
pub const SCENE_SHOW_ALL_TEXT: u32 = 1 << 2;
pub const SCENE_REPEAT_CONDITIONS: u32 = 1 << 3;
pub const SCENE_INTERRUPTIBLE: u32 = 1 << 4;

/// One condition-gated phase in authored order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScenePhase {
    pub name: String,
    /// Conditions before the first `NEXT` marker.
    pub start_conditions: ConditionList,
    /// Conditions between the first and second `NEXT` markers.
    pub completion_conditions: ConditionList,
    /// Creation Kit editor width (`WNAM`); retained for lossless tooling.
    pub editor_width: u32,
}

/// One actor slot participating in the scene. `actor_id` addresses a quest
/// alias, not a FormID; the scene's `quest_form_id` identifies the owner.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneActor {
    pub actor_id: u32,
    /// `LNAM`: bit 0 = no player activation, bit 1 = optional.
    pub flags: u32,
    /// `DNAM`: death/combat/dialogue pause/end behavior.
    pub behavior_flags: u32,
}

/// The three action kinds authored by Skyrim's Creation Kit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SceneActionType {
    #[default]
    Dialogue,
    Package,
    Timer,
    Unknown(u16),
}

impl SceneActionType {
    fn from_raw(raw: u16) -> Self {
        match raw {
            0 => Self::Dialogue,
            1 => Self::Package,
            2 => Self::Timer,
            other => Self::Unknown(other),
        }
    }
}

/// One action spanning an inclusive authored phase range.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SceneAction {
    pub action_type: SceneActionType,
    pub name: String,
    /// Quest-alias actor id. `-1` means no actor.
    pub actor_id: i32,
    /// Creation Kit action index. Indices can be sparse; vector position is
    /// therefore not a substitute.
    pub index: u32,
    pub flags: u32,
    pub start_phase: u32,
    pub end_phase: u32,
    /// Timer action duration (the second `SNAM`, interpreted as `f32`).
    pub timer_seconds: Option<f32>,
    /// Package action stack (`PNAM` FormIDs), in authored order.
    pub packages: Vec<u32>,
    /// Dialogue topic (`DATA` FormID).
    pub topic_form_id: Option<u32>,
    pub headtrack_actor_id: Option<i32>,
    pub looping_max: Option<f32>,
    pub looping_min: Option<f32>,
    pub emotion_type: u32,
    pub emotion_value: u32,
}

/// Parsed Skyrim+ scene definition. Runtime progress intentionally lives in
/// the scripting/ECS layer; this is immutable authored data.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScenRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub flags: u32,
    pub phases: Vec<ScenePhase>,
    pub actors: Vec<SceneActor>,
    pub actions: Vec<SceneAction>,
    pub quest_form_id: Option<u32>,
    pub last_action_index: Option<u32>,
    pub conditions: ConditionList,
    pub script_instance: Option<ScriptInstanceData>,
    pub fragments: Vec<SceneScriptFragment>,
}

#[derive(Default)]
struct OpenPhase {
    phase: ScenePhase,
    next_markers: u8,
}

fn remap_form_id(raw: u32, remap: &Option<FormIdRemap>) -> u32 {
    if raw == 0 {
        0
    } else {
        remap.as_ref().map_or(raw, |mapping| mapping.remap(raw))
    }
}

fn read_u16(sub: &SubRecord) -> u16 {
    SubReader::new(&sub.data).u16_or_default()
}

fn read_u32(sub: &SubRecord) -> u32 {
    SubReader::new(&sub.data).u32_or_default()
}

fn read_i32(sub: &SubRecord) -> i32 {
    SubReader::new(&sub.data).i32_or_default()
}

fn read_f32(sub: &SubRecord) -> f32 {
    SubReader::new(&sub.data).f32_or_default()
}

/// Parse one marker-delimited `SCEN` record.
pub fn parse_scen(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> ScenRecord {
    let mut out = ScenRecord {
        form_id,
        ..Default::default()
    };
    let mut phase: Option<OpenPhase> = None;
    let mut actor: Option<SceneActor> = None;
    let mut action: Option<SceneAction> = None;
    let mut start_phase_seen = false;
    let mut phase_section_finished = false;
    let mut action_section_started = false;

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"VMAD" => {
                out.fragments = parse_scene_fragments(&sub.data);
                out.script_instance = Some(ScriptInstanceData::parse(&sub.data));
            }
            // HNAM alternates open/close. Consecutive HNAMs are the end of
            // one phase followed by the start of the next.
            b"HNAM" if !phase_section_finished && !action_section_started => {
                if let Some(open) = phase.take() {
                    out.phases.push(open.phase);
                } else {
                    phase = Some(OpenPhase::default());
                }
            }
            b"NEXT" if phase.is_some() => {
                let open = phase.as_mut().expect("checked is_some");
                open.next_markers = open.next_markers.saturating_add(1);
            }
            b"WNAM" if phase.is_some() => {
                phase.as_mut().expect("checked is_some").phase.editor_width = read_u32(sub);
            }
            b"NAM0" if action.is_some() => {
                action.as_mut().expect("checked is_some").name = read_zstring(&sub.data);
            }
            b"NAM0" if phase.is_some() => {
                phase.as_mut().expect("checked is_some").phase.name = read_zstring(&sub.data);
            }
            b"CTDA" if phase.is_some() => {
                let open = phase.as_mut().expect("checked is_some");
                if open.next_markers == 0 {
                    push_ctda(sub, remap, &mut open.phase.start_conditions);
                } else {
                    push_ctda(sub, remap, &mut open.phase.completion_conditions);
                }
            }
            // Non-empty ANAM opens an action; empty ANAM closes it.
            b"ANAM" if sub.data.len() >= 2 => {
                phase_section_finished = true;
                action_section_started = true;
                if let Some(open) = phase.take() {
                    out.phases.push(open.phase);
                }
                if let Some(open) = actor.take() {
                    out.actors.push(open);
                }
                if let Some(open) = action.take() {
                    // Gracefully recover a malformed record missing its end
                    // marker instead of silently dropping the prior action.
                    out.actions.push(open);
                }
                action = Some(SceneAction {
                    action_type: SceneActionType::from_raw(read_u16(sub)),
                    actor_id: -1,
                    ..Default::default()
                });
                start_phase_seen = false;
            }
            b"ANAM" if sub.data.is_empty() => {
                if let Some(open) = action.take() {
                    out.actions.push(open);
                }
                start_phase_seen = false;
            }
            b"ALID" if action.is_some() => {
                action.as_mut().expect("checked is_some").actor_id = read_i32(sub);
            }
            b"ALID" => {
                phase_section_finished = true;
                if let Some(open) = phase.take() {
                    out.phases.push(open.phase);
                }
                if let Some(open) = actor.take() {
                    out.actors.push(open);
                }
                actor = Some(SceneActor {
                    actor_id: read_u32(sub),
                    ..Default::default()
                });
            }
            b"LNAM" if actor.is_some() && action.is_none() => {
                actor.as_mut().expect("checked is_some").flags = read_u32(sub);
            }
            b"DNAM" if actor.is_some() && action.is_none() => {
                actor.as_mut().expect("checked is_some").behavior_flags = read_u32(sub);
            }
            b"FNAM" if action.is_some() => {
                action.as_mut().expect("checked is_some").flags = read_u32(sub);
            }
            b"FNAM" if !phase_section_finished => out.flags = read_u32(sub),
            b"INAM" if action.is_some() => {
                action.as_mut().expect("checked is_some").index = read_u32(sub);
            }
            b"INAM" => out.last_action_index = Some(read_u32(sub)),
            b"SNAM" if action.is_some() && !start_phase_seen => {
                action.as_mut().expect("checked is_some").start_phase = read_u32(sub);
                start_phase_seen = true;
            }
            b"SNAM" if action.is_some() => {
                // Only Timer actions author the second SNAM, but preserving
                // it for an unknown future action type is safer than dropping
                // a well-formed scalar.
                action.as_mut().expect("checked is_some").timer_seconds = Some(read_f32(sub));
            }
            b"ENAM" if action.is_some() => {
                action.as_mut().expect("checked is_some").end_phase = read_u32(sub);
            }
            b"PNAM" if action.is_some() => {
                let package = remap_form_id(read_u32(sub), remap);
                if package != 0 {
                    action
                        .as_mut()
                        .expect("checked is_some")
                        .packages
                        .push(package);
                }
            }
            b"PNAM" => {
                let quest = remap_form_id(read_u32(sub), remap);
                out.quest_form_id = (quest != 0).then_some(quest);
            }
            b"DATA" if action.is_some() => {
                let topic = remap_form_id(read_u32(sub), remap);
                action.as_mut().expect("checked is_some").topic_form_id =
                    (topic != 0).then_some(topic);
            }
            b"HTID" if action.is_some() => {
                let actor_id = read_i32(sub);
                action.as_mut().expect("checked is_some").headtrack_actor_id =
                    (actor_id >= 0).then_some(actor_id);
            }
            b"DMAX" if action.is_some() => {
                action.as_mut().expect("checked is_some").looping_max = Some(read_f32(sub));
            }
            b"DMIN" if action.is_some() => {
                action.as_mut().expect("checked is_some").looping_min = Some(read_f32(sub));
            }
            b"DEMO" if action.is_some() => {
                action.as_mut().expect("checked is_some").emotion_type = read_u32(sub);
            }
            b"DEVA" if action.is_some() => {
                action.as_mut().expect("checked is_some").emotion_value = read_u32(sub);
            }
            b"CTDA" if action.is_none() => push_ctda(sub, remap, &mut out.conditions),
            _ => {}
        }
    }

    if let Some(open) = phase {
        out.phases.push(open.phase);
    }
    if let Some(open) = actor {
        out.actors.push(open);
    }
    if let Some(open) = action {
        out.actions.push(open);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(kind: &[u8; 4], data: impl Into<Vec<u8>>) -> SubRecord {
        SubRecord {
            sub_type: *kind,
            data: data.into(),
        }
    }

    fn ctda(function: u32) -> Vec<u8> {
        let mut data = vec![0; 32];
        data[4..8].copy_from_slice(&1.0f32.to_le_bytes());
        data[8..12].copy_from_slice(&function.to_le_bytes());
        data
    }

    #[test]
    fn parses_marker_delimited_phase_actor_and_dialogue_action() {
        let subs = vec![
            sub(b"EDID", b"MQ101Scene1\0".to_vec()),
            sub(b"FNAM", SCENE_BEGIN_ON_QUEST_START.to_le_bytes()),
            sub(b"HNAM", vec![]),
            sub(b"NAM0", b"Load the carts\0".to_vec()),
            sub(b"CTDA", ctda(58)),
            sub(b"NEXT", vec![]),
            sub(b"CTDA", ctda(59)),
            sub(b"NEXT", vec![]),
            sub(b"WNAM", 200u32.to_le_bytes()),
            sub(b"HNAM", vec![]),
            sub(b"ALID", 12u32.to_le_bytes()),
            sub(b"LNAM", 1u32.to_le_bytes()),
            sub(b"DNAM", 26u32.to_le_bytes()),
            sub(b"ANAM", 0u16.to_le_bytes()),
            sub(b"NAM0", b"Ralof speaks\0".to_vec()),
            sub(b"ALID", 12i32.to_le_bytes()),
            sub(b"INAM", 7u32.to_le_bytes()),
            sub(b"FNAM", (1u32 << 15).to_le_bytes()),
            sub(b"SNAM", 0u32.to_le_bytes()),
            sub(b"ENAM", 0u32.to_le_bytes()),
            sub(b"DATA", 0x000B_EC99u32.to_le_bytes()),
            sub(b"HTID", (-1i32).to_le_bytes()),
            sub(b"DMAX", 10.0f32.to_le_bytes()),
            sub(b"DMIN", 1.0f32.to_le_bytes()),
            sub(b"DEMO", 2u32.to_le_bytes()),
            sub(b"DEVA", 60u32.to_le_bytes()),
            sub(b"ANAM", vec![]),
            sub(b"PNAM", 0x0003_372bu32.to_le_bytes()),
            sub(b"INAM", 7u32.to_le_bytes()),
        ];

        let scene = parse_scen(0x000B_ECD4, &subs, &None);
        assert_eq!(scene.editor_id, "MQ101Scene1");
        assert_eq!(scene.flags, SCENE_BEGIN_ON_QUEST_START);
        assert_eq!(scene.quest_form_id, Some(0x0003_372b));
        assert_eq!(scene.last_action_index, Some(7));
        assert_eq!(scene.phases.len(), 1);
        assert_eq!(scene.phases[0].name, "Load the carts");
        assert_eq!(scene.phases[0].start_conditions[0].function_index, 58);
        assert_eq!(scene.phases[0].completion_conditions[0].function_index, 59);
        assert_eq!(
            scene.actors,
            vec![SceneActor {
                actor_id: 12,
                flags: 1,
                behavior_flags: 26
            }]
        );
        assert_eq!(scene.actions.len(), 1);
        let action = &scene.actions[0];
        assert_eq!(action.action_type, SceneActionType::Dialogue);
        assert_eq!(action.name, "Ralof speaks");
        assert_eq!(action.actor_id, 12);
        assert_eq!(action.index, 7);
        assert_eq!(action.topic_form_id, Some(0x000B_EC99));
        assert_eq!(action.headtrack_actor_id, None);
        assert_eq!(action.looping_max, Some(10.0));
        assert_eq!(action.emotion_value, 60);
    }

    #[test]
    fn parses_package_stack_and_timer_seconds() {
        let subs = vec![
            sub(b"ANAM", 1u16.to_le_bytes()),
            sub(b"INAM", 3u32.to_le_bytes()),
            sub(b"SNAM", 1u32.to_le_bytes()),
            sub(b"ENAM", 4u32.to_le_bytes()),
            sub(b"PNAM", 0x111u32.to_le_bytes()),
            sub(b"PNAM", 0x222u32.to_le_bytes()),
            sub(b"ANAM", vec![]),
            sub(b"ANAM", 2u16.to_le_bytes()),
            sub(b"INAM", 4u32.to_le_bytes()),
            sub(b"SNAM", 5u32.to_le_bytes()),
            sub(b"ENAM", 5u32.to_le_bytes()),
            sub(b"SNAM", 2.5f32.to_le_bytes()),
            sub(b"ANAM", vec![]),
        ];
        let scene = parse_scen(1, &subs, &None);
        assert_eq!(scene.actions.len(), 2);
        assert_eq!(scene.actions[0].action_type, SceneActionType::Package);
        assert_eq!(scene.actions[0].packages, vec![0x111, 0x222]);
        assert_eq!(scene.actions[1].action_type, SceneActionType::Timer);
        assert_eq!(scene.actions[1].timer_seconds, Some(2.5));
    }

    #[test]
    fn malformed_missing_action_end_marker_keeps_both_actions() {
        let subs = vec![
            sub(b"ANAM", 0u16.to_le_bytes()),
            sub(b"INAM", 1u32.to_le_bytes()),
            sub(b"ANAM", 2u16.to_le_bytes()),
            sub(b"INAM", 2u32.to_le_bytes()),
        ];
        let scene = parse_scen(1, &subs, &None);
        assert_eq!(scene.actions.len(), 2);
        assert_eq!(scene.actions[0].index, 1);
        assert_eq!(scene.actions[1].index, 2);
    }
}
