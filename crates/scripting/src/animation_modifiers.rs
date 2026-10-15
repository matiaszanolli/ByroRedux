//! The modifiers of a behavior graph that act on the graph itself (M47.2): the variables it
//! holds and the events it raises.
//!
//! A modifier is *active* while the generator it wraps is on the graph's active path (a
//! state it sits under is current). What each does is the engine's own description of it:
//!
//! - `BSAssignVariablesModifier` - "A float variable to set on activate. The float value
//!   to set m_floatVariable1 to on activate.": on activation each bound variable takes its
//!   value (the vanilla `Set_iDynamicLoopStartState_to_1` and `MTDynamicControlsIdle0_AVM`).
//! - `BSModifyOnceModifier` - "The modifier to run on activation" (and on deactivation).
//! - `hkbEventsFromRangeModifier` - "The value that is checked against the ranges to decide
//!   which events should be sent", each range running up from the previous range's upper
//!   bound ("A lower bound of all of the range intervals" for the first), "The event to send
//!   when this range is entered", or every frame the input is in it.
//! - `hkbEvaluateExpressionModifier` - "Expression of the form 'variablename = expression'
//!   or 'eventname = boolean expression'", with the four event modes (once, on true, on
//!   false-to-true, every frame once true).
//! - `BSEventOnDeactivateModifier` - "Event to be raised when this modifier deactivates."
//! - `BSEventEveryNEventsModifier` - "Number of times to get m_eventToCheckFor before
//!   sending m_eventToSend."
//! - `hkbEventDrivenModifier` - "Event used to activate the wrapped modifier. Event used to
//!   deactivate the wrapped modifier."; on from the start when `active_by_default`.
//! - `hkbTimerModifier` / `BSTimerModifier` - "An event is sent at this time." / "The event
//!   to send when the timer goes off."; `BSRandomAlarmModifier` - a time drawn uniformly
//!   between "the minimum amount of time before the timer goes off" and the maximum, started
//!   over on expiry if "Whether or not to reset the alarm when the timer expires".
//! - `BSGetTimeStepModifier` - "The current time step.": the variable bound to its
//!   `timeStep` takes the frame's `dt` (`fTimeStep`).
//! - `hkbModifierList` - its modifiers.
//!
//! Not run: the modifiers that pose the skeleton (look-at, twist, foot IK, ragdoll, jiggle),
//! the ones whose formula the files do not give (`BSSpeedSamplerModifier`,
//! `BSInterpValueModifier`, `hkbDampingModifier`), and `BSIsActiveModifier` (booleans for
//! the engine's own code).

use byroredux_hkx::{BehaviorGraph, Modifier, ModifierNode, Statement, VariableKind};
use std::collections::HashMap;
use std::sync::Arc;

/// The graph's variables, as a modifier sees them.
pub(crate) struct Vars<'a> {
    pub values: &'a mut Vec<f32>,
    pub dirty: &'a mut bool,
    pub graph: &'a BehaviorGraph,
}

impl Vars<'_> {
    fn get(&self, name: &str) -> Option<f32> {
        self.values.get(self.graph.variable_id(name)?).copied()
    }

    /// Write variable `index`, keeping its kind (a bool stays 0/1, an int whole).
    pub(crate) fn set(&mut self, index: usize, value: f32) {
        let Some(variable) = self.graph.variables.get(index) else { return };
        let Some(coerced) = coerce(variable.kind, value) else { return };
        if self.values[index] != coerced {
            self.values[index] = coerced;
            *self.dirty = true;
        }
    }
}

/// `value` as a variable of `kind` holds it; `None` for a kind that is not modelled.
pub(crate) fn coerce(kind: VariableKind, value: f32) -> Option<f32> {
    if !value.is_finite() {
        return None;
    }
    match kind {
        VariableKind::Bool => Some(f32::from(value != 0.0)),
        VariableKind::Int => Some(value.trunc()),
        VariableKind::Real => Some(value),
        VariableKind::Other => None,
    }
}

#[derive(Debug, Clone, Default)]
struct Runtime {
    /// An event-driven modifier's wrapped modifier is on.
    enabled: bool,
    /// The range an events-from-range modifier's input was in last frame.
    range: Option<usize>,
    /// Events seen by an every-N modifier.
    count: u32,
    /// Per expression: parsed once, and what the last frames made of it.
    expressions: Vec<ExpressionRuntime>,
    /// A timer's seconds since it activated (or last went off), and how many times a random
    /// alarm has been drawn.
    elapsed: f32,
    fired: bool,
    draws: u32,
    alarm: Option<f32>,
}

#[derive(Debug, Clone, Default)]
struct ExpressionRuntime {
    statement: Option<Statement>,
    was_true: bool,
    fired: bool,
}

/// The modifiers active on a graph, and what each remembers between frames.
#[derive(Debug, Clone, Default)]
pub(crate) struct ModifierState {
    active: Vec<Arc<ModifierNode>>,
    runtime: HashMap<u32, Runtime>,
}

impl ModifierState {
    /// Make `now` the active set: the new ones activate, the gone ones deactivate. Events
    /// raised on the way are pushed to `events`.
    pub(crate) fn sync(&mut self, now: Vec<Arc<ModifierNode>>, vars: &mut Vars, events: &mut Vec<i32>) {
        let gone: Vec<Arc<ModifierNode>> =
            self.active.iter().filter(|old| !now.iter().any(|new| new.id == old.id)).cloned().collect();
        let fresh: Vec<Arc<ModifierNode>> =
            now.iter().filter(|new| !self.active.iter().any(|old| old.id == new.id)).cloned().collect();
        for node in gone {
            self.deactivate(&node, vars, events);
        }
        self.active = now;
        for node in fresh {
            self.activate(&node, vars);
        }
    }

    // `events` is deliberately absent: no activate arm pushes one (the
    // event modifiers fire on deactivate / timer ticks), so threading it
    // here was clippy's only-used-in-recursion catch.
    fn activate(&mut self, node: &Arc<ModifierNode>, vars: &mut Vars) {
        let runtime = self.runtime.entry(node.id).or_default();
        *runtime = Runtime::default();
        match &node.kind {
            Modifier::AssignVariables { floats, ints } => {
                for (variable, value) in floats {
                    vars.set(*variable, *value);
                }
                for (variable, value) in ints {
                    vars.set(*variable, *value as f32);
                }
            }
            Modifier::ModifyOnce {
                on_activate: Some(inner),
                ..
            } => self.activate(inner, vars),
            Modifier::ModifyOnce {
                on_activate: None,
                ..
            } => {}
            Modifier::EvaluateExpression { expressions } => {
                runtime.expressions = expressions
                    .iter()
                    .map(|expression| ExpressionRuntime { statement: Statement::parse(&expression.text), ..Default::default() })
                    .collect();
            }
            Modifier::EventDriven { active_by_default, inner, .. } => {
                runtime.enabled = *active_by_default;
                if *active_by_default {
                    if let Some(inner) = inner {
                        self.activate(inner, vars);
                    }
                }
            }
            Modifier::List(items) => {
                for item in items {
                    self.activate(item, vars);
                }
            }
            Modifier::RandomAlarm { min_seconds, max_seconds, .. } => {
                runtime.alarm = Some(draw_alarm(node.id, runtime.draws, *min_seconds, *max_seconds));
            }
            Modifier::Timer { .. }
            | Modifier::TimeStep { .. }
            | Modifier::EventsFromRange { .. }
            | Modifier::EventOnDeactivate { .. }
            | Modifier::EventEveryNEvents { .. } => {}
        }
    }

    fn deactivate(&mut self, node: &Arc<ModifierNode>, vars: &mut Vars, events: &mut Vec<i32>) {
        match &node.kind {
            Modifier::EventOnDeactivate { event } => push_event(events, *event),
            Modifier::ModifyOnce {
                on_deactivate: Some(inner),
                ..
            } => self.activate(inner, vars),
            Modifier::ModifyOnce {
                on_deactivate: None,
                ..
            } => {}
            Modifier::EventDriven { inner, .. } => {
                let enabled = self.runtime.get(&node.id).is_some_and(|runtime| runtime.enabled);
                if let (true, Some(inner)) = (enabled, inner) {
                    self.deactivate(inner, vars, events);
                }
            }
            Modifier::List(items) => {
                for item in items {
                    self.deactivate(item, vars, events);
                }
            }
            _ => {}
        }
        self.runtime.remove(&node.id);
    }

    /// An event was raised: event-driven modifiers switch their wrapped modifier, and
    /// every-N modifiers count it.
    pub(crate) fn on_event(&mut self, event: i32, vars: &mut Vars, events: &mut Vec<i32>) {
        let active = self.active.clone();
        for node in &active {
            self.event_into(node, event, vars, events);
        }
    }

    fn event_into(&mut self, node: &Arc<ModifierNode>, event: i32, vars: &mut Vars, events: &mut Vec<i32>) {
        match &node.kind {
            Modifier::EventDriven { activate, deactivate, inner, .. } => {
                let enabled = self.runtime.get(&node.id).is_some_and(|runtime| runtime.enabled);
                if event == *activate && !enabled {
                    self.runtime.entry(node.id).or_default().enabled = true;
                    if let Some(inner) = inner {
                        self.activate(inner, vars);
                    }
                } else if event == *deactivate && enabled {
                    if let Some(inner) = inner {
                        self.deactivate(inner, vars, events);
                    }
                    self.runtime.entry(node.id).or_default().enabled = false;
                } else if let (true, Some(inner)) = (enabled, inner) {
                    self.event_into(inner, event, vars, events);
                }
            }
            Modifier::EventEveryNEvents { check, send, count } => {
                if event == *check {
                    let runtime = self.runtime.entry(node.id).or_default();
                    runtime.count += 1;
                    if runtime.count >= (*count).max(1) {
                        runtime.count = 0;
                        push_event(events, *send);
                    }
                }
            }
            Modifier::List(items) => {
                for item in items {
                    self.event_into(item, event, vars, events);
                }
            }
            Modifier::ModifyOnce { .. } | Modifier::AssignVariables { .. } | Modifier::EventsFromRange { .. } => {}
            Modifier::EvaluateExpression { .. }
            | Modifier::EventOnDeactivate { .. }
            | Modifier::Timer { .. }
            | Modifier::TimeStep { .. }
            | Modifier::RandomAlarm { .. } => {}
        }
    }

    /// One frame of `dt` seconds: the modifiers that look at the variables or the clock run.
    pub(crate) fn tick(&mut self, dt: f32, vars: &mut Vars, events: &mut Vec<i32>) {
        let active = self.active.clone();
        for node in &active {
            self.tick_node(node, dt, vars, events);
        }
    }

    fn tick_node(&mut self, node: &Arc<ModifierNode>, dt: f32, vars: &mut Vars, events: &mut Vec<i32>) {
        match &node.kind {
            Modifier::TimeStep { variable } => {
                if let Some(variable) = variable {
                    vars.set(*variable, dt);
                }
            }
            Modifier::Timer { alarm_seconds, event } => {
                let runtime = self.runtime.entry(node.id).or_default();
                runtime.elapsed += dt.max(0.0);
                if !runtime.fired && runtime.elapsed >= *alarm_seconds {
                    runtime.fired = true;
                    push_event(events, *event);
                }
            }
            Modifier::RandomAlarm { min_seconds, max_seconds, event, reset } => {
                let runtime = self.runtime.entry(node.id).or_default();
                runtime.elapsed += dt.max(0.0);
                let alarm = *runtime.alarm.get_or_insert_with(|| draw_alarm(node.id, 0, *min_seconds, *max_seconds));
                if runtime.elapsed >= alarm && (!runtime.fired || *reset) {
                    push_event(events, *event);
                    runtime.fired = true;
                    if *reset {
                        runtime.elapsed = 0.0;
                        runtime.draws += 1;
                        runtime.alarm = Some(draw_alarm(node.id, runtime.draws, *min_seconds, *max_seconds));
                    }
                }
            }
            Modifier::EventsFromRange { input, input_variable, lower_bound, ranges } => {
                let value = input_variable.and_then(|variable| vars.values.get(variable)).copied().unwrap_or(*input);
                // The range the value falls in: the first whose upper bound it does not pass.
                let current = ranges.iter().position(|range| value <= range.upper).filter(|_| value.is_finite());
                let _ = lower_bound;
                let runtime = self.runtime.entry(node.id).or_default();
                if let Some(index) = current {
                    let range = &ranges[index];
                    if range.when_in_range || runtime.range != Some(index) {
                        push_event(events, range.event);
                    }
                }
                runtime.range = current;
            }
            Modifier::EvaluateExpression { expressions } => {
                for (index, expression) in expressions.iter().enumerate() {
                    let Some(statement) = self.runtime.get(&node.id).and_then(|runtime| runtime.expressions.get(index)).and_then(|e| e.statement.clone()) else {
                        continue;
                    };
                    let value = statement.value.value(&|name| vars.get(name));
                    let Some(value) = value else { continue };
                    if let Some(variable) = vars.graph.variable_id(&statement.target) {
                        vars.set(variable, value);
                        continue;
                    }
                    let Some(event) = vars.graph.event_id(&statement.target) else { continue };
                    let truth = value != 0.0;
                    let Some(state) = self.runtime.get_mut(&node.id).and_then(|runtime| runtime.expressions.get_mut(index)) else { continue };
                    let send = match expression.event_mode {
                        0 => truth && !state.fired,
                        1 => truth,
                        2 => truth && !state.was_true,
                        _ => truth || state.fired,
                    };
                    if truth {
                        state.fired = true;
                    }
                    state.was_true = truth;
                    if send {
                        push_event(events, event);
                    }
                }
            }
            Modifier::EventDriven { inner, .. } => {
                let enabled = self.runtime.get(&node.id).is_some_and(|runtime| runtime.enabled);
                if let (true, Some(inner)) = (enabled, inner) {
                    self.tick_node(inner, dt, vars, events);
                }
            }
            Modifier::List(items) => {
                for item in items {
                    self.tick_node(item, dt, vars, events);
                }
            }
            Modifier::AssignVariables { .. }
            | Modifier::ModifyOnce { .. }
            | Modifier::EventOnDeactivate { .. }
            | Modifier::EventEveryNEvents { .. } => {}
        }
    }
}

/// A deterministic draw in `[min, max]` for a random alarm: a hash of the modifier and how
/// many draws it has made, so a replay sees the same times.
fn draw_alarm(id: u32, draw: u32, min: f32, max: f32) -> f32 {
    let mut x = (u64::from(id) << 32 | u64::from(draw)).wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    let fraction = (x >> 40) as f32 / (1u64 << 24) as f32;
    let (min, max) = if max >= min { (min, max) } else { (max, min) };
    min + (max - min) * fraction
}

fn push_event(events: &mut Vec<i32>, event: i32) {
    if event >= 0 {
        events.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_hkx::{EventRange, ExpressionData, Variable};

    fn graph(variables: &[(&str, VariableKind)], events: &[&str]) -> BehaviorGraph {
        BehaviorGraph {
            variables: variables.iter().map(|(name, kind)| Variable { name: (*name).into(), kind: *kind, initial: 0.0 }).collect(),
            events: events.iter().map(|name| (*name).into()).collect(),
            machine: None,
        }
    }

    fn node(id: u32, kind: Modifier) -> Arc<ModifierNode> {
        Arc::new(ModifierNode { id, kind })
    }

    struct Harness {
        graph: BehaviorGraph,
        values: Vec<f32>,
        dirty: bool,
        state: ModifierState,
        events: Vec<i32>,
    }

    impl Harness {
        fn new(variables: &[(&str, VariableKind)], events: &[&str]) -> Self {
            Self { graph: graph(variables, events), values: vec![0.0; variables.len()], dirty: false, state: ModifierState::default(), events: Vec::new() }
        }

        fn run(&mut self, f: impl FnOnce(&mut ModifierState, &mut Vars, &mut Vec<i32>)) -> Vec<i32> {
            let mut vars = Vars { values: &mut self.values, dirty: &mut self.dirty, graph: &self.graph };
            f(&mut self.state, &mut vars, &mut self.events);
            std::mem::take(&mut self.events)
        }
    }

    /// `BSModifyOnceModifier` -> `BSAssignVariablesModifier`, the vanilla `..._MOM` shape:
    /// the variable takes its value when the state is entered, once.
    #[test]
    fn assign_variables_set_on_activation_through_a_modify_once() {
        let mut h = Harness::new(&[("iControlsIdleSync", VariableKind::Int)], &[]);
        let assign = node(2, Modifier::AssignVariables { floats: Vec::new(), ints: vec![(0, 3)] });
        let once = node(1, Modifier::ModifyOnce { on_activate: Some(assign), on_deactivate: None });
        h.run(|state, vars, events| state.sync(vec![node(0, Modifier::List(vec![once]))], vars, events));
        assert_eq!(h.values[0], 3.0);
        assert!(h.dirty);
        // Still active next frame: it does not assign again.
        h.values[0] = 9.0;
        h.run(|state, vars, events| {
            let again = node(0, Modifier::List(Vec::new()));
            state.sync(vec![again], vars, events);
        });
        assert_eq!(h.values[0], 9.0, "an unchanged id is not re-activated");
    }

    #[test]
    fn events_on_deactivate_and_every_n_events() {
        let mut h = Harness::new(&[], &["Step", "Sound", "PickNewIdle"]);
        let every = node(1, Modifier::EventEveryNEvents { check: 0, send: 1, count: 2 });
        let deactivate = node(2, Modifier::EventOnDeactivate { event: 2 });
        h.run(|state, vars, events| state.sync(vec![every.clone(), deactivate.clone()], vars, events));
        assert!(h.run(|state, vars, events| state.on_event(0, vars, events)).is_empty(), "the first of two");
        assert_eq!(h.run(|state, vars, events| state.on_event(0, vars, events)), vec![1], "the second sends");
        assert!(h.run(|state, vars, events| state.on_event(0, vars, events)).is_empty(), "the count starts over");
        assert_eq!(h.run(|state, vars, events| state.sync(Vec::new(), vars, events)), vec![2], "leaving raises PickNewIdle");
    }

    /// The vanilla `JumpLocomotion_EFR` / `SpeedSmoothed` shape: crossing into a range sends
    /// its event once; staying in it sends nothing; a `when in range` range sends every frame.
    #[test]
    fn events_from_range_send_on_entering_a_range() {
        let mut h = Harness::new(&[("SpeedSmoothed", VariableKind::Real)], &["walkStart", "runStart"]);
        let ranges = vec![
            EventRange { upper: 150.0, event: 0, when_in_range: false },
            EventRange { upper: 600.0, event: 1, when_in_range: false },
        ];
        let modifier = node(1, Modifier::EventsFromRange { input: 0.0, input_variable: Some(0), lower_bound: 0.0, ranges });
        h.run(|state, vars, events| state.sync(vec![modifier.clone()], vars, events));
        h.values[0] = 80.0;
        assert_eq!(h.run(|state, vars, events| state.tick(0.1, vars, events)), vec![0], "entering the walk range");
        assert!(h.run(|state, vars, events| state.tick(0.1, vars, events)).is_empty(), "still in it");
        h.values[0] = 300.0;
        assert_eq!(h.run(|state, vars, events| state.tick(0.1, vars, events)), vec![1], "into the run range");
        h.values[0] = 5000.0;
        assert!(h.run(|state, vars, events| state.tick(0.1, vars, events)).is_empty(), "past every range");
    }

    #[test]
    fn expressions_assign_variables_and_send_events_by_mode() {
        let mut h = Harness::new(&[("iCombatStance", VariableKind::Int), ("fSpeed", VariableKind::Real)], &["weaponDraw"]);
        let expressions = vec![
            ExpressionData { text: "iCombatStance = fSpeed * 2".into(), event_mode: 0 },
            ExpressionData { text: "weaponDraw = (iCombatStance == 4)".into(), event_mode: 2 },
        ];
        let modifier = node(1, Modifier::EvaluateExpression { expressions });
        h.run(|state, vars, events| state.sync(vec![modifier.clone()], vars, events));
        h.values[1] = 2.0;
        // Expressions run in order: the stance is assigned (an int stays whole), then the
        // event expression reads it - false to true, so it is sent.
        assert_eq!(h.run(|state, vars, events| state.tick(0.1, vars, events)), vec![0]);
        assert_eq!(h.values[0], 4.0);
        assert!(h.run(|state, vars, events| state.tick(0.1, vars, events)).is_empty(), "still true: not again");
        h.values[1] = 1.0;
        assert!(h.run(|state, vars, events| state.tick(0.1, vars, events)).is_empty(), "false: nothing");
        h.values[1] = 2.0;
        assert_eq!(h.run(|state, vars, events| state.tick(0.1, vars, events)), vec![0], "false to true again");
    }

    #[test]
    fn a_timer_sends_its_event_once_after_its_time_and_a_random_alarm_when_drawn() {
        let mut h = Harness::new(&[], &["Alarm", "Random"]);
        let timer = node(1, Modifier::Timer { alarm_seconds: 0.25, event: 0 });
        let random = node(2, Modifier::RandomAlarm { min_seconds: 1.0, max_seconds: 3.5, event: 1, reset: true });
        h.run(|state, vars, events| state.sync(vec![timer.clone(), random.clone()], vars, events));
        let mut fired = Vec::new();
        for frame in 0..200 {
            for event in h.run(|state, vars, events| state.tick(0.1, vars, events)) {
                fired.push((frame, event));
            }
        }
        let alarms: Vec<_> = fired.iter().filter(|(_, event)| *event == 0).collect();
        assert_eq!(alarms.len(), 1, "a timer goes off once");
        assert_eq!(alarms[0].0, 2, "after 0.25 s at 0.1 s a frame: the third frame");
        let randoms: Vec<i32> = fired.iter().filter(|(_, event)| *event == 1).map(|(frame, _)| *frame).collect();
        assert!((5..=20).contains(&randoms.len()), "a 1 - 3.5 s alarm that resets goes off 6 to 20 times in 20 s: {randoms:?}");
        assert!(randoms[0] >= 9 && randoms[0] <= 35, "the first draw lies in [1.0, 3.5] s: {randoms:?}");
        for pair in randoms.windows(2) {
            let gap = pair[1] - pair[0];
            assert!((9..=36).contains(&gap), "every draw lies in [1.0, 3.5] s: gap of {gap} frames");
        }
        for pair in 0..64 {
            let draw = draw_alarm(7, pair, 1.0, 3.5);
            assert!((1.0..=3.5).contains(&draw));
        }
        assert_eq!(draw_alarm(7, 3, 1.0, 3.5), draw_alarm(7, 3, 1.0, 3.5), "deterministic");
    }

    #[test]
    fn the_time_step_modifier_writes_dt_to_its_variable() {
        let mut h = Harness::new(&[("fTimeStep", VariableKind::Real)], &[]);
        let step = node(1, Modifier::TimeStep { variable: Some(0) });
        h.run(|state, vars, events| state.sync(vec![step.clone()], vars, events));
        h.run(|state, vars, events| state.tick(0.016, vars, events));
        assert_eq!(h.values[0], 0.016);
    }

    #[test]
    fn an_event_driven_modifier_wraps_its_modifier_between_its_events() {
        let mut h = Harness::new(&[("iMode", VariableKind::Int)], &["On", "Off"]);
        let assign = node(2, Modifier::AssignVariables { floats: Vec::new(), ints: vec![(0, 7)] });
        let driven = node(1, Modifier::EventDriven { activate: 0, deactivate: 1, active_by_default: false, inner: Some(assign) });
        h.run(|state, vars, events| state.sync(vec![driven.clone()], vars, events));
        assert_eq!(h.values[0], 0.0, "off until its activate event");
        h.run(|state, vars, events| state.on_event(0, vars, events));
        assert_eq!(h.values[0], 7.0, "On activated the wrapped modifier");
    }
}
