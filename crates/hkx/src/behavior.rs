//! Havok Behavior data for placed objects (M47.2): the project, character and
//! behavior-graph packfiles that drive an animated prop (a lever, a door, a trap).
//!
//! A NIF with a `BSBehaviorGraphExtraData` names a *project* file. The project
//! lists a *character* file; the character names its skeleton, its animation
//! clips and a *behavior* file; the behavior is a state machine whose
//! transitions fire on named events and whose states play one clip each.
//! `ObjectReference.PlayAnimation(event)` sends such an event.
//!
//! Only what that needs is decoded: the event names, the (root) state machine's
//! states with their transitions and clip, and each clip's trigger events. Blend
//! trees, variables and transition effects are not executed (a transition is
//! instantaneous). The class layouts are Havok 2010.2 (`hk_2010.2.0-r1`),
//! 64-bit: the Havok SDK on disk carries no Behavior headers, so every offset
//! below was derived from, and checked against, the strings and ids in vanilla
//! Skyrim Special Edition lever, door and trap graphs (`examples/probe_behavior.rs`).
//! A 32-bit (Skyrim LE) packfile is reported as an unsupported layout.

use crate::expression::Condition;
use crate::packfile::Packfile;
use std::sync::Arc;
use crate::{HkxError, Result};

/// Sanity caps on array sizes read from a file.
const MAX_EVENTS: usize = 4096;
const MAX_STATES: usize = 1024;
const MAX_TRANSITIONS: usize = 4096;
const MAX_TRIGGERS: usize = 1024;
const MAX_VARIABLES: usize = 4096;

/// `hkbVariableValueSet::m_wordVariableValues` (`hkArray<hkInt32>`): the initial value
/// of every variable that fits a word (bool, int8/16/32, real), in variable order.
const VALUE_SET_WORDS: usize = 0x10;
/// `hkbBehaviorGraphData::m_variableInfos` element: `hkbRoleAttribute` (role, flags: two
/// `hkInt16`), then the `VariableType` byte, padded to 6 bytes.
const VARIABLE_INFO_STRIDE: usize = 6;
const VARIABLE_INFO_TYPE: usize = 4;
/// `hkbBindable::m_variableBindingSet` and its `m_bindings` array; a binding names the
/// member (`blendParameter`, `playbackSpeed`, ...) a variable drives.
const BINDABLE_BINDING_SET: usize = 0x10;
const BINDING_SET_BINDINGS: usize = 0x10;
const BINDING_STRIDE: usize = 0x28;
const BINDING_MEMBER_PATH: usize = 0x00;
const BINDING_VARIABLE_INDEX: usize = 0x1C;
/// `hkbStateMachineTransitionInfoArray` element: the transition effect and condition
/// pointers; `hkbExpressionCondition::m_expression` / `hkbStringCondition::m_conditionString`.
const TRANSITION_EFFECT: usize = 0x20;
const TRANSITION_CONDITION: usize = 0x28;
const CONDITION_TEXT: usize = 0x10;
/// `hkbBlenderGeneratorChild::m_weight` (the child's position on a parametric blender's
/// axis, or its weight) and the `hkbBindable` base of its fields.
const BLENDER_CHILD_WEIGHT: usize = 0x40;
/// `hkbBlenderGeneratorChild::m_boneWeights`; `hkbBoneWeightArray::m_boneWeights`;
/// `hkbManualSelectorGenerator::m_selectedGeneratorIndex` (an `hkInt8` right after the
/// generators array); a binding's `offsetInArrayPlusOne`.
const BLENDER_CHILD_BONE_WEIGHTS: usize = 0x38;
const BONE_WEIGHTS: usize = 0x30;
const SELECTOR_INDEX_AFTER_ARRAY: usize = 0x10;
const BINDING_OFFSET_IN_ARRAY: usize = 0x14;
const MAX_BONES: usize = 1024;
/// Havok 2014 modifier layouts (members start at 0x58, after `hkbModifier::m_enable`): the
/// offsets are read off the vanilla graphs' objects; the member order is the engine's own.
const MODIFIER_GENERATOR_MODIFIER: usize = 0x88;
const MODIFIER_LIST_MODIFIERS: usize = 0x58;
const EVENT_DRIVEN_MODIFIER: usize = 0x58;
const EVENT_DRIVEN_ACTIVATE: usize = 0x60;
const EVENT_DRIVEN_DEACTIVATE: usize = 0x64;
const EVENT_DRIVEN_BY_DEFAULT: usize = 0x68;
const EVENT_ON_DEACTIVATE_EVENT: usize = 0x58;
const EVENTS_FROM_RANGE_INPUT: usize = 0x58;
const EVENTS_FROM_RANGE_LOWER: usize = 0x5C;
const EVENTS_FROM_RANGE_RANGES: usize = 0x60;
const EVENT_RANGES_ARRAY: usize = 0x10;
const EVENT_RANGE_STRIDE: usize = 0x20;
const EVENT_RANGE_EVENT: usize = 0x08;
const EVENT_RANGE_MODE: usize = 0x18;
const EVALUATE_EXPRESSION_EXPRESSIONS: usize = 0x58;
const EXPRESSION_ARRAY: usize = 0x10;
const EXPRESSION_STRIDE: usize = 0x18;
const EXPRESSION_MODE: usize = 0x10;
const EVERY_N_CHECK: usize = 0x58;
const EVERY_N_SEND: usize = 0x68;
const EVERY_N_COUNT: usize = 0x78;
const TIMER_ALARM: usize = 0x58;
const TIMER_EVENT: usize = 0x60;
const RANDOM_ALARM_MIN: usize = 0x58;
const RANDOM_ALARM_MAX: usize = 0x5C;
const RANDOM_ALARM_EVENT: usize = 0x60;
const RANDOM_ALARM_RESET: usize = 0x70;
const MODIFY_ONCE_ON_ACTIVATE: usize = 0x60;
const MODIFY_ONCE_ON_DEACTIVATE: usize = 0x68;
/// `BSAssignVariablesModifier`: 20 `(floatVariableN, floatValueN)` pairs from 0x58, then 4
/// `(intVariableN, intValueN)` pairs; the variable of a pair is named by a binding.
const ASSIGN_FLOATS: usize = 0x58;
const ASSIGN_INTS: usize = 0xF8;
const ASSIGN_PAIR: usize = 8;
const MAX_MODIFIER_DEPTH: usize = 16;
/// `hkbBlenderGenerator::m_flags`.
const BLENDER_FLAG_SYNC: u16 = 0x01;
const BLENDER_FLAG_PARAMETRIC: u16 = 0x10;
const BLENDER_FLAG_CYCLIC: u16 = 0x20;

// ── 64-bit field offsets ──────────────────────────────────────────────

/// One Havok version's class layouts. Skyrim SE's are `hk_2010.2.0-r1`, Fallout 4's
/// `hk_2014.1.0-r1`; both are 64-bit MSVC. The 2014 `hkbNode` grew by 0x40 bytes, which
/// moves every member of the generator classes (graph, state machine, blender) by
/// that much; `hkbBindable`-derived classes (state info, blender child) and the
/// plain data classes did not move. The clip generator moved by 0x48, then 0x4C
/// after its trigger array (one more 4-byte member before the start time).
struct Layout {
    /// `hkbBehaviorGraphStringData::m_eventNames` / `m_variableNames`
    /// (`hkArray<hkStringPtr>`).
    string_data_event_names: usize,
    string_data_variable_names: usize,
    /// `hkbBehaviorGraphData::m_variableInfos` and `m_variableInitialValues` (an
    /// `hkbVariableValueSet`, whose word array is at [`VALUE_SET_WORDS`]).
    graph_data_variable_infos: usize,
    graph_data_initial_values: usize,
    /// `hkbBehaviorGraph::m_rootGenerator` / `m_data`.
    graph_root_generator: usize,
    graph_data: usize,
    /// `hkbBehaviorGraphData::m_stringData`.
    graph_data_string_data: usize,
    /// `hkbStateMachine::m_startStateId`, `m_states`, `m_wildcardTransitions`.
    machine_start_state: usize,
    machine_states: usize,
    machine_wildcard: usize,
    /// `hkbStateMachine::StateInfo` fields.
    state_transitions: usize,
    state_generator: usize,
    state_name: usize,
    state_id: usize,
    /// `hkbStateMachineTransitionInfoArray::m_transitions` and its element layout.
    transitions_array: usize,
    transition_stride: usize,
    transition_event_id: usize,
    transition_to_state: usize,
    transition_priority: usize,
    transition_flags: usize,
    /// `hkbBlenderGenerator::m_blendParameter`, `m_minCyclicBlendParameter`,
    /// `m_maxCyclicBlendParameter` and `m_flags`.
    blender_parameter: usize,
    blender_min_cyclic: usize,
    blender_max_cyclic: usize,
    blender_flags: usize,
    /// `hkbBlendingTransitionEffect::m_duration`.
    effect_duration: usize,
    /// `hkbBlenderGenerator::m_children` and `hkbBlenderGeneratorChild::m_generator`.
    blender_children: usize,
    blender_child_generator: usize,
    /// `BGSGamebryoSequenceGenerator::m_pSequence` (`hkStringPtr`).
    sequence_name: usize,
    /// The inner generator of the wrapper generators (`hkbModifierGenerator`,
    /// `BSiStateTaggingGenerator`, `DynamicAnimationTaggingGenerator`,
    /// `BSOffsetAnimationGenerator`, `BSCyclicBlendTransitionGenerator`). Only
    /// measured for Havok 2014; `None` leaves them unfollowed.
    wrapped_generator: Option<usize>,
    /// `hkbManualSelectorGenerator::m_generators` and
    /// `hkbBehaviorReferenceGenerator::m_behaviorName` (a sub-behavior packfile).
    selector_generators: Option<usize>,
    reference_behavior_name: Option<usize>,
    /// `hkbClipGenerator` fields.
    clip_animation_name: usize,
    clip_triggers: usize,
    clip_start_time: usize,
    clip_playback_speed: usize,
    clip_user_fraction: usize,
    clip_mode: usize,
}

const LAYOUT_2010: Layout = Layout {
    string_data_event_names: 0x10,
    string_data_variable_names: 0x30,
    graph_data_variable_infos: 0x20,
    graph_data_initial_values: 0x70,
    graph_root_generator: 0x80,
    graph_data: 0x88,
    graph_data_string_data: 0x78,
    machine_start_state: 0x68,
    machine_states: 0x90,
    machine_wildcard: 0xA0,
    state_transitions: 0x50,
    state_generator: 0x58,
    state_name: 0x60,
    state_id: 0x68,
    transitions_array: 0x10,
    transition_stride: 72,
    transition_event_id: 0x30,
    transition_to_state: 0x34,
    transition_priority: 0x40,
    transition_flags: 0x42,
    blender_parameter: 0x4C,
    blender_min_cyclic: 0x50,
    blender_max_cyclic: 0x54,
    blender_flags: 0x5A,
    effect_duration: 0x50,
    blender_children: 0x60,
    blender_child_generator: 0x30,
    sequence_name: 0x48,
    wrapped_generator: None,
    selector_generators: None,
    reference_behavior_name: None,
    clip_animation_name: 0x48,
    clip_triggers: 0x50,
    clip_start_time: 0x60,
    clip_playback_speed: 0x64,
    clip_user_fraction: 0x6C,
    clip_mode: 0x72,
};

/// Derived from the vanilla Fallout 4 graphs (`Fallout4 - Animations.ba2`) the same
/// way as the 2010 table (`examples/probe_behavior.rs`): `StateInfo` and
/// `BlenderGeneratorChild` kept their offsets, the generator classes moved by
/// 0x40, the transition element kept its 72 bytes (the array object just starts its
/// elements 0x10 later), and the clip generator moved by 0x48 up to its trigger array and
/// 0x4C after it. Fallout 4 has no `BGSGamebryoSequenceGenerator`.
const LAYOUT_2014: Layout = Layout {
    string_data_event_names: 0x10,
    string_data_variable_names: 0x30,
    graph_data_variable_infos: 0x20,
    graph_data_initial_values: 0x60,
    graph_root_generator: 0xC0,
    graph_data: 0xC8,
    graph_data_string_data: 0x68,
    machine_start_state: 0xA8,
    machine_states: 0xD0,
    machine_wildcard: 0xE0,
    state_transitions: 0x50,
    state_generator: 0x58,
    state_name: 0x60,
    state_id: 0x68,
    transitions_array: 0x10,
    transition_stride: 72,
    transition_event_id: 0x30,
    transition_to_state: 0x34,
    transition_priority: 0x40,
    transition_flags: 0x42,
    blender_parameter: 0x8C,
    blender_min_cyclic: 0x90,
    blender_max_cyclic: 0x94,
    blender_flags: 0x9A,
    effect_duration: 0xA8,
    blender_children: 0xA0,
    blender_child_generator: 0x30,
    sequence_name: 0,
    wrapped_generator: Some(0x90),
    selector_generators: Some(0x88),
    reference_behavior_name: Some(0x88),
    clip_animation_name: 0x90,
    clip_triggers: 0x98,
    clip_start_time: 0xAC,
    clip_playback_speed: 0xB0,
    clip_user_fraction: 0xB8,
    clip_mode: 0xBE,
};

fn layout(file: &Packfile) -> &'static Layout {
    if file.version() == 11 {
        &LAYOUT_2014
    } else {
        &LAYOUT_2010
    }
}

/// Nested generators are followed this deep (a state machine in a blender in a
/// state machine ...).
const MAX_GENERATOR_DEPTH: usize = 32;

/// The most steps one behavior-graph decode may take: an object entered counts one,
/// once for every pointer that reaches it (a shared generator is read again from each
/// parent, because each read builds its own owned subtree), and so does every entry of
/// a child array it walks, null or not. The depth caps bound how deep the readers go,
/// not how much they build: a few kilobytes of child pointers that fan out to shared
/// descendants multiply per level.
///
/// The shipped graphs need at most 115,605 steps by `behavior_graph_steps` (Fallout 4's
/// `_1stperson\behaviors\gunbehavior.hkx`, across the base game and every DLC and Creation
/// archive; Skyrim SE's largest takes 353), and that one decodes in 27 ms, about 230 ns
/// a step. The cap is about 8.6x the shipped maximum, so a graph at the cap costs about a
/// quarter of a second to refuse.
const MAX_GRAPH_STEPS: usize = 1_000_000;

/// A graph's event and variable names by position, so inlining a sub-behavior looks each of
/// its names up in a hash map instead of scanning the whole table (up to 4,096 events a
/// graph, 64 sub-behaviors: tens of billions of comparisons the other way). Names compare
/// ASCII-case-insensitively and the first of two equal names wins, as `event_id` and
/// `variable_id` do.
struct Interned {
    events: std::collections::HashMap<String, i32>,
    variables: std::collections::HashMap<String, usize>,
}

impl Interned {
    fn of(graph: &BehaviorGraph) -> Self {
        let mut events = std::collections::HashMap::with_capacity(graph.events.len());
        for (index, name) in graph.events.iter().enumerate() {
            events.entry(name.to_ascii_lowercase()).or_insert(index as i32);
        }
        let mut variables = std::collections::HashMap::with_capacity(graph.variables.len());
        for (index, variable) in graph.variables.iter().enumerate() {
            variables.entry(variable.name.to_ascii_lowercase()).or_insert(index);
        }
        Self { events, variables }
    }
}

/// What one decode has walked: the objects it is inside right now (a pointer back
/// to one of them is a cycle, which no generator, state machine or modifier tree
/// legitimately has) and how many steps it has taken in all.
struct Walk {
    active: Vec<usize>,
    steps: usize,
    limit: usize,
}

impl Walk {
    fn new(limit: usize) -> Self {
        Self { active: Vec::new(), steps: 0, limit }
    }

    /// Count `steps` of work against the budget.
    fn charge(&mut self, steps: usize) -> Result<()> {
        self.steps = self.steps.saturating_add(steps);
        if self.steps > self.limit {
            return Err(HkxError::InvalidData("behavior graph takes too many steps to read"));
        }
        Ok(())
    }

    /// Step into `object`; pair every `Ok` with a [`leave`](Self::leave).
    fn enter(&mut self, object: usize) -> Result<()> {
        if self.active.contains(&object) {
            return Err(HkxError::InvalidData("behavior graph object contains itself"));
        }
        self.charge(1)?;
        self.active.push(object);
        Ok(())
    }

    fn leave(&mut self) {
        self.active.pop();
    }
}
/// `hkbClipTriggerArray::m_triggers` and its element layout.
const TRIGGERS_ARRAY: usize = 0x10;
const TRIGGER_STRIDE: usize = 32;
const TRIGGER_LOCAL_TIME: usize = 0x00;
const TRIGGER_EVENT_ID: usize = 0x08;
const TRIGGER_RELATIVE_TO_END: usize = 0x18;
const TRIGGER_IS_ANNOTATION: usize = 0x1A;
/// `hkbCharacterStringData` fields.
const CHARACTER_ANIMATION_NAMES: usize = 0x30;
const CHARACTER_RIG_NAME: usize = 0xA8;
const CHARACTER_BEHAVIOR_FILE: usize = 0xB8;
/// `hkbProjectStringData::m_characterFilenames`.
const PROJECT_CHARACTER_FILENAMES: usize = 0x30;

/// `hkbStateMachine::TransitionInfo::m_flags`: the transition never fires.
pub const TRANSITION_FLAG_DISABLED: u16 = 0x20;

/// How a state's clip plays (`hkbClipGenerator::PlaybackMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipMode {
    /// Plays once and holds its last pose.
    SinglePlay,
    Looping,
    /// Held at `user_fraction` of its length (a static pose).
    UserControlled,
    PingPong,
}

/// An event a clip raises while it plays.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipTrigger {
    /// Seconds from the clip start, or from its end when `relative_to_end`.
    pub local_time: f32,
    /// Index into [`BehaviorGraph::events`].
    pub event: i32,
    pub relative_to_end: bool,
    /// Authored on the clip's own annotation track rather than the behavior.
    pub annotation: bool,
}

/// The clip a state plays.
#[derive(Debug, Clone, PartialEq)]
pub struct StateClip {
    /// The clip packfile, relative to the project directory.
    pub animation: String,
    pub mode: ClipMode,
    pub playback_speed: f32,
    pub start_time: f32,
    /// Where a [`ClipMode::UserControlled`] clip is held, as a fraction of its length.
    pub user_fraction: f32,
    pub triggers: Vec<ClipTrigger>,
    /// The variable bound to the clip's `playbackSpeed`: it replaces `playback_speed`.
    pub playback_speed_variable: Option<usize>,
}

/// A state's (or the machine's wildcard) transition on an event.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transition {
    /// Index into [`BehaviorGraph::events`]; negative for none.
    pub event: i32,
    pub to_state: i32,
    pub priority: i16,
    pub flags: u16,
    /// Seconds the old pose fades out over after the transition fires: the blending
    /// effect's authored duration, 0 for an instant transition (or none).
    pub blend_duration: f32,
    /// The variable that supplies the duration instead, when the effect binds one.
    pub blend_duration_variable: Option<usize>,
    /// The expression over the graph's variables the event's transition is gated on.
    pub condition: Option<Arc<Condition>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub id: i32,
    pub name: String,
    pub transitions: Vec<Transition>,
    /// The clip when the state's generator is a clip generator; `None` for any
    /// other kind (see [`Self::content`] for what a blender or nested machine holds).
    pub clip: Option<StateClip>,
    /// What a blender or nested-machine generator holds, flattened.
    pub content: StateContent,
}

/// What a state's non-clip generator holds: a blender's children and nested state
/// machines are flattened into these lists (for the event schedule and the start clip);
/// [`Self::generator`] keeps the tree itself.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StateContent {
    /// `BGSGamebryoSequenceGenerator` sequence names: `NiControllerSequence`s in
    /// the object's own NIF.
    pub sequences: Vec<String>,
    /// Nested state machines. Events are answered inside them.
    pub machines: Vec<StateMachine>,
    /// Further clips of a blender (the first is the state's own `clip`).
    pub blended_clips: Vec<StateClip>,
    /// Sub-behavior packfiles an `hkbBehaviorReferenceGenerator` names (Fallout 4:
    /// a state that runs another graph, e.g. `Behaviors\\DialogueBehavior.hkx`), in the
    /// order [`Generator::Reference`] numbers them.
    pub behavior_references: Vec<String>,
    /// For each reference, the index in [`Self::machines`] of the sub-behavior's machine
    /// once `BehaviorGraph::inline_references` has loaded it.
    pub reference_machines: Vec<Option<usize>>,
    /// The state's generator tree: how its clips, blenders and nested machines combine
    /// into the pose. `None` for a state with no generator.
    pub generator: Option<Generator>,
}

/// One node of a state's generator tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Generator {
    Clip(StateClip),
    /// A Gamebryo `NiControllerSequence` of the object's own NIF.
    Sequence(String),
    /// A nested state machine: an index into the owning state's [`StateContent::machines`].
    Machine(usize),
    /// A sub-behavior: an index into [`StateContent::behavior_references`].
    Reference(usize),
    Blender(Blender),
    /// `hkbManualSelectorGenerator`: one child, picked by an index (the authored one, or
    /// its bound variable).
    Selector { index: i32, index_variable: Option<usize>, children: Vec<Generator> },
    /// `hkbModifierGenerator`: modifiers that run while the generator is active.
    Modified { modifiers: Vec<Arc<ModifierNode>>, inner: Option<Box<Generator>> },
}

/// A modifier of a graph. `id` is unique within the graph (it keys the runtime state a
/// modifier keeps: whether it was active last frame, the range it was in, its count).
#[derive(Debug, Clone, PartialEq)]
pub struct ModifierNode {
    pub id: u32,
    pub kind: Modifier,
}

/// The modifiers that change what the graph itself does - the variables it holds and the
/// events it raises. (The ones that pose the skeleton - look-at, twist, foot IK, ragdoll -
/// and `BSIsActiveModifier`, whose booleans are for the engine, are not modelled.) The
/// semantics are the engine's own description strings: "set on activate",
/// "Expression of the form 'variablename = expression' or 'eventname = boolean
/// expression'", "The event to send when this range is entered", "Event to be raised when
/// this modifier deactivates", "Number of times to get m_eventToCheckFor before sending
/// m_eventToSend", "Event used to activate the wrapped modifier".
#[derive(Debug, Clone, PartialEq)]
pub enum Modifier {
    /// `BSAssignVariablesModifier`: on activation, each variable is set to its value.
    AssignVariables { floats: Vec<(usize, f32)>, ints: Vec<(usize, i32)> },
    /// `BSModifyOnceModifier`: runs `on_activate` when it activates and `on_deactivate`
    /// when it deactivates.
    ModifyOnce { on_activate: Option<Arc<ModifierNode>>, on_deactivate: Option<Arc<ModifierNode>> },
    /// `hkbEventsFromRangeModifier`: sends the event of the range the input falls in.
    EventsFromRange { input: f32, input_variable: Option<usize>, lower_bound: f32, ranges: Vec<EventRange> },
    /// `hkbEvaluateExpressionModifier`.
    EvaluateExpression { expressions: Vec<ExpressionData> },
    /// `BSEventOnDeactivateModifier`.
    EventOnDeactivate { event: i32 },
    /// `BSEventEveryNEventsModifier`: sends `send` once `check` has been raised `count` times.
    EventEveryNEvents { check: i32, send: i32, count: u32 },
    /// `hkbEventDrivenModifier`: the wrapped modifier runs only between its activate and
    /// deactivate events (and from the start when `active_by_default`).
    EventDriven { activate: i32, deactivate: i32, active_by_default: bool, inner: Option<Arc<ModifierNode>> },
    /// `hkbTimerModifier` / `BSTimerModifier`: "An event is sent at this time."
    Timer { alarm_seconds: f32, event: i32 },
    /// `BSRandomAlarmModifier`: a timer whose time is drawn between a minimum and a maximum,
    /// and which starts over when it goes off if `reset`.
    RandomAlarm { min_seconds: f32, max_seconds: f32, event: i32, reset: bool },
    /// `BSGetTimeStepModifier` ("The current time step."): its `timeStep` member is bound
    /// to a variable (`fTimeStep`), which takes the frame's `dt` every frame.
    TimeStep { variable: Option<usize> },
    /// `hkbModifierList`.
    List(Vec<Arc<ModifierNode>>),
}

/// One range of an `hkbEventsFromRangeModifier`: from the previous range's upper bound (or
/// the modifier's lower bound) up to `upper`.
#[derive(Debug, Clone, PartialEq)]
pub struct EventRange {
    pub upper: f32,
    pub event: i32,
    /// Send every frame the input is in the range (else when it enters it).
    pub when_in_range: bool,
}

/// One expression of an `hkbEvaluateExpressionModifier`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpressionData {
    pub text: String,
    /// `EVENT_MODE_SEND_ONCE`, `SEND_ON_TRUE`, `SEND_ON_FALSE_TO_TRUE`,
    /// `SEND_EVERY_FRAME_ONCE_TRUE`: 0..=3.
    pub event_mode: u8,
}

/// An `hkbBlenderGenerator`. A *parametric* blender's child weights are positions on
/// one axis and the blend parameter picks the two neighbours to mix (in the vanilla
/// 8-way locomotion graph the children sit at 80 and 370 for `Speed`, and at
/// 0, 0.125 ... 0.875 for a cyclic direction); any other blender mixes its children by
/// their weights (the engine checks they are between 0 and 1), each scaled bone by bone
/// by the child's bone weights. `sync` blenders keep their children's clips in step.
#[derive(Debug, Clone, PartialEq)]
pub struct Blender {
    pub parametric: bool,
    pub cyclic: bool,
    pub sync: bool,
    /// The authored `blendParameter`, replaced by `parameter_variable` when bound.
    pub parameter: f32,
    pub parameter_variable: Option<usize>,
    pub min_cyclic: f32,
    pub max_cyclic: f32,
    pub children: Vec<BlendChild>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlendChild {
    pub weight: f32,
    pub weight_variable: Option<usize>,
    /// `hkbBoneWeightArray`: how much of each skeleton bone this child contributes.
    pub bone_weights: Option<BoneWeights>,
    /// A variable bound to the child's `boneWeights` member. The vanilla graphs bind
    /// `bEquipOk` and `bAllowHeadTracking` (booleans, on the children that layer a weapon
    /// pose or a head-tracking pose) and `bAnimationDriven` (on the camera machine of the
    /// root blender, which must not reach the body at its default 0): a boolean there
    /// reads as a switch. Integers and reals bound there (`iSyncIdleLocomotion`, `Speed`,
    /// `Direction`) have no meaning the files or the engine's strings give.
    pub bone_weights_variable: Option<usize>,
    pub generator: Generator,
}

/// A child's per-bone weights (indexed by skeleton bone), and the elements a variable
/// drives (`boneWeights` bindings: an upper-body mask that follows `bEquipOk`).
#[derive(Debug, Clone, PartialEq)]
pub struct BoneWeights {
    pub weights: Arc<Vec<f32>>,
    /// `(bone index, variable)` for each element a variable supplies.
    pub bound: Vec<(usize, usize)>,
}

impl Generator {
    /// Every clip of the tree (not those inside nested machines).
    fn for_each_clip_mut(&mut self, f: &mut dyn FnMut(&mut StateClip)) {
        match self {
            Self::Clip(clip) => f(clip),
            Self::Blender(blender) => blender.children.iter_mut().for_each(|child| child.generator.for_each_clip_mut(f)),
            Self::Selector { children, .. } => children.iter_mut().for_each(|child| child.for_each_clip_mut(f)),
            Self::Modified { inner, .. } => {
                if let Some(inner) = inner {
                    inner.for_each_clip_mut(f);
                }
            }
            Self::Sequence(_) | Self::Machine(_) | Self::Reference(_) => {}
        }
    }

    /// Every modifier of the tree (not those inside nested machines).
    fn for_each_modifier_mut(&mut self, f: &mut dyn FnMut(&mut Arc<ModifierNode>)) {
        match self {
            Self::Blender(blender) => blender.children.iter_mut().for_each(|child| child.generator.for_each_modifier_mut(f)),
            Self::Selector { children, .. } => children.iter_mut().for_each(|child| child.for_each_modifier_mut(f)),
            Self::Modified { modifiers, inner } => {
                modifiers.iter_mut().for_each(&mut *f);
                if let Some(inner) = inner {
                    inner.for_each_modifier_mut(f);
                }
            }
            Self::Clip(_) | Self::Sequence(_) | Self::Machine(_) | Self::Reference(_) => {}
        }
    }

    /// Every variable index the tree names.
    fn for_each_variable_mut(&mut self, f: &mut dyn FnMut(&mut Option<usize>)) {
        match self {
            Self::Clip(clip) => f(&mut clip.playback_speed_variable),
            Self::Blender(blender) => {
                f(&mut blender.parameter_variable);
                for child in &mut blender.children {
                    f(&mut child.weight_variable);
                    f(&mut child.bone_weights_variable);
                    if let Some(bones) = &mut child.bone_weights {
                        for (_, variable) in &mut bones.bound {
                            let mut wrapped = Some(*variable);
                            f(&mut wrapped);
                            match wrapped {
                                Some(new) => *variable = new,
                                None => *variable = usize::MAX,
                            }
                        }
                    }
                    child.generator.for_each_variable_mut(f);
                }
            }
            Self::Selector { index_variable, children, .. } => {
                f(index_variable);
                children.iter_mut().for_each(|child| child.for_each_variable_mut(f));
            }
            Self::Modified { inner, .. } => {
                if let Some(inner) = inner {
                    inner.for_each_variable_mut(f);
                }
            }
            Self::Sequence(_) | Self::Machine(_) | Self::Reference(_) => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateMachine {
    pub start_state: i32,
    /// The variable that picks the start state when the machine activates
    /// (`startStateId` binding: `iSyncIdleLocomotion` and the like).
    pub start_state_variable: Option<usize>,
    pub states: Vec<State>,
    /// Transitions taken from any state.
    pub wildcard: Vec<Transition>,
}

/// What a behavior variable holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableKind {
    Bool,
    Int,
    Real,
    /// A vector, quaternion or pointer variable: not modelled.
    Other,
}

/// A behavior variable (`hkbBehaviorGraphData`): the named inputs `Speed` and
/// `Direction` give a locomotion blender, the flags a transition's condition tests.
#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    pub name: String,
    pub kind: VariableKind,
    /// The authored initial value (bool as 0/1, int as its value).
    pub initial: f32,
}

/// A decoded behavior graph.
#[derive(Debug, Clone, PartialEq)]
pub struct BehaviorGraph {
    pub variables: Vec<Variable>,
    pub events: Vec<String>,
    /// The graph's root state machine, when it has one.
    pub machine: Option<StateMachine>,
}

/// How many behavior references [`BehaviorGraph::start_clip`] follows before it gives
/// up (a mod graph that refers to itself cannot hang it).
const MAX_REFERENCE_HOPS: usize = 8;

impl StateMachine {
    /// The clip this machine plays on entry: the start state's own clip, else the
    /// first of its nested machines that yields one, else the first sub-behavior it
    /// refers to. `resolve` loads a referenced behavior by the path it names.
    fn start_clip(&self, resolve: &mut dyn FnMut(&str) -> Option<BehaviorGraph>, hops: &mut usize) -> Option<StateClip> {
        let state = self.states.iter().find(|s| s.id == self.start_state).or(self.states.first())?;
        if let Some(clip) = &state.clip {
            return Some(clip.clone());
        }
        for machine in &state.content.machines {
            if let Some(clip) = machine.start_clip(resolve, hops) {
                return Some(clip);
            }
        }
        for (ordinal, reference) in state.content.behavior_references.iter().enumerate() {
            if state.content.reference_machines.get(ordinal).is_some_and(Option::is_some) {
                continue;
            }
            if *hops >= MAX_REFERENCE_HOPS {
                return None;
            }
            *hops += 1;
            if let Some(clip) = resolve(reference).and_then(|graph| graph.machine?.start_clip(resolve, hops)) {
                return Some(clip);
            }
        }
        state.content.blended_clips.first().cloned()
    }
}

/// How many sub-behaviors [`BehaviorGraph::inline_references`] pulls in before it stops
/// (a graph can refer to the same few behaviors from many states).
const MAX_INLINED_REFERENCES: usize = 64;
/// Machines nested in machines, through the generators of their states: the 32 levels of
/// generator nesting a decoded file can reach below the root machine, plus the root. A sub-behavior
/// inlined under a state counts against the same limit, so every recursive walk over the
/// result (the remaps here, the runtime's `MachineRun`, dispatch and schedule, and the derived
/// `Clone`/`Drop`) goes no deeper than it does on a file that was never inlined.
const MAX_MACHINE_NESTING: usize = MAX_GENERATOR_DEPTH + 1;

/// A number a modifier holds that names something of its graph.
enum Slot<'a> {
    Event(&'a mut i32),
    Variable(&'a mut usize),
    Id(&'a mut u32),
}

/// Visit every event, variable and id a modifier (and the modifiers inside it) names.
fn visit_modifier(node: &mut Arc<ModifierNode>, f: &mut dyn FnMut(Slot<'_>)) {
    let node = Arc::make_mut(node);
    f(Slot::Id(&mut node.id));
    match &mut node.kind {
        Modifier::AssignVariables { floats, ints } => {
            floats.iter_mut().for_each(|(variable, _)| f(Slot::Variable(variable)));
            ints.iter_mut().for_each(|(variable, _)| f(Slot::Variable(variable)));
        }
        Modifier::ModifyOnce { on_activate, on_deactivate } => {
            for inner in on_activate.iter_mut().chain(on_deactivate.iter_mut()) {
                visit_modifier(inner, f);
            }
        }
        Modifier::EventsFromRange { input_variable, ranges, .. } => {
            if let Some(variable) = input_variable {
                f(Slot::Variable(variable));
            }
            ranges.iter_mut().for_each(|range| f(Slot::Event(&mut range.event)));
        }
        Modifier::EvaluateExpression { .. } => {}
        Modifier::Timer { event, .. } | Modifier::RandomAlarm { event, .. } => f(Slot::Event(event)),
        Modifier::TimeStep { variable } => {
            if let Some(variable) = variable {
                f(Slot::Variable(variable));
            }
        }
        Modifier::EventOnDeactivate { event } => f(Slot::Event(event)),
        Modifier::EventEveryNEvents { check, send, .. } => {
            f(Slot::Event(check));
            f(Slot::Event(send));
        }
        Modifier::EventDriven { activate, deactivate, inner, .. } => {
            f(Slot::Event(activate));
            f(Slot::Event(deactivate));
            if let Some(inner) = inner {
                visit_modifier(inner, f);
            }
        }
        Modifier::List(items) => items.iter_mut().for_each(|item| visit_modifier(item, f)),
    }
}

/// Renumber the events `machine` names (transitions and clip triggers) through `map`.
fn remap_machine_events(machine: &mut StateMachine, map: &[i32]) {
    let remap = |event: i32| usize::try_from(event).ok().and_then(|index| map.get(index)).copied().unwrap_or(event);
    for transition in machine.wildcard.iter_mut() {
        transition.event = remap(transition.event);
    }
    for state in &mut machine.states {
        for transition in &mut state.transitions {
            transition.event = remap(transition.event);
        }
        for clip in state.clip.iter_mut().chain(state.content.blended_clips.iter_mut()) {
            for trigger in &mut clip.triggers {
                trigger.event = remap(trigger.event);
            }
        }
        if let Some(generator) = &mut state.content.generator {
            generator.for_each_clip_mut(&mut |clip| {
                for trigger in &mut clip.triggers {
                    trigger.event = remap(trigger.event);
                }
            });
            generator.for_each_modifier_mut(&mut |modifier| {
                visit_modifier(modifier, &mut |slot| {
                    if let Slot::Event(event) = slot {
                        *event = remap(*event);
                    }
                })
            });
        }
        for inner in &mut state.content.machines {
            remap_machine_events(inner, map);
        }
    }
}

/// Add `offset` to the id of every modifier of `machine`.
fn offset_machine_modifier_ids(machine: &mut StateMachine, offset: u32) {
    for state in &mut machine.states {
        if let Some(generator) = &mut state.content.generator {
            generator.for_each_modifier_mut(&mut |modifier| {
                visit_modifier(modifier, &mut |slot| {
                    if let Slot::Id(id) = slot {
                        *id += offset;
                    }
                })
            });
        }
        for inner in &mut state.content.machines {
            offset_machine_modifier_ids(inner, offset);
        }
    }
}

/// How many machines deep `machine` goes: itself and the longest chain of machines nested in
/// the states below it.
fn machine_height(machine: &StateMachine) -> usize {
    1 + machine
        .states
        .iter()
        .flat_map(|state| &state.content.machines)
        .map(machine_height)
        .max()
        .unwrap_or(0)
}

/// One past the highest modifier id of `machine`.
fn machine_modifier_id_bound(machine: &StateMachine) -> u32 {
    let mut bound = 0;
    for state in &machine.states {
        if let Some(generator) = &state.content.generator {
            // `for_each_modifier_mut` needs a mutable tree; clone the (small) generator.
            let mut copy = generator.clone();
            copy.for_each_modifier_mut(&mut |modifier| {
                let mut node = Arc::clone(modifier);
                visit_modifier(&mut node, &mut |slot| {
                    if let Slot::Id(id) = slot {
                        bound = bound.max(*id + 1);
                    }
                });
            });
        }
        for inner in &state.content.machines {
            bound = bound.max(machine_modifier_id_bound(inner));
        }
    }
    bound
}

/// Renumber the variables `machine` names (blend parameters, weights, clip speeds,
/// transition durations, start states) through `map`.
fn remap_machine_variables(machine: &mut StateMachine, map: &[usize]) {
    let remap = |variable: &mut Option<usize>| *variable = variable.and_then(|index| map.get(index).copied());
    remap(&mut machine.start_state_variable);
    for transition in machine.wildcard.iter_mut() {
        remap(&mut transition.blend_duration_variable);
    }
    for state in &mut machine.states {
        for transition in &mut state.transitions {
            remap(&mut transition.blend_duration_variable);
        }
        for clip in state.clip.iter_mut().chain(state.content.blended_clips.iter_mut()) {
            remap(&mut clip.playback_speed_variable);
        }
        if let Some(generator) = &mut state.content.generator {
            generator.for_each_modifier_mut(&mut |modifier| {
                visit_modifier(modifier, &mut |slot| {
                    if let Slot::Variable(variable) = slot {
                        *variable = map.get(*variable).copied().unwrap_or(*variable);
                    }
                })
            });
            generator.for_each_variable_mut(&mut |variable| {
                // `usize::MAX` marks a bone-weight binding whose variable did not map.
                *variable = variable.and_then(|index| if index == usize::MAX { None } else { map.get(index).copied() });
            });
        }
        for inner in &mut state.content.machines {
            remap_machine_variables(inner, map);
        }
    }
}

impl BehaviorGraph {
    /// The id of `name` in this graph's events, adding it when it has none: behaviors
    /// share event *names*, so a referenced graph's events join its parent's table.
    fn intern_event(&mut self, names: &mut Interned, name: &str) -> i32 {
        *names.events.entry(name.to_ascii_lowercase()).or_insert_with(|| {
            self.events.push(name.to_owned());
            (self.events.len() - 1) as i32
        })
    }

    /// The index of `variable` in this graph's variables (by name), adding it when it has
    /// none: a referenced behavior's variables join its parent's table like its events.
    fn intern_variable(&mut self, names: &mut Interned, variable: &Variable) -> usize {
        *names.variables.entry(variable.name.to_ascii_lowercase()).or_insert_with(|| {
            self.variables.push(variable.clone());
            self.variables.len() - 1
        })
    }

    /// The index of the variable named `name` (case-insensitive).
    pub fn variable_id(&self, name: &str) -> Option<usize> {
        self.variables.iter().position(|variable| variable.name.eq_ignore_ascii_case(name))
    }

    /// Replace every state's sub-behavior reference with the referenced graph's root
    /// machine, nested in that state (a state that refers to a behavior runs it as a
    /// machine of its own): the machine's events are renumbered into this graph's
    /// table by name, so an event sent to the parent reaches the sub-behavior's
    /// transitions. `resolve` loads a referenced behavior by the path it names; a
    /// reference it cannot load is left as it was.
    pub fn inline_references(&mut self, resolve: &mut dyn FnMut(&str) -> Option<BehaviorGraph>) {
        let mut budget = MAX_INLINED_REFERENCES;
        self.inline_references_with(resolve, &mut budget);
    }

    fn inline_machine(
        &mut self,
        machine: &mut StateMachine,
        resolve: &mut dyn FnMut(&str) -> Option<BehaviorGraph>,
        budget: &mut usize,
        ids: &mut u32,
        names: &mut Interned,
        depth: usize,
    ) {
        for state in &mut machine.states {
            let wanted = state.content.behavior_references.len();
            state.content.reference_machines.resize(wanted, None);
            for ordinal in 0..wanted {
                if state.content.reference_machines[ordinal].is_some() {
                    continue;
                }
                if *budget == 0 {
                    break;
                }
                *budget -= 1;
                let reference = state.content.behavior_references[ordinal].clone();
                let Some(mut inner) = resolve(&reference) else { continue };
                // The referenced graph's own references first (its events are its own).
                inner.inline_references_with(resolve, budget);
                let Some(inner_machine) = inner.machine.as_ref() else { continue };
                // Nested under this machine it sits one level down: it must still fit the limit
                // a decoded file keeps to (a graph that refers to itself would otherwise stack
                // up to the whole budget of machines, every one walked recursively).
                if depth + machine_height(inner_machine) > MAX_MACHINE_NESTING {
                    continue;
                }
                let inner_bound = machine_modifier_id_bound(inner_machine);
                let Some(mut inner_machine) = inner.machine.take() else { continue };
                let map: Vec<i32> = inner.events.iter().map(|name| self.intern_event(names, name)).collect();
                remap_machine_events(&mut inner_machine, &map);
                let variable_map: Vec<usize> = inner.variables.iter().map(|variable| self.intern_variable(names, variable)).collect();
                remap_machine_variables(&mut inner_machine, &variable_map);
                // Keep the sub-behavior's modifier ids apart from everything inlined so far.
                offset_machine_modifier_ids(&mut inner_machine, *ids);
                *ids += inner_bound;
                state.content.machines.push(inner_machine);
                state.content.reference_machines[ordinal] = Some(state.content.machines.len() - 1);
            }
            for inner in &mut state.content.machines {
                self.inline_machine(inner, resolve, budget, ids, names, depth + 1);
            }
        }
    }

    fn inline_references_with(&mut self, resolve: &mut dyn FnMut(&str) -> Option<BehaviorGraph>, budget: &mut usize) {
        let Some(mut machine) = self.machine.take() else { return };
        let mut ids = machine_modifier_id_bound(&machine);
        let mut names = Interned::of(self);
        self.inline_machine(&mut machine, resolve, budget, &mut ids, &mut names, 1);
        self.machine = Some(machine);
    }

    /// The clip an actor standing idle in this graph plays: following the start
    /// state of every machine down (and into the sub-behaviors a state refers to,
    /// loaded through `resolve`) to the first clip. Fallout 4's `MTBehavior` reaches
    /// `Animations\PoseA_Idle1.hkt` through its `MTIdles` state's reference to
    /// `DialogueBehavior.hkx`. Variables and events are not evaluated: this is the
    /// pose the graph starts in.
    pub fn start_clip(&self, resolve: &mut dyn FnMut(&str) -> Option<BehaviorGraph>) -> Option<StateClip> {
        self.machine.as_ref()?.start_clip(resolve, &mut 0)
    }

    /// The id of the event named `name` (case-insensitive).
    pub fn event_id(&self, name: &str) -> Option<i32> {
        self.events
            .iter()
            .position(|event| event.eq_ignore_ascii_case(name))
            .map(|index| index as i32)
    }

    pub fn event_name(&self, id: i32) -> Option<&str> {
        usize::try_from(id).ok().and_then(|index| self.events.get(index)).map(String::as_str)
    }
}

/// A decoded `hkbCharacterData` + string data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    /// The skeleton packfile, relative to the project directory.
    pub rig: String,
    /// The behavior packfile, relative to the project directory.
    pub behavior: String,
    pub animations: Vec<String>,
}

fn require_64_bit(file: &Packfile) -> Result<()> {
    if file.pointer_size() != 8 {
        return Err(HkxError::UnsupportedLayout(
            "behavior data is decoded for 64-bit (Skyrim SE) packfiles only",
        ));
    }
    Ok(())
}

/// An `hkArray`'s data offset and length at `field`.
fn array(file: &Packfile, field: usize, cap: usize, label: &'static str) -> Result<(usize, usize)> {
    let len = file.u32(field + 8, label)? as usize;
    if len == 0 {
        return Ok((0, 0));
    }
    if len > cap {
        return Err(HkxError::InvalidData("implausible behavior array length"));
    }
    let data = file
        .local_target(field)
        .ok_or(HkxError::InvalidData("behavior array without data"))?;
    Ok((data, len))
}

/// The string an `hkStringPtr` at `field` points to; empty when null.
fn string(file: &Packfile, field: usize, label: &'static str) -> Result<String> {
    Ok(match file.local_target(field) {
        Some(target) => file.cstr(target, label)?.to_owned(),
        None => String::new(),
    })
}

fn string_array(file: &Packfile, field: usize, cap: usize, label: &'static str) -> Result<Vec<String>> {
    let (data, len) = array(file, field, cap, label)?;
    (0..len).map(|index| string(file, data + index * 8, label)).collect()
}

/// The variable the `member` of the bindable at `bindable` is bound to, when it is.
fn bound_variable(file: &Packfile, bindable: usize, member: &str) -> Result<Option<usize>> {
    let Some(set) = file.object_pointer(bindable + BINDABLE_BINDING_SET) else {
        return Ok(None);
    };
    let (data, len) = array(file, set + BINDING_SET_BINDINGS, MAX_VARIABLES, "variable bindings")?;
    for index in 0..len {
        let element = data + index * BINDING_STRIDE;
        if string(file, element + BINDING_MEMBER_PATH, "binding member")?.eq_ignore_ascii_case(member) {
            let variable = file.u32(element + BINDING_VARIABLE_INDEX, "binding variable")? as i32;
            return Ok(usize::try_from(variable).ok());
        }
    }
    Ok(None)
}

/// The blending effect's duration, and the variable that overrides it.
fn read_transition_effect(file: &Packfile, effect: usize) -> Result<(f32, Option<usize>)> {
    let l = layout(file);
    if file.class_at(effect) != Some("hkbBlendingTransitionEffect") {
        return Ok((0.0, None));
    }
    let duration = file.f32(effect + l.effect_duration, "transition duration")?;
    let duration = if duration.is_finite() && duration > 0.0 { duration } else { 0.0 };
    Ok((duration, bound_variable(file, effect, "duration")?))
}

fn read_transitions(file: &Packfile, array_object: usize) -> Result<Vec<Transition>> {
    let l = layout(file);
    let (data, len) = array(file, array_object + l.transitions_array, MAX_TRANSITIONS, "transitions")?;
    (0..len)
        .map(|index| {
            let element = data + index * l.transition_stride;
            let (blend_duration, blend_duration_variable) = match file.object_pointer(element + TRANSITION_EFFECT) {
                Some(effect) => read_transition_effect(file, effect)?,
                None => (0.0, None),
            };
            let condition = match file.object_pointer(element + TRANSITION_CONDITION) {
                Some(object) if matches!(file.class_at(object), Some("hkbExpressionCondition" | "hkbStringCondition")) => {
                    Some(Arc::new(Condition::parse(&string(file, object + CONDITION_TEXT, "condition text")?)))
                }
                _ => None,
            };
            Ok(Transition {
                condition,
                event: file.u32(element + l.transition_event_id, "transition event")? as i32,
                to_state: file.u32(element + l.transition_to_state, "transition target")? as i32,
                priority: file.u16(element + l.transition_priority, "transition priority")? as i16,
                flags: file.u16(element + l.transition_flags, "transition flags")?,
                blend_duration,
                blend_duration_variable,
            })
        })
        .collect()
}

fn read_clip(file: &Packfile, clip: usize) -> Result<StateClip> {
    let l = layout(file);
    let mut triggers = Vec::new();
    if let Some(array_object) = file.object_pointer(clip + l.clip_triggers) {
        let (data, len) = array(file, array_object + TRIGGERS_ARRAY, MAX_TRIGGERS, "clip triggers")?;
        for index in 0..len {
            let element = data + index * TRIGGER_STRIDE;
            triggers.push(ClipTrigger {
                local_time: file.f32(element + TRIGGER_LOCAL_TIME, "trigger time")?,
                event: file.u32(element + TRIGGER_EVENT_ID, "trigger event")? as i32,
                relative_to_end: file.u8(element + TRIGGER_RELATIVE_TO_END, "trigger end flag")? != 0,
                annotation: file.u8(element + TRIGGER_IS_ANNOTATION, "trigger annotation flag")? != 0,
            });
        }
    }
    let mode = match file.u8(clip + l.clip_mode, "clip mode")? {
        0 => ClipMode::SinglePlay,
        1 => ClipMode::Looping,
        2 => ClipMode::UserControlled,
        3 => ClipMode::PingPong,
        _ => return Err(HkxError::InvalidData("unknown clip playback mode")),
    };
    Ok(StateClip {
        animation: string(file, clip + l.clip_animation_name, "clip animation name")?,
        mode,
        playback_speed: file.f32(clip + l.clip_playback_speed, "clip playback speed")?,
        start_time: file.f32(clip + l.clip_start_time, "clip start time")?,
        user_fraction: file.f32(clip + l.clip_user_fraction, "clip user fraction")?,
        triggers,
        playback_speed_variable: bound_variable(file, clip, "playbackSpeed")?,
    })
}

/// The element a binding's `offsetInArrayPlusOne` names: that field is a signed int, and 0 or
/// a negative value (a file can say `0x80000000`, whose predecessor does not exist) names none.
fn bone_of_binding(offset_plus_one: u32) -> Option<usize> {
    usize::try_from((offset_plus_one as i32).checked_sub(1)?).ok()
}

/// `hkbBoneWeightArray`: its weights (`boneWeights` at the bindable base's end), and the
/// elements variables drive (bindings on the array whose member is `boneWeights`, the
/// element being the binding's `offsetInArrayPlusOne`).
fn read_bone_weights(file: &Packfile, array_object: usize) -> Result<BoneWeights> {
    let (data, len) = array(file, array_object + BONE_WEIGHTS, MAX_BONES, "bone weights")?;
    let weights = (0..len).map(|index| file.f32(data + index * 4, "bone weight")).collect::<Result<Vec<_>>>()?;
    let mut bound = Vec::new();
    if let Some(set) = file.object_pointer(array_object + BINDABLE_BINDING_SET) {
        let (bindings, count) = array(file, set + BINDING_SET_BINDINGS, MAX_VARIABLES, "bone weight bindings")?;
        for index in 0..count {
            let element = bindings + index * BINDING_STRIDE;
            if !string(file, element + BINDING_MEMBER_PATH, "binding member")?.eq_ignore_ascii_case("boneWeights") {
                continue;
            }
            let variable = file.u32(element + BINDING_VARIABLE_INDEX, "binding variable")? as i32;
            let offset = file.u32(element + BINDING_OFFSET_IN_ARRAY, "binding array offset")?;
            if let (Ok(variable), Some(bone)) = (usize::try_from(variable), bone_of_binding(offset)) {
                if bone < weights.len() {
                    bound.push((bone, variable));
                }
            }
        }
    }
    Ok(BoneWeights { weights: Arc::new(weights), bound })
}

/// Read the generator at `generator` into the generator tree, and - the same walk - into
/// the state's flattened lists (`clip` is the first clip met; the event schedule and the
/// start-clip search use the lists, the pose uses the tree). `None` for a kind of
/// generator the runtime has no meaning for.
fn read_generator(
    file: &Packfile,
    generator: usize,
    clip: &mut Option<StateClip>,
    content: &mut StateContent,
    depth: usize,
    walk: &mut Walk,
) -> Result<Option<Generator>> {
    if depth > MAX_GENERATOR_DEPTH {
        return Ok(None);
    }
    walk.enter(generator)?;
    let node = read_generator_node(file, generator, clip, content, depth, walk);
    walk.leave();
    node
}

fn read_generator_node(
    file: &Packfile,
    generator: usize,
    clip: &mut Option<StateClip>,
    content: &mut StateContent,
    depth: usize,
    walk: &mut Walk,
) -> Result<Option<Generator>> {
    let l = layout(file);
    Ok(match file.class_at(generator) {
        Some("hkbClipGenerator") => {
            let read = read_clip(file, generator)?;
            if clip.is_none() {
                *clip = Some(read.clone());
            } else {
                content.blended_clips.push(read.clone());
            }
            Some(Generator::Clip(read))
        }
        Some("BGSGamebryoSequenceGenerator") => {
            let name = string(file, generator + l.sequence_name, "sequence name")?;
            if name.is_empty() {
                None
            } else {
                content.sequences.push(name.clone());
                Some(Generator::Sequence(name))
            }
        }
        Some("hkbStateMachine") => {
            content.machines.push(read_machine_at(file, generator, depth + 1, walk)?);
            Some(Generator::Machine(content.machines.len() - 1))
        }
        Some("hkbModifierGenerator") => {
            let inner = match l.wrapped_generator.and_then(|field| file.object_pointer(generator + field)) {
                Some(inner) => read_generator(file, inner, clip, content, depth + 1, walk)?,
                None => None,
            };
            let modifier = match file.object_pointer(generator + MODIFIER_GENERATOR_MODIFIER) {
                Some(modifier) => read_modifier(file, modifier, 0, walk)?,
                None => None,
            };
            match modifier {
                Some(modifier) => Some(Generator::Modified { modifiers: vec![modifier], inner: inner.map(Box::new) }),
                None => inner,
            }
        }
        Some("BSiStateTaggingGenerator" | "DynamicAnimationTaggingGenerator" | "BSOffsetAnimationGenerator") => {
            match l.wrapped_generator.and_then(|field| file.object_pointer(generator + field)) {
                Some(inner) => read_generator(file, inner, clip, content, depth + 1, walk)?,
                None => None,
            }
        }
        // The cyclic blend wrapper is what takes the direction variable (its
        // `fBlendParameter` binding) and feeds the blender it wraps, which carries no
        // binding of its own (the vanilla 8-way graph: 4 wrappers, 4 unbound blenders;
        // the engine's description: "The blend parameter to pass into the cyclic blend
        // generator. DO NOT SET A VARIABLE ON THE REFERENCED BLENDER GENERATOR").
        Some("BSCyclicBlendTransitionGenerator") => {
            let Some(inner) = l.wrapped_generator.and_then(|field| file.object_pointer(generator + field)) else {
                return Ok(None);
            };
            let mut node = read_generator(file, inner, clip, content, depth + 1, walk)?;
            if let Some(Generator::Blender(blender)) = &mut node {
                if blender.parameter_variable.is_none() {
                    blender.parameter_variable = bound_variable(file, generator, "fBlendParameter")?;
                }
            }
            node
        }
        Some("hkbManualSelectorGenerator") => {
            let Some(field) = l.selector_generators else { return Ok(None) };
            let (data, len) = array(file, generator + field, MAX_STATES, "selector generators")?;
            walk.charge(len)?;
            let mut children = Vec::with_capacity(len);
            for index in 0..len {
                if let Some(inner) = file.object_pointer(data + index * 8) {
                    if let Some(child) = read_generator(file, inner, clip, content, depth + 1, walk)? {
                        children.push(child);
                    }
                }
            }
            let index = i32::from(file.u8(generator + field + SELECTOR_INDEX_AFTER_ARRAY, "selected generator")? as i8);
            Some(Generator::Selector { index, index_variable: bound_variable(file, generator, "selectedGeneratorIndex")?, children })
        }
        Some("hkbBehaviorReferenceGenerator") => {
            let Some(field) = l.reference_behavior_name else { return Ok(None) };
            let name = string(file, generator + field, "behavior reference")?;
            if name.is_empty() {
                None
            } else {
                content.behavior_references.push(name);
                Some(Generator::Reference(content.behavior_references.len() - 1))
            }
        }
        Some("hkbBlenderGenerator") => {
            let flags = file.u16(generator + l.blender_flags, "blender flags")?;
            let (data, len) = array(file, generator + l.blender_children, MAX_STATES, "blender children")?;
            walk.charge(len)?;
            let mut children = Vec::with_capacity(len);
            for index in 0..len {
                let Some(child) = file.object_pointer(data + index * 8) else { continue };
                let Some(inner) = file.object_pointer(child + l.blender_child_generator) else { continue };
                let Some(node) = read_generator(file, inner, clip, content, depth + 1, walk)? else { continue };
                let bone_weights = match file.object_pointer(child + BLENDER_CHILD_BONE_WEIGHTS) {
                    Some(array_object) => Some(read_bone_weights(file, array_object)?),
                    None => None,
                };
                children.push(BlendChild {
                    weight: file.f32(child + BLENDER_CHILD_WEIGHT, "blender child weight")?,
                    weight_variable: bound_variable(file, child, "weight")?,
                    bone_weights,
                    bone_weights_variable: bound_variable(file, child, "boneWeights")?,
                    generator: node,
                });
            }
            Some(Generator::Blender(Blender {
                parametric: flags & BLENDER_FLAG_PARAMETRIC != 0,
                cyclic: flags & BLENDER_FLAG_CYCLIC != 0,
                sync: flags & BLENDER_FLAG_SYNC != 0,
                parameter: file.f32(generator + l.blender_parameter, "blend parameter")?,
                parameter_variable: bound_variable(file, generator, "blendParameter")?,
                min_cyclic: file.f32(generator + l.blender_min_cyclic, "min cyclic parameter")?,
                max_cyclic: file.f32(generator + l.blender_max_cyclic, "max cyclic parameter")?,
                children,
            }))
        }
        _ => None,
    })
}

/// The event id of an `hkbEvent` field (its `m_id`; negative for none).
fn event_at(file: &Packfile, field: usize) -> Result<i32> {
    Ok(file.u32(field, "event id")? as i32)
}

/// A graph's modifier at `object`: `None` for the kinds the runtime has no use for, and for
/// any Havok 2010 file (their layouts are not measured).
fn read_modifier(file: &Packfile, object: usize, depth: usize, walk: &mut Walk) -> Result<Option<Arc<ModifierNode>>> {
    if file.version() != 11 || depth > MAX_MODIFIER_DEPTH {
        return Ok(None);
    }
    walk.enter(object)?;
    let node = read_modifier_node(file, object, depth, walk);
    walk.leave();
    node
}

fn read_modifier_node(file: &Packfile, object: usize, depth: usize, walk: &mut Walk) -> Result<Option<Arc<ModifierNode>>> {
    let id = object as u32;
    // The kinds that wrap another modifier read it through here; `walk` is borrowed only
    // for the call, so the closure is built where it is used.
    macro_rules! child {
        ($field:expr) => {
            match file.object_pointer(object + $field) {
                Some(inner) => read_modifier(file, inner, depth + 1, walk)?,
                None => None,
            }
        };
    }
    let kind = match file.class_at(object) {
        Some("BSAssignVariablesModifier") => {
            let (mut floats, mut ints) = (Vec::new(), Vec::new());
            if let Some(set) = file.object_pointer(object + BINDABLE_BINDING_SET) {
                let (bindings, count) = array(file, set + BINDING_SET_BINDINGS, MAX_VARIABLES, "assign bindings")?;
                for index in 0..count {
                    let element = bindings + index * BINDING_STRIDE;
                    let path = string(file, element + BINDING_MEMBER_PATH, "binding member")?;
                    let variable = file.u32(element + BINDING_VARIABLE_INDEX, "binding variable")? as i32;
                    let Ok(variable) = usize::try_from(variable) else { continue };
                    let number = path.trim_start_matches(|c: char| c.is_alphabetic()).parse::<usize>().ok().filter(|n| *n >= 1);
                    let Some(number) = number else { continue };
                    if path.starts_with("floatVariable") && number <= 20 {
                        let at = object + ASSIGN_FLOATS + (number - 1) * ASSIGN_PAIR + 4;
                        floats.push((variable, file.f32(at, "assigned float")?));
                    } else if path.starts_with("intVariable") && number <= 4 {
                        let at = object + ASSIGN_INTS + (number - 1) * ASSIGN_PAIR + 4;
                        ints.push((variable, file.u32(at, "assigned int")? as i32));
                    }
                }
            }
            Modifier::AssignVariables { floats, ints }
        }
        Some("BSModifyOnceModifier") => {
            Modifier::ModifyOnce { on_activate: child!(MODIFY_ONCE_ON_ACTIVATE), on_deactivate: child!(MODIFY_ONCE_ON_DEACTIVATE) }
        }
        Some("hkbEventsFromRangeModifier") => {
            let mut ranges = Vec::new();
            if let Some(array_object) = file.object_pointer(object + EVENTS_FROM_RANGE_RANGES) {
                let (data, len) = array(file, array_object + EVENT_RANGES_ARRAY, MAX_STATES, "event ranges")?;
                for index in 0..len {
                    let element = data + index * EVENT_RANGE_STRIDE;
                    ranges.push(EventRange {
                        upper: file.f32(element, "range upper bound")?,
                        event: event_at(file, element + EVENT_RANGE_EVENT)?,
                        when_in_range: file.u8(element + EVENT_RANGE_MODE, "range event mode")? == 1,
                    });
                }
            }
            Modifier::EventsFromRange {
                input: file.f32(object + EVENTS_FROM_RANGE_INPUT, "range input")?,
                input_variable: bound_variable(file, object, "inputValue")?,
                lower_bound: file.f32(object + EVENTS_FROM_RANGE_LOWER, "range lower bound")?,
                ranges,
            }
        }
        Some("hkbEvaluateExpressionModifier") => {
            let mut expressions = Vec::new();
            if let Some(array_object) = file.object_pointer(object + EVALUATE_EXPRESSION_EXPRESSIONS) {
                let (data, len) = array(file, array_object + EXPRESSION_ARRAY, MAX_STATES, "expressions")?;
                for index in 0..len {
                    let element = data + index * EXPRESSION_STRIDE;
                    expressions.push(ExpressionData {
                        text: string(file, element, "expression")?,
                        event_mode: file.u8(element + EXPRESSION_MODE, "expression event mode")?,
                    });
                }
            }
            Modifier::EvaluateExpression { expressions }
        }
        Some("BSEventOnDeactivateModifier") => Modifier::EventOnDeactivate { event: event_at(file, object + EVENT_ON_DEACTIVATE_EVENT)? },
        Some("BSEventEveryNEventsModifier") => Modifier::EventEveryNEvents {
            check: event_at(file, object + EVERY_N_CHECK)?,
            send: event_at(file, object + EVERY_N_SEND)?,
            count: u32::from(file.u8(object + EVERY_N_COUNT, "events before send")?),
        },
        Some("hkbEventDrivenModifier") => Modifier::EventDriven {
            activate: event_at(file, object + EVENT_DRIVEN_ACTIVATE)?,
            deactivate: event_at(file, object + EVENT_DRIVEN_DEACTIVATE)?,
            active_by_default: file.u8(object + EVENT_DRIVEN_BY_DEFAULT, "event driven default")? != 0,
            inner: child!(EVENT_DRIVEN_MODIFIER),
        },
        Some("BSGetTimeStepModifier") => Modifier::TimeStep { variable: bound_variable(file, object, "timeStep")? },
        Some("hkbTimerModifier" | "BSTimerModifier") => Modifier::Timer {
            alarm_seconds: file.f32(object + TIMER_ALARM, "alarm time")?,
            event: event_at(file, object + TIMER_EVENT)?,
        },
        Some("BSRandomAlarmModifier") => Modifier::RandomAlarm {
            min_seconds: file.f32(object + RANDOM_ALARM_MIN, "minimum alarm time")?,
            max_seconds: file.f32(object + RANDOM_ALARM_MAX, "maximum alarm time")?,
            event: event_at(file, object + RANDOM_ALARM_EVENT)?,
            reset: file.u8(object + RANDOM_ALARM_RESET, "reset alarm")? != 0,
        },
        Some("hkbModifierList") => {
            let (data, len) = array(file, object + MODIFIER_LIST_MODIFIERS, MAX_STATES, "modifier list")?;
            walk.charge(len)?;
            let mut items = Vec::with_capacity(len);
            for index in 0..len {
                if let Some(inner) = file.object_pointer(data + index * 8) {
                    items.extend(read_modifier(file, inner, depth + 1, walk)?);
                }
            }
            Modifier::List(items)
        }
        _ => return Ok(None),
    };
    Ok(Some(Arc::new(ModifierNode { id, kind })))
}

fn read_state(file: &Packfile, state: usize, depth: usize, walk: &mut Walk) -> Result<State> {
    let l = layout(file);
    let transitions = match file.object_pointer(state + l.state_transitions) {
        Some(array_object) => read_transitions(file, array_object)?,
        None => Vec::new(),
    };
    let mut clip = None;
    let mut content = StateContent::default();
    if let Some(generator) = file.object_pointer(state + l.state_generator) {
        content.generator = read_generator(file, generator, &mut clip, &mut content, depth, walk)?;
    }
    Ok(State {
        id: file.u32(state + l.state_id, "state id")? as i32,
        name: string(file, state + l.state_name, "state name")?,
        transitions,
        clip,
        content,
    })
}

fn read_machine(file: &Packfile, machine: usize, walk: &mut Walk) -> Result<StateMachine> {
    walk.enter(machine)?;
    let read = read_machine_at(file, machine, 0, walk);
    walk.leave();
    read
}

/// A state machine `read_generator` has already entered (it is the generator).
fn read_machine_at(file: &Packfile, machine: usize, depth: usize, walk: &mut Walk) -> Result<StateMachine> {
    let l = layout(file);
    let (data, len) = array(file, machine + l.machine_states, MAX_STATES, "states")?;
    walk.charge(len)?;
    let mut states = Vec::with_capacity(len);
    for index in 0..len {
        let state = file
            .object_pointer(data + index * 8)
            .ok_or(HkxError::InvalidData("state pointer without a target"))?;
        states.push(read_state(file, state, depth, walk)?);
    }
    let wildcard = match file.object_pointer(machine + l.machine_wildcard) {
        Some(array_object) => read_transitions(file, array_object)?,
        None => Vec::new(),
    };
    Ok(StateMachine {
        start_state: file.u32(machine + l.machine_start_state, "start state")? as i32,
        start_state_variable: bound_variable(file, machine, "startStateId")?,
        states,
        wildcard,
    })
}

/// The graph's variables: names, kinds and initial values.
fn read_variables(file: &Packfile, graph: usize, string_data: usize) -> Result<Vec<Variable>> {
    let l = layout(file);
    let Some(data) = file.object_pointer(graph + l.graph_data) else {
        return Ok(Vec::new());
    };
    let names = string_array(file, string_data + l.string_data_variable_names, MAX_VARIABLES, "variable names")?;
    let (infos, count) = array(file, data + l.graph_data_variable_infos, MAX_VARIABLES, "variable infos")?;
    let words = match file.object_pointer(data + l.graph_data_initial_values) {
        Some(set) => array(file, set + VALUE_SET_WORDS, MAX_VARIABLES, "variable words")?,
        None => (0, 0),
    };
    let mut variables = Vec::with_capacity(count);
    let mut next_word = 0;
    for index in 0..count {
        // BOOL, INT8, INT16, INT32 and REAL each take one word of the initial values; a
        // vector, quaternion or pointer lives elsewhere and takes none.
        let (kind, in_words) = match file.u8(infos + index * VARIABLE_INFO_STRIDE + VARIABLE_INFO_TYPE, "variable type")? {
            0 => (VariableKind::Bool, true),
            1..=3 => (VariableKind::Int, true),
            4 => (VariableKind::Real, true),
            _ => (VariableKind::Other, false),
        };
        let mut initial = 0.0;
        if in_words {
            if next_word < words.1 {
                let word = file.u32(words.0 + next_word * 4, "variable value")?;
                initial = match kind {
                    VariableKind::Real => f32::from_bits(word),
                    _ => word as i32 as f32,
                };
            }
            next_word += 1;
        }
        variables.push(Variable { name: names.get(index).cloned().unwrap_or_default(), kind, initial });
    }
    Ok(variables)
}

/// Decode a behavior-graph packfile (`Behaviors\\*.hkx`).
pub fn decode_behavior_graph(bytes: &[u8]) -> Result<BehaviorGraph> {
    decode_behavior_graph_within(bytes, MAX_GRAPH_STEPS).map(|(graph, _)| graph)
}

/// How many steps decoding `bytes` takes (the figure [`MAX_GRAPH_STEPS`] bounds), for the
/// corpus census that keeps that bound above what the shipped graphs need.
#[doc(hidden)]
pub fn behavior_graph_steps(bytes: &[u8]) -> Result<usize> {
    decode_behavior_graph_within(bytes, usize::MAX).map(|(_, steps)| steps)
}

fn decode_behavior_graph_within(bytes: &[u8], limit: usize) -> Result<(BehaviorGraph, usize)> {
    let file = Packfile::parse(bytes)?;
    require_64_bit(&file)?;
    let l = layout(&file);
    let graph = file.object("hkbBehaviorGraph")?;
    let string_data = file
        .object_pointer(graph + l.graph_data)
        .and_then(|data| file.object_pointer(data + l.graph_data_string_data))
        .or_else(|| file.objects_of("hkbBehaviorGraphStringData").first().copied())
        .ok_or(HkxError::MissingClass("hkbBehaviorGraphStringData"))?;
    let events = string_array(&file, string_data + l.string_data_event_names, MAX_EVENTS, "event names")?;
    let mut walk = Walk::new(limit);
    let machine = match file.object_pointer(graph + l.graph_root_generator) {
        Some(root) if file.class_at(root) == Some("hkbStateMachine") => Some(read_machine(&file, root, &mut walk)?),
        _ => match file.objects_of("hkbStateMachine").first() {
            Some(&machine) => Some(read_machine(&file, machine, &mut walk)?),
            None => None,
        },
    };
    let variables = read_variables(&file, graph, string_data)?;
    Ok((BehaviorGraph { variables, events, machine }, walk.steps))
}

/// How many objects of each class a packfile holds, most numerous first: the first
/// look at a graph before a class layout is derived (`examples/dump_objects.rs`).
#[doc(hidden)]
pub fn class_census(bytes: &[u8]) -> Result<Vec<(String, usize)>> {
    let file = Packfile::parse(bytes)?;
    Ok(file.class_census())
}

/// A line-per-slot dump of every object of `class_name` in a packfile: each
/// 8-byte slot as raw hex, plus what it points at (an object's class, or a local
/// string). The evidence for a class layout, where the Havok SDK on disk has none
/// (`examples/graph_dump.rs --objects`).
#[doc(hidden)]
pub fn dump_objects(bytes: &[u8], class_name: &str, bytes_per_object: usize) -> Result<Vec<String>> {
    let file = Packfile::parse(bytes)?;
    let mut lines = Vec::new();
    for object in file.objects_of(class_name) {
        lines.push(format!("{class_name} @ {object:#x}"));
        for slot in (0..bytes_per_object).step_by(8) {
            let at = object + slot;
            let raw = file.u32(at, "slot")? as u64 | (file.u32(at + 4, "slot")? as u64) << 32;
            let mut note = String::new();
            if let Some(target) = file.object_pointer(at) {
                note = format!("-> object {} @ {target:#x}", file.class_at(target).unwrap_or("?"));
            } else if let Some(target) = file.local_target(at) {
                note = match file.cstr(target, "dump string") {
                    Ok(text) if !text.is_empty() && text.len() < 80 && text.is_ascii() => format!("-> local {target:#x} {text:?}"),
                    _ => format!("-> local {target:#x}"),
                };
            }
            lines.push(format!("  +{slot:#04x}  {raw:#018x}  {note}"));
        }
    }
    Ok(lines)
}

/// Decode a character packfile (`Characters\\*.hkx`).
pub fn decode_character(bytes: &[u8]) -> Result<Character> {
    let file = Packfile::parse(bytes)?;
    require_64_bit(&file)?;
    let strings = file.object("hkbCharacterStringData")?;
    Ok(Character {
        rig: string(&file, strings + CHARACTER_RIG_NAME, "rig name")?,
        behavior: string(&file, strings + CHARACTER_BEHAVIOR_FILE, "behavior file")?,
        animations: string_array(&file, strings + CHARACTER_ANIMATION_NAMES, MAX_EVENTS, "animation names")?,
    })
}

/// The character files a project packfile lists (`hkbProjectStringData`).
pub fn decode_project(bytes: &[u8]) -> Result<Vec<String>> {
    let file = Packfile::parse(bytes)?;
    require_64_bit(&file)?;
    let strings = file.object("hkbProjectStringData")?;
    string_array(&file, strings + PROJECT_CHARACTER_FILENAMES, 64, "character filenames")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packfile::fixtures::PackfileBuilder;

    /// Data layout of the `i`-th blender of a synthetic chain: the blender, its child array,
    /// and its child records.
    const BLENDER_STRIDE: usize = 0x200;
    const CHILD_ARRAY: usize = 0x100;
    const CHILDREN: usize = 0x140;

    /// A Havok 2010 packfile of `blenders` `hkbBlenderGenerator`s: blender `i` has `fan(i)`
    /// children, each blending blender `target(i)`. Everything else is zero, which the readers
    /// take as "nothing authored".
    fn blender_chain(blenders: usize, fan: impl Fn(usize) -> usize, target: impl Fn(usize) -> usize) -> Vec<u8> {
        let l = &LAYOUT_2010;
        let mut builder = PackfileBuilder { data: vec![0u8; blenders * BLENDER_STRIDE + 0x40], ..Default::default() };
        let class = builder.class("hkbBlenderGenerator");
        for i in 0..blenders {
            let blender = i * BLENDER_STRIDE;
            builder.virtual_fixups.push((blender as u32, 0, class));
            let children = fan(i);
            if children == 0 {
                continue;
            }
            // The children array: a local pointer to its data, and the element count.
            builder.local.push(((blender + l.blender_children) as u32, (blender + CHILD_ARRAY) as u32));
            builder.data[blender + l.blender_children + 8..blender + l.blender_children + 12]
                .copy_from_slice(&(children as u32).to_le_bytes());
            for k in 0..children {
                let child = blender + CHILDREN + k * 0x40;
                // Array element -> child record; child record -> the generator it blends.
                builder.global.push(((blender + CHILD_ARRAY + k * 8) as u32, 1, child as u32));
                builder.global.push(((child + l.blender_child_generator) as u32, 1, (target(i) * BLENDER_STRIDE) as u32));
            }
        }
        // `Packfile` binary-searches both tables.
        builder.local.sort_unstable();
        builder.global.sort_unstable();
        builder.build()
    }

    /// Read the first blender of `bytes` the way `decode_behavior_graph` does, under a
    /// `limit`-step budget, handing back what the walk cost.
    fn read_first_blender(bytes: &[u8], limit: usize) -> (Result<Option<Generator>>, usize) {
        let file = Packfile::parse(bytes).expect("a well-formed packfile");
        let mut walk = Walk::new(limit);
        let (mut clip, mut content) = (None, StateContent::default());
        let read = read_generator(&file, 0, &mut clip, &mut content, 0, &mut walk);
        (read, walk.steps)
    }

    #[test]
    fn a_blender_that_blends_itself_is_refused_not_expanded() {
        // Two children, both pointing back at the blender: 2^32 nodes before the depth cap.
        let bytes = blender_chain(1, |_| 2, |_| 0);
        let (read, steps) = read_first_blender(&bytes, MAX_GRAPH_STEPS);
        assert!(matches!(read, Err(HkxError::InvalidData("behavior graph object contains itself"))), "{read:?}");
        assert!(steps < 10, "a cycle is caught on the first re-entry, not after {steps} steps");
    }

    #[test]
    fn a_cycle_through_two_blenders_is_refused() {
        let bytes = blender_chain(2, |_| 2, |i| 1 - i);
        let (read, _) = read_first_blender(&bytes, MAX_GRAPH_STEPS);
        assert!(matches!(read, Err(HkxError::InvalidData("behavior graph object contains itself"))), "{read:?}");
    }

    #[test]
    fn shared_descendants_past_the_step_budget_are_refused_in_bounded_work() {
        // 30 blenders in a line, each with two children that both name the next one, the last
        // with none: no cycle, but 2^30 reads without a budget.
        let bytes = blender_chain(31, |i| if i < 30 { 2 } else { 0 }, |i| i + 1);
        let (read, steps) = read_first_blender(&bytes, MAX_GRAPH_STEPS);
        assert!(matches!(read, Err(HkxError::InvalidData("behavior graph takes too many steps to read"))), "{read:?}");
        assert!(steps <= MAX_GRAPH_STEPS + 2, "the work stops at the budget, not {steps}");
    }

    #[test]
    fn a_shared_descendant_that_is_not_a_cycle_still_reads() {
        // Blender 0 has two children that both name blender 1, which has none. Blender 1 is
        // entered twice, once from each child; neither is a cycle, and the walk is exactly:
        // blender 0 (1) + its two array entries (2) + blender 1 twice (2).
        let bytes = blender_chain(2, |i| if i == 0 { 2 } else { 0 }, |_| 1);
        let (read, steps) = read_first_blender(&bytes, MAX_GRAPH_STEPS);
        assert!(matches!(read, Ok(Some(Generator::Blender(_)))), "{read:?}");
        assert_eq!(steps, 5);
    }

    #[test]
    fn the_walk_unwinds_so_a_sibling_may_revisit_what_a_cousin_left() {
        // After the first child's subtree is read and left, the same object under the second
        // child is not "active" any more: a cycle needs the object to be an ancestor.
        let mut walk = Walk::new(100);
        walk.enter(8).unwrap();
        walk.enter(16).unwrap();
        walk.leave();
        walk.enter(16).unwrap();
        walk.leave();
        assert!(walk.enter(8).is_err(), "the root is still an ancestor");
    }

    fn animations_archive() -> Option<byroredux_bsa::BsaArchive> {
        let env = byroredux_plugin::esm::test_paths::SKYRIM_SE_ENV;
        let dir = std::env::var(env)
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| byroredux_plugin::esm::test_paths::SKYRIM_SE_DEFAULT.to_owned());
        let path = std::path::Path::new(&dir).join("Skyrim - Animations.bsa");
        if !path.is_file() {
            if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
                panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but {path:?} is not a file");
            }
            eprintln!("SKIP: {} not found (no game data?)", path.display());
            return None;
        }
        Some(byroredux_bsa::BsaArchive::open(path).unwrap())
    }

    /// The swinging blade: its single root state holds a blender over two nested
    /// machines - one swinging the `Trigger` clip (held, looped or played once,
    /// raising `Apex` / `InWin*` / a sound), one naming Gamebryo sequences.
    /// `cargo test -p byroredux-hkx -- --ignored`
    #[test]
    #[ignore = "needs Skyrim SE game data on disk"]
    fn the_swinging_blade_graph_decodes_its_nested_machines() {
        let Some(archive) = animations_archive() else { return };
        let bytes = archive.extract(r"meshes\traps\swingingblade\behaviors\behavior00.hkx").expect("extract");
        let graph = decode_behavior_graph(&bytes).unwrap();
        let root = graph.machine.as_ref().expect("machine");
        assert_eq!(root.states.len(), 1);
        let state = &root.states[0];
        assert!(state.clip.is_none(), "the root state holds a blender, not a clip");
        assert_eq!(state.content.machines.len(), 2);
        let (swing, sequences) = (&state.content.machines[0], &state.content.machines[1]);
        assert_eq!(swing.states.len(), 3);
        let modes: Vec<_> = swing.states.iter().map(|s| s.clip.as_ref().unwrap().mode).collect();
        assert_eq!(modes, [ClipMode::UserControlled, ClipMode::Looping, ClipMode::SinglePlay]);
        let single = graph.event_id("Single").unwrap();
        assert!(swing.states[0].transitions.iter().any(|t| t.event == single && t.to_state == 2));
        let names: Vec<_> = sequences.states.iter().flat_map(|s| s.content.sequences.iter().map(String::as_str)).collect();
        assert_eq!(names, ["playanim02", "playanim01", "playanim02"]);
    }

    /// The Nordic ruins lever: a project -> character -> behavior chain whose
    /// graph is an eleven-state machine on named events, with a clip per state,
    /// end-of-clip trigger events and three wildcard "position" events.
    /// `cargo test -p byroredux-hkx -- --ignored`
    #[test]
    #[ignore = "needs Skyrim SE game data on disk"]
    fn the_nordic_lever_graph_decodes_end_to_end() {
        let Some(archive) = animations_archive() else { return };
        let base = r"meshes\dungeons\nordic\levers\ruinslever01\";
        let get = |name: &str| archive.extract(&format!("{base}{name}")).expect("extract");

        let characters = decode_project(&get("norlever01.hkx")).unwrap();
        assert_eq!(characters, [r"Characters\Character00.hkx"]);
        let character = decode_character(&get(&characters[0].to_ascii_lowercase())).unwrap();
        assert_eq!(character.behavior, r"Behaviors\Behavior00.hkx");
        assert_eq!(character.rig, r"CharacterAssets\NorLever01_Skeleton.hkx");
        assert_eq!(character.animations.len(), 6);

        let graph = decode_behavior_graph(&get(&character.behavior.to_ascii_lowercase())).unwrap();
        assert_eq!(graph.events.len(), 28);
        assert_eq!(graph.event_id("PullDown"), Some(0));
        let machine = graph.machine.as_ref().expect("state machine");
        assert_eq!(machine.start_state, 1);
        assert_eq!(machine.states.len(), 11);
        assert_eq!(machine.wildcard.len(), 3);

        let mid = machine.states.iter().find(|s| s.name == "MidPosition").unwrap();
        let pull_down = graph.event_id("PullDown").unwrap();
        let to_pulling = mid.transitions.iter().find(|t| t.event == pull_down).unwrap().to_state;
        let pulling = machine.states.iter().find(|s| s.id == to_pulling).unwrap();
        assert_eq!(pulling.name, "MidToPulled");
        let clip = pulling.clip.as_ref().unwrap();
        assert_eq!(clip.animation, r"Animations\NorLever01_PullDown.hkx");
        assert_eq!(clip.mode, ClipMode::SinglePlay);
        assert_eq!(clip.playback_speed, 2.0);
        let pulled = graph.event_id("Pulled").unwrap();
        assert!(clip.triggers.iter().any(|t| t.event == pulled && t.relative_to_end));
    }

    fn fo4_animations_archive() -> Option<byroredux_bsa::Ba2Archive> {
        let env = byroredux_plugin::esm::test_paths::FO4_ENV;
        let dir = std::env::var(env)
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| byroredux_plugin::esm::test_paths::FO4_DEFAULT.to_owned());
        let path = std::path::Path::new(&dir).join("Fallout4 - Animations.ba2");
        if !path.is_file() {
            if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
                panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but {path:?} is not a file");
            }
            eprintln!("SKIP: {} not found (no game data?)", path.display());
            return None;
        }
        Some(byroredux_bsa::Ba2Archive::open(path).unwrap())
    }

    /// Fallout 4 ships Havok 2014 packfiles (file version 11): the LED display's
    /// two-state graph and the player's eight-way walk blend decode with the
    /// `LAYOUT_2014` offsets - a state machine on named events, a clip per state,
    /// a blender over clips.
    /// `cargo test -p byroredux-hkx -- --ignored`
    #[test]
    #[ignore = "needs Fallout 4 game data on disk"]
    fn fallout_4_havok_2014_graphs_decode() {
        let Some(archive) = fo4_animations_archive() else { return };
        let led = archive
            .extract(r"meshes\setdressing\animated\leddisplay01\behaviors\behavior00.hkx")
            .expect("extract");
        let graph = decode_behavior_graph(&led).unwrap();
        let machine = graph.machine.as_ref().expect("state machine");
        assert_eq!(machine.states.len(), 2);
        let first = &machine.states[0];
        assert_eq!((first.id, first.name.as_str()), (5, "StartLevel"));
        assert!(machine.states.iter().any(|s| s.id == machine.start_state));
        let clip = machine
            .states
            .iter()
            .find_map(|state| state.clip.as_ref().or(state.content.blended_clips.first()))
            .expect("a state clip");
        assert!(clip.animation.starts_with("Animations\\"), "{}", clip.animation);
        assert!(clip.playback_speed.is_finite() && clip.playback_speed > 0.0);

        let walk = archive
            .extract(r"meshes\actors\character\behaviors\locomotion_8wayblend.hkx")
            .expect("extract");
        let graph = decode_behavior_graph(&walk).unwrap();
        assert_eq!(graph.events.len(), 26);
        assert_eq!(graph.event_id("FootLeft"), Some(0));
        let blended: usize = graph
            .machine
            .iter()
            .flat_map(|m| &m.states)
            .map(|s| s.content.blended_clips.len() + usize::from(s.clip.is_some()))
            .sum();
        assert!(blended >= 8, "the eight walk directions are clips of one blender: {blended}");

        // The humanoid MT graph idles in `PoseA_Idle1`, reached through its
        // `MTIdles` state's reference to `DialogueBehavior.hkx`.
        let mt = decode_behavior_graph(&archive.extract(r"meshes\actors\character\behaviors\mtbehavior.hkx").unwrap()).unwrap();
        let clip = mt
            .start_clip(&mut |reference| {
                let path = format!(r"meshes\actors\character\{}", reference.to_ascii_lowercase());
                decode_behavior_graph(&archive.extract(&path).ok()?).ok()
            })
            .expect("the MT idle clip");
        assert_eq!(clip.animation, "Animations\\PoseA_Idle1.hkt");
        assert_eq!(clip.mode, ClipMode::Looping);
    }

    fn machine_of(start: i32, states: Vec<State>) -> StateMachine {
        StateMachine { start_state: start, start_state_variable: None, states, wildcard: Vec::new() }
    }

    fn state_of(id: i32, clip: Option<&str>, content: StateContent) -> State {
        State {
            id,
            name: format!("S{id}"),
            transitions: Vec::new(),
            clip: clip.map(|animation| StateClip {
                animation: animation.to_owned(),
                mode: ClipMode::Looping,
                playback_speed: 1.0,
                start_time: 0.0,
                user_fraction: 0.0,
                triggers: Vec::new(),
                playback_speed_variable: None,
            }),
            content,
        }
    }

    /// The idle pose is found by following start states down, then nested machines,
    /// then referenced sub-behaviors; a self-referencing graph ends instead of hanging.
    #[test]
    fn start_clip_follows_start_states_nested_machines_and_references() {
        let leaf = BehaviorGraph {
            variables: Vec::new(),
            events: Vec::new(),
            machine: Some(machine_of(
                1,
                vec![state_of(0, Some("Animations\\Other.hkt"), StateContent::default()), state_of(1, Some("Animations\\Idle.hkt"), StateContent::default())],
            )),
        };
        let root = BehaviorGraph {
            variables: Vec::new(),
            events: Vec::new(),
            machine: Some(machine_of(
                0,
                vec![state_of(
                    0,
                    None,
                    StateContent {
                        // The first nested machine has nothing; the reference is next.
                        machines: vec![machine_of(0, vec![state_of(0, None, StateContent::default())])],
                        behavior_references: vec!["Behaviors\\Leaf.hkx".into()],
                        ..Default::default()
                    },
                )],
            )),
        };
        let mut loaded = Vec::new();
        let clip = root
            .start_clip(&mut |path| {
                loaded.push(path.to_owned());
                Some(leaf.clone())
            })
            .expect("a clip");
        assert_eq!(clip.animation, "Animations\\Idle.hkt", "the start state, not the first state");
        assert_eq!(loaded, ["Behaviors\\Leaf.hkx"]);

        let looping = BehaviorGraph {
            variables: Vec::new(),
            events: Vec::new(),
            machine: Some(machine_of(
                0,
                vec![state_of(0, None, StateContent { behavior_references: vec!["Self.hkx".into()], ..Default::default() })],
            )),
        };
        let mut hops = 0;
        assert!(looping
            .start_clip(&mut |_| {
                hops += 1;
                Some(looping.clone())
            })
            .is_none());
        assert!(hops <= MAX_REFERENCE_HOPS);
    }

    /// A referenced behavior becomes a machine of the referring state; its events join
    /// the parent's table by name, so an event of the parent reaches its transitions.
    #[test]
    fn inline_references_nests_the_sub_behavior_and_renumbers_its_events_by_name() {
        let transition = |event, to_state| Transition { event, to_state, priority: 0, flags: 0, ..Default::default() };
        let mut child_state = state_of(0, Some("Animations\\Idle.hkt"), StateContent::default());
        child_state.transitions = vec![transition(1, 1)]; // "C" in the child's own table
        let child_wildcard = vec![transition(0, 0)]; // "B"
        let child = BehaviorGraph {
            variables: Vec::new(),
            events: vec!["B".into(), "C".into()],
            machine: Some(StateMachine {
                start_state: 0,
                start_state_variable: None,
                states: vec![child_state, state_of(1, Some("Animations\\Other.hkt"), StateContent::default())],
                wildcard: child_wildcard,
            }),
        };
        let mut parent = BehaviorGraph {
            variables: Vec::new(),
            events: vec!["A".into(), "B".into()],
            machine: Some(machine_of(
                0,
                vec![state_of(0, None, StateContent { behavior_references: vec!["Child.hkx".into()], ..Default::default() })],
            )),
        };
        parent.inline_references(&mut |path| (path == "Child.hkx").then(|| child.clone()));
        assert_eq!(parent.events, ["A", "B", "C"], "the child's new event joins the table");
        let state = &parent.machine.as_ref().unwrap().states[0];
        assert_eq!(state.content.reference_machines, [Some(0)], "the reference now names the nested machine");
        let inner = &state.content.machines[0];
        assert_eq!(inner.states[0].transitions[0].event, 2, "C is the parent's event 2");
        assert_eq!(inner.wildcard[0].event, 1, "B is the parent's existing event 1");
        // An unloadable reference stays a reference.
        let mut orphan = BehaviorGraph {
            variables: Vec::new(),
            events: Vec::new(),
            machine: Some(machine_of(
                0,
                vec![state_of(0, None, StateContent { behavior_references: vec!["Gone.hkx".into()], ..Default::default() })],
            )),
        };
        orphan.inline_references(&mut |_| None);
        let orphan = orphan.machine.unwrap();
        assert_eq!(orphan.states[0].content.behavior_references, ["Gone.hkx"]);
        assert_eq!(orphan.states[0].content.reference_machines, [None]);
    }

    /// A behavior that names itself would otherwise stack its whole inlining budget of machines
    /// (64) under one another, and the walkers over the result recurse once per level.
    #[test]
    fn a_behavior_that_references_itself_stops_at_the_machine_nesting_limit() {
        let looping = BehaviorGraph {
            variables: Vec::new(),
            events: Vec::new(),
            machine: Some(machine_of(
                0,
                vec![state_of(0, None, StateContent { behavior_references: vec!["Self.hkx".into()], ..Default::default() })],
            )),
        };
        let mut graph = looping.clone();
        let mut resolved = 0;
        graph.inline_references(&mut |_| {
            resolved += 1;
            Some(looping.clone())
        });
        let height = machine_height(graph.machine.as_ref().unwrap());
        assert!(height > 1, "the first levels still inline");
        assert!(height <= MAX_MACHINE_NESTING, "{height} machines deep");
        assert!(resolved <= MAX_INLINED_REFERENCES);
    }

    /// Event and variable names join the parent's table case-insensitively, the first spelling
    /// winning, whichever order the sub-behaviors come in.
    #[test]
    fn inlined_names_merge_case_insensitively_into_the_parents_tables() {
        let transition = |event, to_state| Transition { event, to_state, priority: 0, flags: 0, ..Default::default() };
        let variable = |name: &str| Variable { name: name.into(), kind: VariableKind::Real, initial: 0.0 };
        let mut child_state = state_of(0, None, StateContent::default());
        child_state.transitions = vec![transition(0, 0), transition(1, 0)];
        let child = BehaviorGraph {
            variables: vec![variable("speed"), variable("Turn")],
            events: vec!["go".into(), "Halt".into()],
            machine: Some(machine_of(0, vec![child_state])),
        };
        let mut parent = BehaviorGraph {
            variables: vec![variable("Speed")],
            events: vec!["Go".into()],
            machine: Some(machine_of(
                0,
                vec![state_of(0, None, StateContent { behavior_references: vec!["Child.hkx".into()], ..Default::default() })],
            )),
        };
        parent.inline_references(&mut |_| Some(child.clone()));
        assert_eq!(parent.events, ["Go", "Halt"], "\"go\" is the parent's \"Go\"");
        let names: Vec<&str> = parent.variables.iter().map(|variable| variable.name.as_str()).collect();
        assert_eq!(names, ["Speed", "Turn"]);
        let inner = &parent.machine.as_ref().unwrap().states[0].content.machines[0];
        assert_eq!([inner.states[0].transitions[0].event, inner.states[0].transitions[1].event], [0, 1]);
    }

    /// 64 sub-behaviors that each name the same 4,096 events: a linear scan per name is tens
    /// of billions of comparisons, the name index is a few hundred thousand lookups.
    #[test]
    fn inlining_many_sub_behaviors_with_many_events_is_not_quadratic() {
        let events: Vec<String> = (0..MAX_EVENTS).map(|index| format!("Event{index}")).collect();
        let child = BehaviorGraph {
            variables: Vec::new(),
            events: events.clone(),
            machine: Some(machine_of(0, vec![state_of(0, None, StateContent::default())])),
        };
        let references: Vec<String> = (0..MAX_INLINED_REFERENCES).map(|index| format!("Child{index}.hkx")).collect();
        let mut parent = BehaviorGraph {
            variables: Vec::new(),
            events: Vec::new(),
            machine: Some(machine_of(
                0,
                vec![state_of(0, None, StateContent { behavior_references: references, ..Default::default() })],
            )),
        };
        let started = std::time::Instant::now();
        parent.inline_references(&mut |_| Some(child.clone()));
        assert_eq!(parent.events, events, "every child's events are the same 4,096 names");
        assert!(started.elapsed() < std::time::Duration::from_secs(3), "took {:?}", started.elapsed());
    }

    #[test]
    fn a_binding_offset_names_the_element_before_it_and_none_for_zero_or_negative() {
        assert_eq!(bone_of_binding(1), Some(0));
        assert_eq!(bone_of_binding(5), Some(4));
        assert_eq!(bone_of_binding(0), None);
        // `i32::MIN - 1` used to overflow (a panic in the dev profile, which checks).
        assert_eq!(bone_of_binding(0x8000_0000), None);
        assert_eq!(bone_of_binding(u32::MAX), None, "-1");
    }

    /// A Havok 2010 `hkbBoneWeightArray` of two weights, bound by one `boneWeights` binding
    /// of variable 3 to the element `offset_plus_one` names.
    fn bone_weight_array(offset_plus_one: u32) -> Vec<u8> {
        let mut builder = PackfileBuilder { data: vec![0u8; 0x200], ..Default::default() };
        // The array object at 0: weights at +0x30 (local pointer to 0x100, count 2), bindings at +0x10.
        builder.local.push((BONE_WEIGHTS as u32, 0x100));
        builder.data[BONE_WEIGHTS + 8..BONE_WEIGHTS + 12].copy_from_slice(&2u32.to_le_bytes());
        builder.data[0x100..0x104].copy_from_slice(&0.25f32.to_le_bytes());
        builder.data[0x104..0x108].copy_from_slice(&0.75f32.to_le_bytes());
        builder.global.push((BINDABLE_BINDING_SET as u32, 1, 0x140));
        // The binding set at 0x140: one binding at 0x180.
        builder.local.push(((0x140 + BINDING_SET_BINDINGS) as u32, 0x180));
        builder.data[0x140 + BINDING_SET_BINDINGS + 8..0x140 + BINDING_SET_BINDINGS + 12].copy_from_slice(&1u32.to_le_bytes());
        // The binding: member path "boneWeights" (a string at 0x1C0), variable 3, the offset.
        builder.data[0x1C0..0x1CC].copy_from_slice(b"boneWeights\0");
        builder.local.push(((0x180 + BINDING_MEMBER_PATH) as u32, 0x1C0));
        builder.data[0x180 + BINDING_VARIABLE_INDEX..0x180 + BINDING_VARIABLE_INDEX + 4].copy_from_slice(&3u32.to_le_bytes());
        builder.data[0x180 + BINDING_OFFSET_IN_ARRAY..0x180 + BINDING_OFFSET_IN_ARRAY + 4].copy_from_slice(&offset_plus_one.to_le_bytes());
        builder.local.sort_unstable();
        builder.global.sort_unstable();
        builder.build()
    }

    #[test]
    fn a_bone_weight_binding_with_an_overflowing_offset_is_ignored_not_a_panic() {
        let read = |offset| {
            let bytes = bone_weight_array(offset);
            let file = Packfile::parse(&bytes).expect("a well-formed packfile");
            read_bone_weights(&file, 0).expect("the array decodes").bound
        };
        assert_eq!(read(2), [(1, 3)], "the second weight is driven by variable 3");
        assert!(read(0x8000_0000).is_empty());
        assert!(read(0).is_empty());
        assert!(read(3).is_empty(), "past the two weights");
    }
}
