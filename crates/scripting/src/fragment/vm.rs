//! The Papyrus VM (M47.2): runs a fragment the effect lowering declines.
//!
//! The lowering in [`crate::translate::effects`] turns a fragment into a flat
//! effect list ahead of time, which is exact but cannot express a value it
//! only learns at run time: an object local assigned from a call
//! (`ObjectReference x = Alias.GetRef()`), a quest-script function that
//! returns a value, a loop. This module interprets such a fragment over its
//! decompiled AST instead. It owns only control flow and values; every native
//! still goes through the effect runtime:
//!
//! * a native *statement* (`x.Enable()`) is re-written with its evaluated
//!   receiver and arguments bound to synthetic names — a form as a property
//!   of a synthetic VMAD, a quest or the player through
//!   [`NativeBindings`], numbers as literals — and classified by the same
//!   primitive table the lowering uses, then applied by `apply_effects`;
//! * a native *read* (`x.IsDisabled()`, `Q.GetStage()`) is classified the
//!   same way as a [`crate::translate::effects::ValueExpr`] or as guards and
//!   evaluated by the fragment runtime;
//! * the object reads no effect models — alias references, `GetPlayer`,
//!   `GetLinkedRef`, base objects, `GetOwningQuest` — are read here.
//!
//! A call into a quest's script (`kmyQuest.Startup()`, or one returning a
//! value) runs that function here too, against the quest's own VMAD.
//!
//! Script state follows the CK's *States (Papyrus)*: a call runs the version
//! in the instance's current state down the `extends` chain, then the empty
//! state's; an instance starts in its `Auto State`. The compiler's generated
//! `GotoState` / `GetState` keep the state in `::State`, stored per script
//! among its owner's script variables. Script variables — numbers in
//! [`crate::QuestScriptVariables`], objects and strings in
//! [`crate::QuestScriptObjects`] — are keyed by the object the script is
//! attached to; an unwritten one reads what the script declares.
//!
//! `Utility.Wait` suspends the run anywhere in a statement call chain; the
//! frames ride an `Effect::Interpret` resume on the continuation queue. The
//! per-frame statement budget ([`super::budget`], #5763) suspends a run the
//! same way, between two statements, with a zero delay; see there for what
//! cannot be split.
//!
//! [`check_function`] decides ahead of time, against typed placeholders,
//! whether every statement and call of a fragment (and of the script
//! functions it reaches, in every state they have) is one this VM runs, so
//! populate installs `Effect::Interpret` only for fragments that run whole.
//! A run that still meets something it cannot do — a native that declines
//! its actual values — stops there and logs it.
//!
//! A call or member access through a cast or a typed variable reaches the
//! script the VM runs on another reference, as that reference's.
//!
//! Structs and array methods run (`Value::Struct`, 20cbfd7b7). What is
//! genuinely not modelled: a struct or array held across `Utility.Wait` —
//! the saved continuation stores scalars only (scripting.md § continuations).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_papyrus::ast::{
    AssignOp, BinaryOp, CallArg, Expr, FunctionFlags, Identifier, Param, Script, ScriptItem, StateItem,
    Stmt, Type, UnaryOp,
};
use byroredux_plugin::esm::records::condition::ConditionStringId;
use byroredux_papyrus::span::{Span, Spanned};
use byroredux_plugin::esm::records::script_instance::{
    PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
};

use super::effects::{apply_effects, evaluate_guard, evaluate_value, DeferredFragmentEffects};
use crate::papyrus_provider::MAX_PROVIDER_WAIT_SECONDS;
use crate::quest_stages::{QuestFormId, QuestObjectiveState, QuestStageAdvanced, QuestStageState};
use crate::translate::compose::vmad_property_name;
use crate::translate::effects::{
    classify_native_effect, classify_native_guard, classify_native_value, NativeBindings, VmFrame,
    VmResume, VmStep, VmStored,
};

/// The player's reference, `PlayerRef`.
const PLAYER_REF: u32 = 0x14;
/// Fallout 4's `Caps001` (`Game.GetCaps`), form `0x0F` of `Fallout4.esm`.
const FO4_CAPS: u32 = 0x0F;
/// Skyrim's `Gold001` (`Actor.GetGoldAmount`), form `0x0F` of `Skyrim.esm`.
const SKYRIM_GOLD: u32 = 0x0F;
/// Starfield's `Credits` (`Game.GetCredits`), form `0x0F` of `Starfield.esm`.
const STARFIELD_CREDITS: u32 = 0x0F;
/// Statements one run may execute before it is stopped (a runaway loop).
const MAX_STEPS: usize = 100_000;
/// Script-function nesting.
const MAX_DEPTH: usize = 16;
/// Elements one array may hold — matches the static lowering's ceiling
/// (`papyrus_provider/lower_call.rs` declines `new T[n]` past this), so a
/// script the lowering accepts cannot be stopped here for size.
const MAX_ARRAY_ELEMENTS: usize = byroredux_sdk::script_function::MAX_SCRIPT_ARRAY_ELEMENTS;
/// Bytes in one string result — `s = s + s` doubles per statement and would
/// reach terabytes long before `MAX_STEPS` bites.
const MAX_STRING_BYTES: usize = byroredux_sdk::script_function::MAX_SCRIPT_STRING_BYTES;
/// Hops a "follow the linked-ref chain" native walks (#5844). The CK
/// documents `iMaxExpectedLinkedRefs` with default 100 — the documented
/// value is the ceiling for every chain walk (loop guard, caller-supplied
/// limits, and `GetNthLinkedRef`'s hop count), so the three spellings that
/// used to disagree (256 / 100 / 64) are one concept.
const MAX_LINK_CHAIN_HOPS: usize = 100;

/// A Papyrus value.
#[derive(Debug, Clone)]
pub enum Value {
    None,
    Bool(bool),
    Int(i32),
    Float(f32),
    Str(String),
    /// Any form by FormID — a reference, a base object, a quest; the
    /// player's reference is `0x14`.
    Form(u32),
    /// A quest alias.
    Alias { quest: u32, alias: i16 },
    Array(Rc<RefCell<Vec<Value>>>),
    /// A Fallout 4 struct: its members by name. Structs, like arrays, are
    /// shared by reference.
    Struct(Rc<RefCell<Vec<(String, Value)>>>),
}

impl Value {
    fn truthy(&self) -> bool {
        match self {
            Value::None => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Form(form) => *form != 0,
            Value::Alias { .. } | Value::Array(_) | Value::Struct(_) => true,
        }
    }

    fn number(&self) -> Option<f32> {
        match self {
            Value::Bool(b) => Some(f32::from(u8::from(*b))),
            Value::Int(i) => Some(*i as f32),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Whether both are the same alias.
    fn same_alias(&self, other: &Value) -> bool {
        matches!((self, other), (Value::Alias { quest: a, alias: x }, Value::Alias { quest: b, alias: y }) if a == b && x == y)
    }

    fn is_float(&self) -> bool {
        matches!(self, Value::Float(_))
    }

    /// An `Int` (or `Bool`) exactly — `number()` goes through an `f32`,
    /// which cannot hold every 32-bit integer.
    fn int(&self) -> Option<i32> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Bool(b) => Some(i32::from(*b)),
            _ => None,
        }
    }

    fn text(&self) -> String {
        match self {
            Value::None => "None".into(),
            Value::Bool(b) => if *b { "True" } else { "False" }.into(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => format!("{f:.6}"),
            Value::Str(s) => s.clone(),
            Value::Form(form) => format!("[Form <{form:08X}>]"),
            Value::Alias { quest, alias } => format!("[Alias <{quest:08X}:{alias}>]"),
            Value::Array(_) => "[Array]".into(),
            Value::Struct(_) => "[Struct]".into(),
        }
    }

    fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::None, Value::None) => true,
            (Value::None, _) | (_, Value::None) => false,
            (Value::Str(a), Value::Str(b)) => a.eq_ignore_ascii_case(b),
            (Value::Form(a), Value::Form(b)) => a == b,
            (Value::Alias { quest: a, alias: x }, Value::Alias { quest: b, alias: y }) => a == b && x == y,
            (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b),
            (Value::Struct(a), Value::Struct(b)) => Rc::ptr_eq(a, b),
            (a, b) => match (a.int(), b.int()) {
                (Some(a), Some(b)) => a == b,
                _ => match (a.number(), b.number()) {
                    (Some(a), Some(b)) => a == b,
                    _ => false,
                },
            },
        }
    }

    /// A struct's member `name`, case-insensitively.
    fn member(members: &[(String, Value)], name: &str) -> Option<usize> {
        members.iter().position(|(member, _)| member.eq_ignore_ascii_case(name))
    }

    fn default_for(ty: &Type) -> Value {
        match ty {
            Type::Bool => Value::Bool(false),
            Type::Int => Value::Int(0),
            Type::Float => Value::Float(0.0),
            Type::String => Value::Str(String::new()),
            _ => Value::None,
        }
    }

    fn from_property(value: &PropertyValue) -> Value {
        match value {
            PropertyValue::Object { form_id: 0, alias: -1 } => Value::None,
            PropertyValue::Object { form_id, alias: -1 } => Value::Form(*form_id),
            PropertyValue::Object { form_id, alias } => Value::Alias {
                quest: *form_id,
                alias: *alias,
            },
            PropertyValue::String(s) => Value::Str(s.clone()),
            PropertyValue::Int32(i) => Value::Int(*i),
            PropertyValue::Float(f) => Value::Float(*f),
            PropertyValue::Bool(b) => Value::Bool(*b),
            PropertyValue::Array(items) => Value::Array(Rc::new(RefCell::new(
                items.iter().map(Value::from_property).collect(),
            ))),
            PropertyValue::Struct(members) => Value::Struct(Rc::new(RefCell::new(
                members
                    .iter()
                    .map(|member| (member.name.clone(), Value::from_property(&member.value)))
                    .collect(),
            ))),
            PropertyValue::Unknown(_) => Value::None,
        }
    }
}

/// Why a run stopped.
#[derive(Debug)]
struct Stop(String);

type Run<T> = Result<T, Stop>;

fn stop<T>(why: impl Into<String>) -> Run<T> {
    Err(Stop(why.into()))
}

/// One function activation.
struct Frame {
    script: Arc<Script>,
    /// The object the script is attached to (`Self`): a quest, or a
    /// reference for a reference script. Its script variables are keyed by
    /// it.
    owner: QuestFormId,
    vmad: Option<ScriptInstanceData>,
    locals: HashMap<String, Value>,
    /// The script whose body is running — the instance's script or one it
    /// extends — which `Parent.Fn()` calls past. Set when a function starts.
    defined_in: Option<Arc<Script>>,
}

enum Flow {
    Normal,
    Return(Value),
    /// `Utility.Wait(seconds)`: the path down to it in this frame, and the
    /// suspended frames of the call it waits in (outermost first).
    Suspend(f32, Vec<VmStep>, Vec<VmFrame>),
}

fn suspend_at(step: VmStep, flow: Flow) -> Flow {
    match flow {
        Flow::Suspend(seconds, mut path, frames) => {
            path.insert(0, step);
            Flow::Suspend(seconds, path, frames)
        }
        other => other,
    }
}

/// `frame`'s state at `path` in `code` for a [`VmResume`].
fn frame_state(frame: &Frame, code: &Code, function: &str, path: Vec<VmStep>) -> Run<VmFrame> {
    let mut locals = Vec::with_capacity(frame.locals.len());
    for (name, value) in &frame.locals {
        let Some(value) = store(value) else {
            return stop("an array local across a wait");
        };
        locals.push((name.clone(), value));
    }
    Ok(VmFrame {
        script: frame.script.name.node.0.clone(),
        function: function.to_owned(),
        owner: frame.owner.0,
        defined_in: code.script.name.node.0.clone(),
        state: code.state.clone(),
        path,
        locals,
    })
}

/// Every local of `frame` can be stored in a [`VmFrame`] (no array or struct).
fn locals_storable(frame: &Frame) -> bool {
    frame
        .locals
        .values()
        .all(|value| !matches!(value, Value::Array(_) | Value::Struct(_)))
}

fn restore_locals(frame: &VmFrame) -> HashMap<String, Value> {
    frame
        .locals
        .iter()
        .map(|(name, value)| (name.clone(), restore(value)))
        .collect()
}

fn store(value: &Value) -> Option<VmStored> {
    Some(match value {
        Value::None => VmStored::None,
        Value::Bool(b) => VmStored::Bool(*b),
        Value::Int(i) => VmStored::Int(*i),
        Value::Float(f) => VmStored::Float(*f),
        Value::Str(s) => VmStored::Str(s.clone()),
        Value::Form(form) => VmStored::Form(*form),
        Value::Alias { quest, alias } => VmStored::Alias {
            quest: *quest,
            alias: *alias,
        },
        Value::Array(_) | Value::Struct(_) => return None,
    })
}

fn restore(stored: &VmStored) -> Value {
    match stored {
        VmStored::None => Value::None,
        VmStored::Bool(b) => Value::Bool(*b),
        VmStored::Int(i) => Value::Int(*i),
        VmStored::Float(f) => Value::Float(*f),
        VmStored::Str(s) => Value::Str(s.clone()),
        VmStored::Form(form) => Value::Form(*form),
        VmStored::Alias { quest, alias } => Value::Alias {
            quest: *quest,
            alias: *alias,
        },
        VmStored::Array(items) => Value::Array(Rc::new(RefCell::new(items.iter().map(restore).collect()))),
    }
}

/// `value` as an event argument: a scalar, or an array of scalars.
fn event_arg(value: &Value) -> Option<VmStored> {
    match value {
        Value::Array(items) => items
            .borrow()
            .iter()
            .map(|item| if matches!(item, Value::Array(_)) { None } else { event_arg(item) })
            .collect::<Option<Vec<_>>>()
            .map(VmStored::Array),
        other => store(other),
    }
}

/// The run's mutable context: the fragment runtime's own state.
struct Vm<'a> {
    world: &'a World,
    context: QuestFormId,
    stages: &'a mut QuestStageState,
    objectives: &'a mut QuestObjectiveState,
    deferred: &'a mut DeferredFragmentEffects,
    steps: usize,
    depth: usize,
    advances: Vec<QuestStageAdvanced>,
    /// The frame budget this run may spend (statements), the statements it
    /// has spent of it, and whether it suspended because that ran out.
    allowance: usize,
    spent: usize,
    yielded: bool,
    /// Calls for a value in progress, and calling frames holding an array or
    /// struct local: a run cannot suspend while either is on the stack.
    value_calls: usize,
    blocked_callers: usize,
    /// Whether the native call being resolved is a statement (its result is
    /// unused): a call for effect on a `None` object is ignored, as the engine does.
    for_effect: bool,
    /// The arrays and structs script variables hold that this run has read
    /// or written, by (owner, variable). They are reference values: every
    /// read in the run shares one, so a mutation is seen by the next read, and
    /// the run writes them back whole when it ends.
    aggregates: std::cell::RefCell<HashMap<(u32, ConditionStringId), Value>>,
    /// The members of the script-filled reference collections this run has
    /// read or changed, written back when it ends.
    collections: std::cell::RefCell<HashMap<crate::alias_collections::CollectionKey, Vec<u32>>>,
}

/// Run `script`'s `function` attached to `context` — a quest's fragment, or
/// a reference script's event with its `args` — its properties bound by
/// `vmad`, from the start or from where `resume` says a `Utility.Wait`
/// suspended it. Returns the quest-stage advances its natives produced, and
/// the wait and resume point when it suspends again.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_fragment(
    script: &str,
    function: &str,
    args: &[VmStored],
    resume: Option<&VmResume>,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
    deferred: &mut DeferredFragmentEffects,
) -> (Vec<QuestStageAdvanced>, Option<(f32, VmResume)>) {
    let Some(script) = deferred.vm_scripts.get(&script.to_ascii_lowercase()).cloned() else {
        log::warn!("Papyrus VM: script '{script}' is not installed");
        return (Vec::new(), None);
    };
    let mut vm = Vm {
        world,
        context,
        stages,
        objectives,
        deferred,
        // A run the frame budget suspended counts its earlier statements
        // against the cap; one a wait suspended starts over.
        steps: resume.map_or(0, |state| state.carried_steps as usize),
        depth: 0,
        advances: Vec::new(),
        allowance: world
            .try_resource::<super::VmFrameBudget>()
            .map_or(usize::MAX, |budget| budget.remaining()),
        spent: super::RUN_START_COST,
        yielded: false,
        value_calls: 0,
        blocked_callers: 0,
        for_effect: false,
        aggregates: Default::default(),
        collections: Default::default(),
    };
    let frame = Frame {
        script: Arc::clone(&script),
        owner: context,
        vmad: vmad.cloned(),
        locals: HashMap::new(),
        defined_in: None,
    };
    let run = match resume.and_then(|state| state.frames.split_first()) {
        Some((top, inner)) => vm.resume_frame(frame, top, inner),
        None => match vm.resolve(&script, context, function) {
            // An event the script does not handle in its current state:
            // nothing runs.
            None if super::reference::is_delivered_event(function) => Ok(Flow::Normal),
            None => stop(format!("no function {function}")),
            Some(code) => {
                let mut args: Vec<Value> = args.iter().map(restore).collect();
                // A topic-info fragment's speaker parameter is the line's
                // speaker, as the lowering binds it.
                let speaker_parameter = code.callable(function).is_ok_and(|callable| {
                    callable.params.first().is_some_and(|param| {
                        param
                            .name
                            .node
                            .0
                            .eq_ignore_ascii_case(crate::translate::effects::INFO_SPEAKER_PARAMETER)
                    })
                });
                if args.is_empty() && speaker_parameter {
                    let speaker = vm.deferred.speaker.and_then(|entity| vm.entity_form(entity));
                    args.push(speaker.map_or(Value::None, Value::Form));
                }
                vm.enter(frame, &code, function, args).and_then(|mut frame| {
                    let flow = vm.run_frame(&mut frame, &code, function, None, &[])?;
                    vm.callee_flow(flow, &frame, &code, function)
                })
            }
        },
    }
    .map(|flow| match flow {
        Flow::Suspend(seconds, _, frames) => {
            let carried_steps = if vm.yielded { vm.steps as u32 } else { 0 };
            Some((seconds, VmResume { frames, carried_steps }))
        }
        _ => None,
    });
    let suspended = match run {
        Ok(suspended) => suspended,
        Err(Stop(why)) => {
            log::warn!(
                "Papyrus VM: {}.{function} on {:08X} stopped: {why}",
                script.name.node.0,
                context.0
            );
            None
        }
    };
    // The arrays and structs the run touched go back to the saved store.
    vm.flush_aggregates();
    for (key, members) in std::mem::take(&mut *vm.collections.borrow_mut()) {
        vm.deferred.alias_collections.push((key, members));
    }
    // Settle with the frame's budget: what this run spent, and whether it was
    // cut short by it.
    if let Some(mut budget) = world.try_resource_mut::<super::VmFrameBudget>() {
        budget.charge(vm.spent);
        if vm.yielded {
            budget.note_yielded();
        }
    }
    (vm.advances, suspended)
}

/// What a call runs: a function's or an event's parameters and body.
struct Callable<'s> {
    params: &'s [Param],
    body: &'s [Spanned<Stmt>],
    /// A function's declared return type; an event returns nothing.
    returns: Option<&'s Spanned<Type>>,
}

/// `name` defined in `script` — in the state block `state`, or in the empty
/// state for `None` — as a function or an event. A `Native` function is not
/// one: its declaration (in `Quest`, `ObjectReference`, … which the script
/// library also serves) has no body, and the native bridge runs it.
fn callable_in<'s>(script: &'s Script, state: Option<&str>, name: &str) -> Option<Callable<'s>> {
    let named = |n: &Identifier| n.0.eq_ignore_ascii_case(name);
    let scripted = |f: &byroredux_papyrus::ast::Function| !f.flags.contains(FunctionFlags::NATIVE);
    match state {
        None => script.body.iter().find_map(|item| match &item.node {
            ScriptItem::Function(f) if named(&f.name.node) && scripted(f) => Some(Callable {
                params: &f.params,
                body: &f.body,
                returns: f.return_type.as_ref(),
            }),
            ScriptItem::Event(e) if named(&e.name.node) => Some(Callable {
                params: &e.params,
                body: &e.body,
                returns: None,
            }),
            _ => None,
        }),
        Some(state) => script.body.iter().find_map(|item| match &item.node {
            ScriptItem::State(block) if block.name.node.0.eq_ignore_ascii_case(state) => {
                block.body.iter().find_map(|item| match &item.node {
                    StateItem::Function(f) if named(&f.name.node) && scripted(f) => Some(Callable {
                        params: &f.params,
                        body: &f.body,
                        returns: f.return_type.as_ref(),
                    }),
                    StateItem::Event(e) if named(&e.name.node) => Some(Callable {
                        params: &e.params,
                        body: &e.body,
                        returns: None,
                    }),
                    _ => None,
                })
            }
            _ => None,
        }),
    }
}

/// Where a running body is: the script that defines it and its state block
/// (`None`: the empty state).
#[derive(Clone)]
struct Code {
    script: Arc<Script>,
    state: Option<String>,
}

impl Code {
    fn callable(&self, name: &str) -> Run<Callable<'_>> {
        match callable_in(&self.script, self.state.as_deref(), name) {
            Some(callable) => Ok(callable),
            None => stop(format!("no function {name}")),
        }
    }
}

/// `script` and the scripts it extends that `scripts` carries, most derived
/// first.
fn script_chain(script: &Arc<Script>, scripts: &HashMap<String, Arc<Script>>) -> Vec<Arc<Script>> {
    let mut chain = vec![Arc::clone(script)];
    while chain.len() < 16 {
        let Some(parent) = chain.last().and_then(|last| last.parent.as_ref()) else {
            break;
        };
        match scripts.get(&parent.node.0.to_ascii_lowercase()) {
            Some(parent) => chain.push(Arc::clone(parent)),
            None => break,
        }
    }
    chain
}

/// An engine class — `Form`, `ObjectReference`, `Quest`, … — declares
/// `Native` functions; no game script does. Its API, scripted wrappers
/// included (`Quest.SetStage` → `SetCurrentStageID`), is the native
/// bridge's, so calls never resolve into it.
fn is_engine_class(script: &Script) -> bool {
    let native = |f: &byroredux_papyrus::ast::Function| f.flags.contains(FunctionFlags::NATIVE);
    script.body.iter().any(|item| match &item.node {
        ScriptItem::Function(f) => native(f),
        ScriptItem::State(block) => block.body.iter().any(|item| matches!(&item.node, StateItem::Function(f) if native(f))),
        _ => false,
    })
}

/// The body a call of `name` runs on an instance of `script` in `state`.
/// CK *States (Papyrus)*: the current state down the `extends` chain, then
/// the empty state down the chain — stopping at the engine classes.
fn resolve_in(script: &Arc<Script>, state: &str, name: &str, scripts: &HashMap<String, Arc<Script>>) -> Option<Code> {
    let chain: Vec<_> = script_chain(script, scripts)
        .into_iter()
        .take_while(|script| !is_engine_class(script))
        .collect();
    let in_state = (!state.is_empty())
        .then(|| {
            chain.iter().find(|s| callable_in(s, Some(state), name).is_some()).map(|s| Code {
                script: Arc::clone(s),
                state: Some(state.to_owned()),
            })
        })
        .flatten();
    in_state.or_else(|| {
        chain.iter().find(|s| callable_in(s, None, name).is_some()).map(|s| Code {
            script: Arc::clone(s),
            state: None,
        })
    })
}

/// The state an instance of `script` starts in: its `Auto State`, or one a
/// script it extends declares; else the empty state.
fn auto_state(script: &Arc<Script>, scripts: &HashMap<String, Arc<Script>>) -> String {
    script_chain(script, scripts)
        .iter()
        .find_map(|s| {
            s.body.iter().find_map(|item| match &item.node {
                ScriptItem::State(block) if block.is_auto => Some(block.name.node.0.clone()),
                _ => None,
            })
        })
        .unwrap_or_default()
}

/// The names of `script`'s state blocks.
fn state_names(script: &Script) -> impl Iterator<Item = &str> {
    script.body.iter().filter_map(|item| match &item.node {
        ScriptItem::State(block) => Some(block.name.node.0.as_str()),
        _ => None,
    })
}

/// The decompiled `GotoState` / `GetState` keep a script's state in the
/// `::State` variable.
fn is_state_variable(name: &str) -> bool {
    name.eq_ignore_ascii_case("::state")
}

/// Where `script`'s state is kept among its owner's script variables — per
/// script, since one reference may carry several.
fn state_key(script: &Script) -> ConditionStringId {
    ConditionStringId::from_text(&format!("::state::{}", script.name.node.0.to_ascii_lowercase()))
}

/// The value a script variable or property declares (`Int n = 5`,
/// `Bool Property P = True Auto`), or an `AutoReadOnly` property's constant,
/// when it is a literal.
fn declared_default(script: &Script, name: &str) -> Option<Value> {
    let bare = vmad_property_name(name);
    let named = |n: &Identifier| n.0.eq_ignore_ascii_case(name) || n.0.eq_ignore_ascii_case(bare);
    script.body.iter().find_map(|item| {
        let initial = match &item.node {
            ScriptItem::Variable(v) if named(&v.name.node) => v.initial_value.as_ref(),
            ScriptItem::Property(p) if named(&p.name.node) => match &p.initial_value {
                Some(initial) => Some(initial),
                None => return getter_constant(p),
            },
            _ => return None,
        };
        literal_value(&initial?.node)
    })
}

fn literal_value(e: &Expr) -> Option<Value> {
    Some(match literal_of(e)? {
        Expr::IntLit(i) => Value::Int(i as i32),
        Expr::FloatLit(f) => Value::Float(f as f32),
        Expr::BoolLit(b) => Value::Bool(b),
        Expr::StringLit(text) => Value::Str(text),
        _ => Value::None,
    })
}

/// An `AutoReadOnly` property's constant: the compiler gives it a getter
/// that returns a literal (`ObjectReference.Motion_Keyframed` → 2).
fn getter_constant(property: &byroredux_papyrus::ast::Property) -> Option<Value> {
    let getter = property.getter.as_ref()?;
    match getter.body.as_slice() {
        [statement] => match &statement.node {
            Stmt::Return(Some(value)) => literal_value(&value.node),
            _ => None,
        },
        _ => None,
    }
}

/// `value` as a stored object (see [`crate::ScriptObject`]); `None` for what
/// a script variable cannot hold (a nested array).
fn to_stored(value: &Value) -> Option<crate::ScriptObject> {
    Some(match value {
        Value::None => crate::ScriptObject::None,
        Value::Bool(b) => crate::ScriptObject::Bool(*b),
        Value::Int(i) => crate::ScriptObject::Int(*i),
        Value::Float(f) => crate::ScriptObject::Float(*f),
        Value::Str(text) => crate::ScriptObject::Str(text.clone()),
        Value::Form(form) => crate::ScriptObject::Form(*form),
        Value::Alias { quest, alias } => crate::ScriptObject::Alias {
            quest: *quest,
            alias: *alias,
        },
        Value::Array(items) => {
            let mut stored = Vec::new();
            for item in items.borrow().iter() {
                if matches!(item, Value::Array(_)) {
                    return None;
                }
                stored.push(to_stored(item)?);
            }
            crate::ScriptObject::Array(stored)
        }
        Value::Struct(members) => {
            let mut stored = Vec::new();
            for (name, member) in members.borrow().iter() {
                if matches!(member, Value::Array(_)) {
                    return None;
                }
                stored.push((name.clone(), to_stored(member)?));
            }
            crate::ScriptObject::Struct(stored)
        }
    })
}

/// A stored object as a value; arrays and structs come back as fresh shared
/// values.
fn from_stored(object: &crate::ScriptObject) -> Value {
    match object {
        crate::ScriptObject::None => Value::None,
        crate::ScriptObject::Form(form) => Value::Form(*form),
        crate::ScriptObject::Alias { quest, alias } => Value::Alias {
            quest: *quest,
            alias: *alias,
        },
        crate::ScriptObject::Str(text) => Value::Str(text.clone()),
        crate::ScriptObject::Int(i) => Value::Int(*i),
        crate::ScriptObject::Float(f) => Value::Float(*f),
        crate::ScriptObject::Bool(b) => Value::Bool(*b),
        crate::ScriptObject::Array(items) => {
            Value::Array(Rc::new(RefCell::new(items.iter().map(from_stored).collect())))
        }
        crate::ScriptObject::Struct(members) => Value::Struct(Rc::new(RefCell::new(
            members.iter().map(|(name, member)| (name.clone(), from_stored(member))).collect(),
        ))),
    }
}

/// The `RefCollectionAlias` natives the VM runs.
/// An `InputEnableLayer` member function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerNative {
    Disable,
    Enable,
    Reset,
    Delete,
    /// `Enable<Control>(abEnable = true)`.
    Set(crate::Control),
    /// `Is<Control>Enabled()`.
    Query(crate::Control),
}

/// The layer function a lowercase name is, if any.
fn layer_native(name: &str) -> Option<LayerNative> {
    match name {
        "disableplayercontrols" => return Some(LayerNative::Disable),
        "enableplayercontrols" => return Some(LayerNative::Enable),
        "reset" => return Some(LayerNative::Reset),
        "delete" => return Some(LayerNative::Delete),
        _ => {}
    }
    if let Some(control) = name.strip_prefix("enable") {
        return crate::Control::of_name(control).map(LayerNative::Set);
    }
    let control = name.strip_prefix("is")?.strip_suffix("enabled")?;
    crate::Control::of_name(control).map(LayerNative::Query)
}

/// The most references one `PlaceAtMe` call creates.
const MAX_PLACED_PER_CALL: i32 = 32;

const COLLECTION_NATIVES: [&str; 13] = [
    "evaluateall",
    "getcount",
    "getat",
    "getarray",
    "getactorarray",
    "enableall",
    "disableall",
    "find",
    "addref",
    "addarray",
    "addrefcollection",
    "removeref",
    "removeall",
];

/// The `Math` natives the VM computes itself.
const MATH_NATIVES: [&str; 8] = ["min", "max", "abs", "floor", "ceiling", "sqrt", "pow", "sin"];

/// `Math.<name>(values)`, by lowercase name (CK *Math Script* pages; `Min` /
/// `Max` are Fallout 4's). `None` arity or kind the VM cannot compute.
fn math_native(name: &str, values: &[Value]) -> Run<Option<Value>> {
    let mut numbers = Vec::with_capacity(values.len());
    for value in values {
        let Some(number) = value.number() else {
            return stop(format!("Math.{name} on {}", value.text()));
        };
        numbers.push(number);
    }
    let arity = if matches!(name, "min" | "max" | "pow") { 2 } else { 1 };
    if numbers.len() != arity {
        return stop(format!("Math.{name} takes {arity} argument(s)"));
    }
    let a = numbers[0];
    Ok(Some(match name {
        "min" => Value::Float(a.min(numbers[1])),
        "max" => Value::Float(a.max(numbers[1])),
        "abs" => Value::Float(a.abs()),
        "floor" => Value::Int(a.floor() as i32),
        "ceiling" => Value::Int(a.ceil() as i32),
        "sqrt" => Value::Float(a.sqrt()),
        "pow" => Value::Float(a.powf(numbers[1])),
        // Papyrus trigonometry takes degrees.
        "sin" => Value::Float(a.to_radians().sin()),
        _ => return Ok(None),
    }))
}

/// The array methods the VM runs (CK *Arrays (Papyrus)*).
const ARRAY_METHODS: [&str; 9] = [
    "find",
    "rfind",
    "findstruct",
    "rfindstruct",
    "add",
    "insert",
    "remove",
    "removelast",
    "clear",
];

/// The declaration of the struct type `name`: `Script#Struct` names its
/// script; a bare name is a struct of `script`'s chain.
fn struct_declaration(
    script: &Arc<Script>,
    name: &str,
    scripts: &HashMap<String, Arc<Script>>,
) -> Option<byroredux_papyrus::ast::Struct> {
    let (owners, struct_name) = match name.split_once('#') {
        Some((owner, struct_name)) => (vec![scripts.get(&owner.to_ascii_lowercase())?.clone()], struct_name),
        None => (script_chain(script, scripts), name),
    };
    owners.iter().find_map(|owner| {
        owner.body.iter().find_map(|item| match &item.node {
            ScriptItem::Struct(declaration) if declaration.name.node.0.eq_ignore_ascii_case(struct_name) => {
                Some(declaration.clone())
            }
            _ => None,
        })
    })
}

/// The engine classes whose `AutoReadOnly` constants a script reads on any
/// reference (`akRef.Motion_Keyframed`).
const CONSTANT_CLASSES: [&str; 3] = ["objectreference", "actor", "form"];

/// A constant property's value: an `AutoReadOnly` one's literal, as source
/// declares it or as the compiled getter returns it.
fn property_constant(property: &byroredux_papyrus::ast::Property) -> Option<Value> {
    let read_only = property
        .flags
        .contains(byroredux_papyrus::ast::PropertyFlags::AUTO_READ_ONLY);
    match &property.initial_value {
        Some(initial) if read_only => literal_value(&initial.node),
        _ => getter_constant(property),
    }
}

/// `name` as a constant property of `script` (not a parent).
fn constant_property<'s>(script: &'s Script, name: &str) -> Option<&'s byroredux_papyrus::ast::Property> {
    script.body.iter().find_map(|item| match &item.node {
        ScriptItem::Property(p) if p.name.node.0.eq_ignore_ascii_case(name) && property_constant(p).is_some() => Some(&**p),
        _ => None,
    })
}

/// The declared type of `name` in `script` or a parent script `scripts`
/// carries.
fn member_type_in(script: &Script, name: &str, scripts: &HashMap<String, Arc<Script>>) -> Option<Type> {
    let mut current: Option<&Script> = Some(script);
    for _ in 0..16 {
        let here = current?;
        if let Some(ty) = script_member_type(here, name) {
            return Some(ty);
        }
        current = here
            .parent
            .as_ref()
            .and_then(|parent| scripts.get(&parent.node.0.to_ascii_lowercase()))
            .map(|parent| &**parent);
    }
    None
}

/// Whether `receiver` names a property or variable of `script` declared a
/// `MusicType`: the one receiver `Add()` / `Remove()` is a music-stack call on
/// (a script's own `Add()` function is a different call).
fn declares_music_type(script: &Script, receiver: &Expr) -> bool {
    let Expr::Ident(name) = receiver else {
        return false;
    };
    matches!(
        script_member_type(script, &name.0),
        Some(Type::Object(ty)) if ty.0.eq_ignore_ascii_case("musictype")
    )
}

/// The script's declared type for `name` (a property or a script variable).
fn script_member_type(script: &Script, name: &str) -> Option<Type> {
    let bare = vmad_property_name(name);
    script.body.iter().find_map(|item| match &item.node {
        ScriptItem::Property(p)
            if p.name.node.0.eq_ignore_ascii_case(bare) || p.name.node.0.eq_ignore_ascii_case(name) =>
        {
            Some(p.ty.node.clone())
        }
        ScriptItem::Variable(v)
            if v.name.node.0.eq_ignore_ascii_case(name) || v.name.node.0.eq_ignore_ascii_case(bare) =>
        {
            Some(v.ty.node.clone())
        }
        _ => None,
    })
}

/// `Game` / `Utility` / `Debug` / `Math` — the global-function classes.
fn static_class(e: &Expr) -> Option<&str> {
    let Expr::Ident(name) = e else {
        return None;
    };
    ["game", "utility", "debug", "math", "weather"]
        .into_iter()
        .find(|class| name.0.eq_ignore_ascii_case(class))
}

fn spanned<T>(node: T) -> Spanned<T> {
    Spanned::new(node, Span::new(0, 0))
}

fn ident(name: &str) -> Spanned<Expr> {
    spanned(Expr::Ident(Identifier(name.to_owned())))
}

/// Builds one synthetic native call: evaluated values bound to names.
#[derive(Default)]
struct Synth {
    bindings: NativeBindings,
    properties: Vec<ScriptProperty>,
}

impl Synth {
    fn bind(&mut self, vm: &Vm<'_>, value: &Value) -> Run<Spanned<Expr>> {
        let name = format!("__vm{}", self.properties.len() + self.bindings.players.len() + 1);
        let object = |form_id: u32, alias: i16| PropertyValue::Object { form_id, alias };
        Ok(match value {
            Value::None => spanned(Expr::NoneLit),
            Value::Bool(b) => spanned(Expr::BoolLit(*b)),
            Value::Int(i) => spanned(Expr::IntLit(i64::from(*i))),
            // An integral number binds as an Int literal: the primitives read
            // Int parameters with `int_arg`, Float ones with `as_num`, which
            // takes either.
            Value::Float(f) if f.fract() == 0.0 && f.abs() < 2_147_483_648.0 => {
                spanned(Expr::IntLit(*f as i64))
            }
            Value::Float(f) => spanned(Expr::FloatLit(f64::from(*f))),
            Value::Str(s) => spanned(Expr::StringLit(s.clone())),
            Value::Form(PLAYER_REF) => {
                self.bindings.players.insert(name.clone());
                ident(&name)
            }
            Value::Form(form) => {
                if vm.is_quest(*form) {
                    self.bindings.quests.insert(name.clone());
                }
                self.properties.push(ScriptProperty {
                    name: name.clone(),
                    status: 1,
                    value: object(*form, -1),
                });
                ident(&name)
            }
            // The effect runtime resolves an alias property against the
            // fragment's own quest.
            Value::Alias { quest, alias } if *quest == vm.context.0 => {
                self.properties.push(ScriptProperty {
                    name: name.clone(),
                    status: 1,
                    value: object(*quest, *alias),
                });
                ident(&name)
            }
            Value::Alias { .. } => return stop("another quest's alias as a native argument"),
            Value::Array(_) => return stop("an array as a native argument"),
            Value::Struct(_) => return stop("a struct as a native argument"),
        })
    }

    fn vmad(self) -> (NativeBindings, ScriptInstanceData) {
        (
            self.bindings,
            ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance {
                    name: "<Papyrus VM>".into(),
                    status: 0,
                    properties: self.properties,
                }],
            },
        )
    }
}

impl Vm<'_> {
    fn is_quest_form(&self, value: &Value) -> bool {
        matches!(value, Value::Form(form) if self.is_quest(*form))
    }

    fn is_quest(&self, form: u32) -> bool {
        let quest = QuestFormId(form);
        self.deferred.quest_scripts.contains_key(&quest) || self.stages.is_started(quest)
    }

    fn tick(&mut self) -> Run<()> {
        self.steps += 1;
        self.spent += 1;
        if self.steps > MAX_STEPS {
            return stop("step limit");
        }
        Ok(())
    }

    /// The run has used its frame allowance and may stop here, between two
    /// statements: nothing on the stack needs a value back or holds a local
    /// the saved continuation cannot store.
    fn should_yield(&self, frame: &Frame) -> bool {
        self.spent >= self.allowance
            && self.value_calls == 0
            && self.blocked_callers == 0
            && locals_storable(frame)
    }

    /// The body a call of `name` on `owner`'s instance of `script` runs, in
    /// the instance's current state.
    fn resolve(&self, script: &Arc<Script>, owner: QuestFormId, name: &str) -> Option<Code> {
        let state = self.script_state(owner, script);
        resolve_in(script, &state, name, &self.deferred.vm_scripts)
    }

    /// `<quest>.RegisterForUpdate(…)` and the like on another quest: every
    /// script attached to it that handles the update registers (or all of
    /// them unregister).
    fn quest_update(&mut self, frame: &mut Frame, quest: u32, name: &str, args: &[CallArg]) -> Run<()> {
        if super::EventRequest::is_call(name) {
            return self.event_registration(frame, Some(quest), name, args);
        }
        if super::DistanceRequest::handler(name).is_some() {
            return self.distance_registration(frame, Some(quest), name, args);
        }
        let mut numbers = Vec::with_capacity(args.len());
        for arg in args {
            numbers.push(self.eval(frame, &arg.value.node)?.number());
        }
        let Some(request) = super::UpdateRequest::of_call(name, &numbers) else {
            return stop(format!("{name} without its numbers"));
        };
        let handler = super::UpdateRequest::handler(name).unwrap_or_default();
        let owner = QuestFormId(quest);
        let attached: Vec<String> = self
            .deferred
            .quest_scripts
            .get(&owner)
            .map(|vmad| vmad.scripts.iter().map(|script| script.name.clone()).collect())
            .unwrap_or_default();
        for name in attached {
            let script = self.deferred.vm_scripts.get(&name.to_ascii_lowercase()).cloned();
            let handles = script.is_some_and(|script| handler.is_empty() || self.resolve(&script, owner, handler).is_some());
            if handles {
                self.deferred.script_updates.push((quest, name, request));
            }
        }
        Ok(())
    }

    /// `RegisterForCustomEvent` / `RegisterForRemoteEvent` and the unregister
    /// calls, on the script itself (`quest` `None`) or on another quest, whose
    /// attached scripts all register.
    fn event_registration(&mut self, frame: &mut Frame, quest: Option<u32>, name: &str, args: &[CallArg]) -> Run<()> {
        let values = self.eval_args(frame, args)?;
        let sender = match values.first() {
            Some(Value::Form(form)) if *form != 0 => Some(super::DistanceTarget::Reference(*form)),
            Some(Value::Alias { quest, alias }) => Some(super::DistanceTarget::Alias {
                quest: *quest,
                alias: *alias,
            }),
            _ => None,
        };
        let event = match values.get(1) {
            Some(Value::Str(event)) => Some(event.clone()),
            _ => None,
        };
        if !name.starts_with("unregisterforall") && (sender.is_none() || event.is_none()) {
            // CK: the sender must be a script object; a None registers nothing.
            return Ok(());
        }
        let Some(request) = super::EventRequest::of_call(name, sender, event) else {
            return stop(format!("{name} as written"));
        };
        let scripts: Vec<(u32, String)> = match quest {
            None => vec![(frame.owner.0, frame.script.name.node.0.clone())],
            Some(quest) => self
                .deferred
                .quest_scripts
                .get(&QuestFormId(quest))
                .map(|vmad| vmad.scripts.iter().map(|script| (quest, script.name.clone())).collect())
                .unwrap_or_default(),
        };
        for (owner, script) in scripts {
            self.deferred.event_requests.push((owner, script, request.clone()));
        }
        Ok(())
    }

    /// `SendCustomEvent(name, args)` on the script itself: every registered
    /// receiver's handler is queued to run on its own, after this run.
    fn send_custom_event(&mut self, frame: &mut Frame, args: &[CallArg]) -> Run<()> {
        let values = self.eval_args(frame, args)?;
        let Some(Value::Str(event)) = values.first() else {
            return stop("SendCustomEvent without an event name");
        };
        let payload = match values.get(1) {
            None | Some(Value::None) => VmStored::None,
            Some(array @ Value::Array(_)) => match event_arg(array) {
                Some(stored) => stored,
                None => return stop("a custom event argument that cannot be sent"),
            },
            Some(other) => return stop(format!("custom event arguments are an array, not {}", other.text())),
        };
        let stored = self
            .world
            .try_resource::<super::EventRegistrations>()
            .map(|registrations| registrations.clone())
            .unwrap_or_default();
        let registrants = super::events::registrants(&stored, &self.deferred.event_requests, frame.owner.0, event, false);
        super::events::enqueue(self.world, &registrants, frame.owner.0, &[payload]);
        Ok(())
    }

    /// `Register…DistanceEvent` / `UnregisterForDistanceEvents` on the script
    /// itself (`quest` `None`) or on another quest, whose attached scripts that
    /// handle the event all register.
    fn distance_registration(&mut self, frame: &mut Frame, quest: Option<u32>, name: &str, args: &[CallArg]) -> Run<()> {
        let values = self.eval_args(frame, args)?;
        let target = |value: &Value| match value {
            Value::Form(form) if *form != 0 => Some(super::DistanceTarget::Reference(*form)),
            Value::Alias { quest, alias } => Some(super::DistanceTarget::Alias {
                quest: *quest,
                alias: *alias,
            }),
            _ => None,
        };
        let (Some(a), Some(b)) = (values.first().and_then(target), values.get(1).and_then(target)) else {
            // CK: the objects "must be an ObjectReference or ReferenceAlias";
            // a None registers nothing.
            return Ok(());
        };
        let Some(request) = super::DistanceRequest::of_call(name, a, b, values.get(2).and_then(Value::number)) else {
            return stop(format!("{name} without a distance"));
        };
        let handler = super::DistanceRequest::handler(name).unwrap_or_default();
        let (owner, scripts): (u32, Vec<String>) = match quest {
            None => (frame.owner.0, vec![frame.script.name.node.0.clone()]),
            Some(quest) => {
                let owner = QuestFormId(quest);
                let attached: Vec<String> = self
                    .deferred
                    .quest_scripts
                    .get(&owner)
                    .map(|vmad| vmad.scripts.iter().map(|script| script.name.clone()).collect())
                    .unwrap_or_default();
                let scripts = attached
                    .into_iter()
                    .filter(|name| {
                        let script = self.deferred.vm_scripts.get(&name.to_ascii_lowercase()).cloned();
                        script.is_some_and(|script| handler.is_empty() || self.resolve(&script, owner, handler).is_some())
                    })
                    .collect();
                (quest, scripts)
            }
        };
        for script in scripts {
            self.deferred.distance_requests.push((owner, script, request));
        }
        Ok(())
    }

    /// `RegisterForHitEvent` / `UnregisterForHitEvent` /
    /// `UnregisterForAllHitEvents` on the script itself (see [`super::hits`]).
    fn hit_registration(&mut self, frame: &mut Frame, name: &str, args: &[CallArg]) -> Run<()> {
        let values = self.eval_args(frame, args)?;
        let target = |value: &Value| match value {
            Value::Form(form) if *form != 0 => Some(super::DistanceTarget::Reference(*form)),
            Value::Alias { quest, alias } => Some(super::DistanceTarget::Alias {
                quest: *quest,
                alias: *alias,
            }),
            _ => None,
        };
        let request = match name {
            "unregisterforallhitevents" => super::HitRequest::UnregisterAll,
            "unregisterforhitevent" => match values.first().and_then(target) {
                Some(target) => super::HitRequest::Unregister { target },
                None => return Ok(()),
            },
            _ => {
                // CK: the target is "the ObjectReference, ReferenceAlias or
                // RefCollectionAlias that is being hit"; a None registers nothing.
                let Some(target) = values.first().and_then(target) else {
                    return Ok(());
                };
                let aggressor = match values.get(1) {
                    None | Some(Value::None) => None,
                    Some(value @ Value::Form(form)) if *form != 0 => {
                        // A faction filter matches the aggressor's membership;
                        // any other form the check let through is a reference.
                        if self.declared_object_type(frame, args.get(1)).is_some_and(|ty| ty.eq_ignore_ascii_case("faction")) {
                            Some(super::HitAggressor::Faction(*form))
                        } else {
                            target_of_aggressor(value).map(super::HitAggressor::Target)
                        }
                    }
                    Some(value) => match target_of_aggressor(value) {
                        Some(aggressor) => Some(super::HitAggressor::Target(aggressor)),
                        None => return stop("a hit event aggressor filter beyond None, a reference, an alias and a faction"),
                    },
                };
                if !matches!(values.get(2), None | Some(Value::None)) || !matches!(values.get(3), None | Some(Value::None)) {
                    return stop("a hit event source or projectile filter");
                }
                let tri = |index: usize| values.get(index).and_then(Value::int).unwrap_or(-1);
                super::HitRequest::Register {
                    target,
                    filter: super::HitFilter {
                        aggressor,
                        power: tri(4),
                        sneak: tri(5),
                        bash: tri(6),
                        block: tri(7),
                        invert: values.get(8).is_some_and(|value| !value.truthy()),
                    },
                }
            }
        };
        let script = frame.script.name.node.0.clone();
        self.deferred.hit_requests.push((frame.owner.0, script, request));
        Ok(())
    }

    /// The declared object type of a call argument written as a property or a
    /// cast of one (`OnHitFaction`, `Alias_Boss as ScriptObject`).
    fn declared_object_type(&self, frame: &Frame, arg: Option<&CallArg>) -> Option<String> {
        let mut expr = &arg?.value.node;
        while let Expr::Cast { expr: inner, .. } = expr {
            expr = &inner.node;
        }
        let Expr::Ident(name) = expr else {
            return None;
        };
        match member_type_in(&frame.script, &name.0, &self.deferred.vm_scripts)? {
            Type::Object(ty) => Some(ty.0),
            _ => None,
        }
    }

    /// `GotoState(state)` for a script without the compiler's generated
    /// one: `OnEndState` in the old state, the switch, `OnBeginState` in the
    /// new one (CK *OnBeginState*: also when re-entering the same state).
    fn goto_state(&mut self, frame: &Frame, state: String) -> Run<()> {
        for (event, switch) in [("OnEndState", false), ("OnBeginState", true)] {
            if switch {
                let key = state_key(&frame.script);
                self.write_object(frame.owner, key, &Value::Str(state.clone()))?;
            }
            if let Some(code) = self.resolve(&frame.script, frame.owner, event) {
                let handler = Frame {
                    script: Arc::clone(&frame.script),
                    owner: frame.owner,
                    vmad: frame.vmad.clone(),
                    locals: HashMap::new(),
                    defined_in: None,
                };
                self.call_function(handler, &code, event, Vec::new())?;
            }
        }
        Ok(())
    }

    /// `owner`'s instance of `script`'s state: the one `GotoState` last set,
    /// else the auto state.
    fn script_state(&self, owner: QuestFormId, script: &Arc<Script>) -> String {
        match self.written_object(owner, state_key(script)) {
            Some(Value::Str(state)) => state,
            _ => auto_state(script, &self.deferred.vm_scripts),
        }
    }

    /// Run `function`'s body `code` in `frame` — from the start, or along
    /// `path`, with `inner` the suspended frames of the call the path ends in.
    fn run_frame(
        &mut self,
        frame: &mut Frame,
        code: &Code,
        function: &str,
        path: Option<&[VmStep]>,
        inner: &[VmFrame],
    ) -> Run<Flow> {
        if self.depth >= MAX_DEPTH {
            return stop("call depth");
        }
        let callable = code.callable(function)?;
        frame.defined_in = Some(Arc::clone(&code.script));
        self.depth += 1;
        let flow = self.exec_block(frame, callable.body, path, inner, true);
        self.depth -= 1;
        flow
    }

    /// `frame` with the parameters of `name` (in `code`) bound to `args`.
    fn enter(&mut self, mut frame: Frame, code: &Code, name: &str, args: Vec<Value>) -> Run<Frame> {
        let callable = code.callable(name)?;
        let mut args = args.into_iter();
        for param in callable.params {
            let value = match args.next() {
                Some(value) => value,
                None => match &param.default {
                    Some(default) => self.eval(&mut frame, &default.node)?,
                    None => Value::default_for(&param.ty.node),
                },
            };
            frame
                .locals
                .insert(param.name.node.0.to_ascii_lowercase(), value);
        }
        Ok(frame)
    }

    /// A function called for its value; it may not wait.
    fn call_function(&mut self, frame: Frame, code: &Code, name: &str, args: Vec<Value>) -> Run<Value> {
        let mut frame = self.enter(frame, code, name, args)?;
        self.value_calls += 1;
        let flow = self.run_frame(&mut frame, code, name, None, &[]);
        self.value_calls -= 1;
        Ok(match flow? {
            Flow::Return(value) => value,
            Flow::Normal => Value::None,
            Flow::Suspend(..) => return stop("a wait in a function called for its value"),
        })
    }

    /// A called function's flow as its call statement's: a suspension adds
    /// the callee's frame to the chain.
    fn callee_flow(&mut self, flow: Flow, frame: &Frame, code: &Code, function: &str) -> Run<Flow> {
        Ok(match flow {
            Flow::Suspend(seconds, path, deeper) => {
                let mut frames = vec![frame_state(frame, code, function, path)?];
                frames.extend(deeper);
                Flow::Suspend(seconds, Vec::new(), frames)
            }
            _ => Flow::Normal,
        })
    }

    /// Continue the suspended frame `state` as `frame` (its script, owner
    /// and properties), with `inner` the frames of the call it waits in.
    fn resume_frame(&mut self, mut frame: Frame, state: &VmFrame, inner: &[VmFrame]) -> Run<Flow> {
        let Some(defined_in) = self
            .deferred
            .vm_scripts
            .get(&state.defined_in.to_ascii_lowercase())
            .cloned()
        else {
            return stop(format!("script {} is not installed", state.defined_in));
        };
        let code = Code {
            script: defined_in,
            state: state.state.clone(),
        };
        frame.locals = restore_locals(state);
        let flow = self.run_frame(&mut frame, &code, &state.function, Some(&state.path), inner)?;
        self.callee_flow(flow, &frame, &code, &state.function)
    }

    /// Continue the suspended callee chain `inner` (outermost first) — none
    /// when the path ended at the wait itself — called from `caller`.
    fn resume_callee(&mut self, caller: &Frame, inner: &[VmFrame]) -> Run<Flow> {
        let Some((callee, deeper)) = inner.split_first() else {
            return Ok(Flow::Normal);
        };
        let Some(script) = self
            .deferred
            .vm_scripts
            .get(&callee.script.to_ascii_lowercase())
            .cloned()
        else {
            return stop(format!("script {} is not installed", callee.script));
        };
        let owner = QuestFormId(callee.owner);
        // The properties the call bound (`script_call_target`): the caller's
        // own for a call into its own script, else the quest's.
        let vmad = if owner == caller.owner && Arc::ptr_eq(&script, &caller.script) {
            caller.vmad.clone()
        } else if let Some(vmad) = self.deferred.quest_scripts.get(&owner) {
            Some(vmad.clone())
        } else {
            // A script on another reference: its own properties.
            self.reference_scripts(owner.0)
                .into_iter()
                .find(|(attached, _)| Arc::ptr_eq(attached, &script))
                .map(|(_, vmad)| (*vmad).clone())
        };
        let frame = Frame {
            script,
            owner,
            vmad,
            locals: HashMap::new(),
            defined_in: None,
        };
        self.resume_frame(frame, callee, deeper)
    }

    /// Run `body`, or — `resume` — continue it from the position a wait
    /// suspended it at.
    fn exec_block(
        &mut self,
        frame: &mut Frame,
        body: &[Spanned<Stmt>],
        resume: Option<&[VmStep]>,
        inner: &[VmFrame],
        // `body` is a function's own body, not a branch inside one: nothing of
        // this function follows its last statement.
        ends_function: bool,
    ) -> Run<Flow> {
        let mut start = 0;
        if let Some(path) = resume {
            let Some((VmStep::Statement(index), rest)) = path.split_first() else {
                return stop("a malformed resume path");
            };
            let index = *index as usize;
            let Some(statement) = body.get(index) else {
                return stop("a resume path past its block");
            };
            // The path ends at the wait itself or at the call it waits in.
            let flow = if rest.is_empty() {
                self.resume_callee(frame, inner)?
            } else {
                self.resume_statement(frame, &statement.node, rest, inner)?
            };
            match flow {
                Flow::Normal => {}
                flow => return Ok(suspend_at(VmStep::Statement(index as u32), flow)),
            }
            start = index + 1;
        }
        for (index, statement) in body.iter().enumerate().skip(start) {
            match self.exec(frame, &statement.node)? {
                Flow::Normal => {
                    // The frame budget ran out: suspend after this statement,
                    // as a zero-second `Utility.Wait` here would.
                    if !(ends_function && index + 1 == body.len()) && self.should_yield(frame) {
                        self.yielded = true;
                        return Ok(suspend_at(
                            VmStep::Statement(index as u32),
                            Flow::Suspend(0.0, Vec::new(), Vec::new()),
                        ));
                    }
                }
                flow => return Ok(suspend_at(VmStep::Statement(index as u32), flow)),
            }
        }
        Ok(Flow::Normal)
    }

    /// Continue inside the `If` branch or `While` body a wait suspended.
    fn resume_statement(
        &mut self,
        frame: &mut Frame,
        statement: &Stmt,
        path: &[VmStep],
        inner: &[VmFrame],
    ) -> Run<Flow> {
        let Some((VmStep::Branch(branch), rest)) = path.split_first() else {
            return stop("a malformed resume path");
        };
        match statement {
            Stmt::If {
                body,
                elseif_clauses,
                else_body,
                ..
            } => {
                let block = match *branch {
                    0 => body.as_slice(),
                    u32::MAX => match else_body {
                        Some(body) => body.as_slice(),
                        None => return stop("a resume path into a missing Else"),
                    },
                    k => match elseif_clauses.get(k as usize - 1) {
                        Some((_, body)) => body.as_slice(),
                        None => return stop("a resume path into a missing ElseIf"),
                    },
                };
                let flow = self.exec_block(frame, block, Some(rest), inner, false)?;
                Ok(suspend_at(VmStep::Branch(*branch), flow))
            }
            Stmt::While { condition, body } => {
                match self.exec_block(frame, body, Some(rest), inner, false)? {
                    Flow::Normal => self.exec_while(frame, &condition.node, body),
                    flow => Ok(suspend_at(VmStep::Branch(0), flow)),
                }
            }
            _ => stop("a resume path into a statement without a block"),
        }
    }

    fn exec_while(&mut self, frame: &mut Frame, condition: &Expr, body: &[Spanned<Stmt>]) -> Run<Flow> {
        while self.eval(frame, condition)?.truthy() {
            self.tick()?;
            match self.exec_block(frame, body, None, &[], false)? {
                Flow::Normal => {}
                flow => return Ok(suspend_at(VmStep::Branch(0), flow)),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec(&mut self, frame: &mut Frame, statement: &Stmt) -> Run<Flow> {
        self.tick()?;
        match statement {
            Stmt::VarDecl(variable) => {
                let value = match &variable.initial_value {
                    Some(init) => self.eval(frame, &init.node)?,
                    None => Value::default_for(&variable.ty.node),
                };
                frame
                    .locals
                    .insert(variable.name.node.0.to_ascii_lowercase(), value);
            }
            Stmt::Assign { target, op, value } => {
                let value = self.eval(frame, &value.node)?;
                let value = match op {
                    AssignOp::Eq => value,
                    op => {
                        let current = self.eval(frame, &target.node)?;
                        let binary = match op {
                            AssignOp::PlusEq => BinaryOp::Add,
                            AssignOp::MinusEq => BinaryOp::Sub,
                            AssignOp::MulEq => BinaryOp::Mul,
                            AssignOp::DivEq => BinaryOp::Div,
                            AssignOp::ModEq => BinaryOp::Mod,
                            AssignOp::Eq => unreachable!(),
                        };
                        arithmetic(binary, &current, &value)?
                    }
                };
                self.assign(frame, &target.node, value)?;
            }
            Stmt::Return(value) => {
                let value = match value {
                    Some(value) => self.eval(frame, &value.node)?,
                    None => Value::None,
                };
                return Ok(Flow::Return(value));
            }
            Stmt::If {
                condition,
                body,
                elseif_clauses,
                else_body,
            } => {
                if self.eval(frame, &condition.node)?.truthy() {
                    let flow = self.exec_block(frame, body, None, &[], false)?;
                    return Ok(suspend_at(VmStep::Branch(0), flow));
                }
                for (k, (condition, body)) in elseif_clauses.iter().enumerate() {
                    if self.eval(frame, &condition.node)?.truthy() {
                        let flow = self.exec_block(frame, body, None, &[], false)?;
                        return Ok(suspend_at(VmStep::Branch(k as u32 + 1), flow));
                    }
                }
                if let Some(body) = else_body {
                    let flow = self.exec_block(frame, body, None, &[], false)?;
                    return Ok(suspend_at(VmStep::Branch(u32::MAX), flow));
                }
            }
            Stmt::While { condition, body } => {
                return self.exec_while(frame, &condition.node, body);
            }
            Stmt::ExprStmt(expression) => {
                return self.call_statement(frame, &expression.node);
            }
        }
        Ok(Flow::Normal)
    }

    fn assign(&mut self, frame: &mut Frame, target: &Expr, value: Value) -> Run<()> {
        match target {
            Expr::Ident(name) => {
                let key = name.0.to_ascii_lowercase();
                if let Some(slot) = frame.locals.get_mut(&key) {
                    *slot = value;
                    return Ok(());
                }
                if is_state_variable(&name.0) {
                    let key = state_key(&frame.script);
                    return self.write_object(frame.owner, key, &value);
                }
                // A script variable or property of the frame's quest script.
                self.write_quest_variable(frame.owner, &name.0, &value)
            }
            Expr::MemberAccess { object, member } => {
                let owner = self.eval(frame, &object.node)?;
                if let Value::Struct(members) = &owner {
                    let mut members = members.borrow_mut();
                    return match Value::member(&members, &member.node.0) {
                        Some(index) => {
                            members[index].1 = value;
                            Ok(())
                        }
                        None => stop(format!("struct member {}", member.node.0)),
                    };
                }
                if member.node.0.eq_ignore_ascii_case("value") && matches!(owner, Value::Form(_)) && !self.is_quest_form(&owner) {
                    let Some(number) = value.number() else {
                        return stop("a non-number GlobalVariable.Value");
                    };
                    let (callee, args) = set_value_call(object, &spanned(Expr::FloatLit(f64::from(number))));
                    return self.call_statement(frame, &Expr::Call { callee, args }).map(|_| ());
                }
                match owner {
                    Value::Form(quest) if self.is_quest(quest) => {
                        self.write_quest_variable(QuestFormId(quest), &member.node.0, &value)
                    }
                    // A variable of a script on another reference, kept
                    // under that reference.
                    Value::Form(reference) if reference != 0 && reference != PLAYER_REF => {
                        self.write_quest_variable(QuestFormId(reference), &member.node.0, &value)
                    }
                    _ => stop(format!("assignment to {}'s {}", owner.text(), member.node.0)),
                }
            }
            Expr::Index { object, index } => {
                let array = self.eval(frame, &object.node)?;
                let index = self.eval(frame, &index.node)?.number().unwrap_or(-1.0) as i64;
                let Value::Array(array) = array else {
                    return stop("index assignment to a non-array");
                };
                let mut items = array.borrow_mut();
                let Some(slot) = usize::try_from(index).ok().and_then(|i| items.get_mut(i)) else {
                    return stop("array index out of range");
                };
                *slot = value;
                Ok(())
            }
            _ => stop("unsupported assignment target"),
        }
    }

    /// A quest script variable: a number in the store `GetVMQuestVariable`
    /// and the lowered `SetQuestVariable` share, an object in
    /// [`crate::QuestScriptObjects`].
    fn write_quest_variable(&mut self, quest: QuestFormId, name: &str, value: &Value) -> Run<()> {
        let key = crate::QuestScriptVariables::property_key(name);
        if let Some(number) = value.number() {
            self.deferred.quest_variables.push((quest, key, number));
            return Ok(());
        }
        self.write_object(quest, key, value)
    }

    /// A non-numeric script variable write under `key`.
    fn write_object(&mut self, owner: QuestFormId, key: ConditionStringId, value: &Value) -> Run<()> {
        // An array or struct is a reference value kept for the run and
        // written back at its end.
        if matches!(value, Value::Array(_) | Value::Struct(_)) {
            if to_stored(value).is_none() {
                return stop("an array or struct that cannot be stored in a script variable");
            }
            self.aggregates.borrow_mut().insert((owner.0, key), value.clone());
            return Ok(());
        }
        let Some(object) = to_stored(value) else {
            return stop(format!("a {} write to a script variable", value.text()));
        };
        // A scalar replaces whatever aggregate the run held there.
        self.aggregates.borrow_mut().remove(&(owner.0, key));
        self.deferred.quest_objects.push((owner, key, object));
        Ok(())
    }

    /// An `InputEnableLayer` native on a layer handle (see [`crate::input_layers`]):
    /// `Disable` / `EnablePlayerControls`, `Enable<Control>`, `Is<Control>Enabled`,
    /// `Reset`, `Delete`. `None` when `callee` is not one on a layer.
    fn layer_call(&mut self, frame: &mut Frame, callee: &Expr, args: &[CallArg]) -> Run<Option<Value>> {
        let Expr::MemberAccess { object, member } = callee else {
            return Ok(None);
        };
        let name = member.node.0.to_ascii_lowercase();
        let Some(native) = layer_native(&name) else {
            return Ok(None);
        };
        if static_class(&object.node).is_some() {
            return Ok(None);
        }
        let Value::Int(layer) = self.eval(frame, &object.node)? else {
            return Ok(None);
        };
        let layer = layer as u32;
        let values = self.eval_args(frame, args)?;
        let flag = |index: usize, default: bool| values.get(index).map_or(default, Value::truthy);
        let Some(mut layers) = self.world.try_resource_mut::<crate::InputLayers>() else {
            return stop("an input layer without the layer store");
        };
        let result = match native {
            LayerNative::Query(control) => return Ok(Some(Value::Bool(layers.layer_enabled(layer, control)))),
            LayerNative::Disable | LayerNative::Enable => {
                let enabling = native == LayerNative::Enable;
                for (index, control) in crate::Control::PARAMETER_ORDER.into_iter().enumerate() {
                    let default = if enabling { true } else { crate::Control::DISABLE_DEFAULTS[index] };
                    if flag(index, default) {
                        layers.set_enabled(layer, control, enabling);
                    }
                }
                Value::None
            }
            LayerNative::Set(control) => {
                layers.set_enabled(layer, control, flag(0, true));
                Value::None
            }
            LayerNative::Reset => {
                layers.reset(layer);
                Value::None
            }
            LayerNative::Delete => {
                layers.delete(layer);
                Value::None
            }
        };
        if let Some(mut state) = self.world.try_resource_mut::<crate::player_control::PlayerControlState>() {
            layers.sync_state(&mut state);
        }
        Ok(Some(result))
    }

    /// A `RefCollectionAlias` native on a collection alias: `GetCount`,
    /// `GetAt`, `Find`, `AddRef`, `AddArray`, `AddRefCollection`, `RemoveRef`,
    /// `RemoveAll`. `None` when `callee` is not one on an alias. The collection
    /// must be one the quest leaves to its scripts (see
    /// [`crate::alias_collections`]).
    fn collection_call(&mut self, frame: &mut Frame, callee: &Expr, args: &[CallArg]) -> Run<Option<Value>> {
        let Expr::MemberAccess { object, member } = callee else {
            return Ok(None);
        };
        let name = member.node.0.to_ascii_lowercase();
        if !COLLECTION_NATIVES.contains(&name.as_str()) || static_class(&object.node).is_some() {
            return Ok(None);
        }
        let Value::Alias { quest, alias } = self.eval(frame, &object.node)? else {
            return Ok(None);
        };
        let key = (quest, alias);
        self.require_script_filled(key)?;
        if !self.collections.borrow().contains_key(&key) {
            let stored: Vec<u32> = self
                .world
                .try_resource::<crate::AliasCollections>()
                .map(|collections| collections.members(key).to_vec())
                .unwrap_or_default();
            self.collections.borrow_mut().insert(key, stored);
        }
        let values = self.eval_args(frame, args)?;
        let form_of = |value: Option<&Value>| match value {
            Some(Value::Form(form)) if *form != 0 => Some(*form),
            _ => None,
        };
        let mut collections = self.collections.borrow_mut();
        let members = collections.get_mut(&key).expect("loaded above");
        Ok(Some(match name.as_str() {
            "getcount" => Value::Int(members.len() as i32),
            "getat" => {
                let index = values.first().and_then(Value::int).unwrap_or(-1);
                usize::try_from(index)
                    .ok()
                    .and_then(|index| members.get(index))
                    .map_or(Value::None, |form| Value::Form(*form))
            }
            "getarray" | "getactorarray" => Value::Array(Rc::new(RefCell::new(
                members.iter().map(|form| Value::Form(*form)).collect(),
            ))),
            // CK *EvaluateAll - RefCollectionAlias*: "Calls EvaluatePackage on
            // all the actors in the collection."
            "evaluateall" => {
                if let Some(mut requests) = self.world.query_mut::<crate::EvaluatePackageRequest>() {
                    for form in members.iter() {
                        if let Some(entity) = crate::condition::resolve_entity_by_global_form_id(self.world, *form) {
                            requests.insert(entity, crate::EvaluatePackageRequest);
                        }
                    }
                }
                Value::None
            }
            // CK: "Calls EnableNoWait / DisableNoWait on all the references in
            // the collection."
            "enableall" | "disableall" => {
                let enabled = name == "enableall";
                for form in members.iter() {
                    self.deferred.reference_enable_changes.push((*form, enabled));
                }
                Value::None
            }
            "find" => Value::Int(match form_of(values.first()) {
                Some(form) => members.iter().position(|member| *member == form).map_or(-1, |i| i as i32),
                None => -1,
            }),
            "addref" => {
                if let Some(form) = form_of(values.first()) {
                    if !members.contains(&form) {
                        members.push(form);
                    }
                }
                Value::None
            }
            "addarray" => {
                // CK: "Reference Aliases inside arrays will not be added to a
                // collection if they are none."
                if let Some(Value::Array(items)) = values.first() {
                    for item in items.borrow().iter() {
                        if let Value::Form(form) = item {
                            if *form != 0 && !members.contains(form) {
                                members.push(*form);
                            }
                        }
                    }
                }
                Value::None
            }
            "addrefcollection" => {
                let Some(Value::Alias { quest: other_quest, alias: other_alias }) = values.first() else {
                    return Ok(Some(Value::None));
                };
                let other = (*other_quest, *other_alias);
                drop(collections);
                self.require_script_filled(other)?;
                let others: Vec<u32> = self
                    .collections
                    .borrow()
                    .get(&other)
                    .cloned()
                    .or_else(|| {
                        self.world
                            .try_resource::<crate::AliasCollections>()
                            .map(|collections| collections.members(other).to_vec())
                    })
                    .unwrap_or_default();
                let mut collections = self.collections.borrow_mut();
                let members = collections.get_mut(&key).expect("loaded above");
                for form in others {
                    if !members.contains(&form) {
                        members.push(form);
                    }
                }
                Value::None
            }
            "removeref" => {
                if let Some(form) = form_of(values.first()) {
                    members.retain(|member| *member != form);
                }
                Value::None
            }
            "removeall" => {
                members.clear();
                Value::None
            }
            _ => return Ok(None),
        }))
    }

    /// Stop unless the collection alias `key` is one the quest leaves to its
    /// scripts: a collection alias with no fill type and no match conditions.
    fn require_script_filled(&self, key: crate::alias_collections::CollectionKey) -> Run<()> {
        match self.deferred.collection_kinds.get(&key) {
            Some(true) => Ok(()),
            Some(false) => stop("a reference collection filled by the quest's conditions"),
            None => stop("a collection native on an alias that is not a known reference collection"),
        }
    }

    /// Write every array and struct the run touched back as a stored object.
    fn flush_aggregates(&mut self) {
        let aggregates = std::mem::take(&mut *self.aggregates.borrow_mut());
        for ((owner, key), value) in aggregates {
            if let Some(object) = to_stored(&value) {
                self.deferred.quest_objects.push((QuestFormId(owner), key, object));
            }
        }
    }

    /// An object or string a script wrote to `quest`'s variable `name`, if
    /// any.
    fn written_quest_object(&self, quest: QuestFormId, name: &str) -> Option<Value> {
        self.written_object(quest, crate::QuestScriptVariables::property_key(name))
    }

    fn written_object(&self, owner: QuestFormId, key: ConditionStringId) -> Option<Value> {
        if let Some(held) = self.aggregates.borrow().get(&(owner.0, key)) {
            return Some(held.clone());
        }
        let pending = self
            .deferred
            .quest_objects
            .iter()
            .rev()
            .find(|(q, k, _)| *q == owner && *k == key)
            .map(|(_, _, value)| value.clone());
        let stored = pending.or_else(|| {
            self.world
                .try_resource::<crate::QuestScriptObjects>()
                .and_then(|objects| objects.get(owner, key))
        });
        let value = from_stored(&stored?);
        // A stored array or struct becomes the run's one shared value.
        if matches!(value, Value::Array(_) | Value::Struct(_)) {
            self.aggregates.borrow_mut().insert((owner.0, key), value.clone());
        }
        Some(value)
    }

    fn read_quest_variable(&self, quest: QuestFormId, name: &str) -> Option<f32> {
        let key = crate::QuestScriptVariables::property_key(name);
        self.deferred
            .quest_variables
            .iter()
            .rev()
            .find(|(q, k, _)| *q == quest && *k == key)
            .map(|(_, _, value)| *value)
            .or_else(|| crate::quest_variable(self.world, quest, key))
    }

    fn property(vmad: Option<&ScriptInstanceData>, name: &str) -> Option<Value> {
        let bare = vmad_property_name(name);
        vmad?
            .scripts
            .iter()
            .find_map(|script| script.property(bare))
            .map(|property| Value::from_property(&property.value))
    }

    fn eval(&mut self, frame: &mut Frame, e: &Expr) -> Run<Value> {
        Ok(match e {
            Expr::IntLit(i) => Value::Int(*i as i32),
            Expr::FloatLit(f) => Value::Float(*f as f32),
            Expr::BoolLit(b) => Value::Bool(*b),
            Expr::StringLit(s) => Value::Str(s.clone()),
            Expr::NoneLit => Value::None,
            Expr::Ident(name) => self.read_ident(frame, &name.0)?,
            Expr::Cast { expr, target_type } => {
                let value = self.eval(frame, &expr.node)?;
                cast(value, &target_type.node)
            }
            Expr::UnaryOp { op, operand } => {
                let value = self.eval(frame, &operand.node)?;
                match op {
                    UnaryOp::Not => Value::Bool(!value.truthy()),
                    UnaryOp::Neg => match value {
                        Value::Int(i) => Value::Int(-i),
                        Value::Float(f) => Value::Float(-f),
                        other => return stop(format!("negating {}", other.text())),
                    },
                }
            }
            Expr::BinaryOp { left, op, right } => match op {
                BinaryOp::And => {
                    let l = self.eval(frame, &left.node)?;
                    if !l.truthy() {
                        Value::Bool(false)
                    } else {
                        Value::Bool(self.eval(frame, &right.node)?.truthy())
                    }
                }
                BinaryOp::Or => {
                    let l = self.eval(frame, &left.node)?;
                    if l.truthy() {
                        Value::Bool(true)
                    } else {
                        Value::Bool(self.eval(frame, &right.node)?.truthy())
                    }
                }
                op => {
                    let l = self.eval(frame, &left.node)?;
                    let r = self.eval(frame, &right.node)?;
                    binary(*op, &l, &r)?
                }
            },
            Expr::Index { object, index } => {
                let array = self.eval(frame, &object.node)?;
                let index = self.eval(frame, &index.node)?.number().unwrap_or(-1.0) as i64;
                let Value::Array(array) = array else {
                    return stop("indexing a non-array");
                };
                let items = array.borrow();
                match usize::try_from(index).ok().and_then(|i| items.get(i)) {
                    Some(value) => value.clone(),
                    None => return stop("array index out of range"),
                }
            }
            Expr::NewStruct { ty } => {
                let name = match &ty.node {
                    Type::Object(name) => name.0.clone(),
                    _ => String::new(),
                };
                match self.new_struct(frame, &name) {
                    Some(value) => value,
                    None => return stop(format!("no struct {name}")),
                }
            }
            Expr::New { ty, size } => {
                let size = self.eval(frame, &size.node)?.number().unwrap_or(0.0).max(0.0) as usize;
                if size > MAX_ARRAY_ELEMENTS {
                    return stop(format!("array size {size} over {MAX_ARRAY_ELEMENTS}"));
                }
                Value::Array(Rc::new(RefCell::new(vec![Value::default_for(&ty.node); size])))
            }
            Expr::ArrayLit(items) => {
                if items.len() > MAX_ARRAY_ELEMENTS {
                    return stop(format!("array literal over {MAX_ARRAY_ELEMENTS} elements"));
                }
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(frame, &item.node)?);
                }
                Value::Array(Rc::new(RefCell::new(values)))
            }
            Expr::MemberAccess { object, member } => {
                let owner = self.eval(frame, &object.node)?;
                let name = &member.node.0;
                if name.eq_ignore_ascii_case("length") {
                    if let Value::Array(array) = &owner {
                        return Ok(Value::Int(array.borrow().len() as i32));
                    }
                }
                if let Value::Struct(members) = &owner {
                    let members = members.borrow();
                    return match Value::member(&members, name) {
                        Some(index) => Ok(members[index].1.clone()),
                        None => stop(format!("struct member {name}")),
                    };
                }
                self.read_member(&owner, name)?
            }
            Expr::Call { callee, args } => self.call_value(frame, &callee.node, args)?,
            Expr::Is { .. } | Expr::ParentAccess => return stop("unsupported expression"),
        })
    }

    fn read_ident(&mut self, frame: &mut Frame, name: &str) -> Run<Value> {
        let key = name.to_ascii_lowercase();
        if let Some(value) = frame.locals.get(&key) {
            return Ok(value.clone());
        }
        if key == "self" {
            return Ok(self.self_value(frame.owner));
        }
        if is_state_variable(name) {
            let script = Arc::clone(&frame.script);
            return Ok(Value::Str(self.script_state(frame.owner, &script)));
        }
        let script = Arc::clone(&frame.script);
        match self.read_script_member(frame.owner, &script, frame.vmad.as_ref(), name) {
            Some(value) => Ok(value),
            None => stop(format!("unknown name {name}")),
        }
    }

    /// What `Self` is for the instance `owner` owns: the alias an alias script
    /// is attached to, else the quest or reference itself.
    fn self_value(&self, owner: QuestFormId) -> Value {
        match self.deferred.alias_owners.get(&owner.0) {
            Some(&(quest, alias)) => Value::Alias { quest, alias },
            None => Value::Form(owner.0),
        }
    }

    /// `owner`'s instance of `script` (its properties `vmad`): the property
    /// or script variable `name` — what a script wrote, else the VMAD's
    /// value, else what the script declares. `None` when the script has no
    /// such member.
    fn read_script_member(
        &self,
        owner: QuestFormId,
        script: &Arc<Script>,
        vmad: Option<&ScriptInstanceData>,
        name: &str,
    ) -> Option<Value> {
        if let Some(value) = Self::property(vmad, name) {
            // A numeric property a script wrote reads the written value.
            if value.number().is_some() {
                if let Some(written) = self.read_quest_variable(owner, name) {
                    return Some(if value.is_float() {
                        Value::Float(written)
                    } else {
                        Value::Int(written as i32)
                    });
                }
            }
            // An object property a script re-pointed reads the new object; an
            // array or struct property is one shared value across the run,
            // starting from what a script last wrote or else the VMAD's.
            if matches!(value, Value::Array(_) | Value::Struct(_)) {
                return Some(self.aggregate(owner, name, value));
            }
            return Some(self.written_quest_object(owner, name).unwrap_or(value));
        }
        let declared = || {
            script_chain(script, &self.deferred.vm_scripts)
                .iter()
                .find_map(|script| declared_default(script, name))
        };
        Some(match member_type_in(script, name, &self.deferred.vm_scripts)? {
            ty @ (Type::Int | Type::Float | Type::Bool) => {
                let value = match self.read_quest_variable(owner, name) {
                    Some(value) => value,
                    None => declared().and_then(|value| value.number()).unwrap_or(0.0),
                };
                match ty {
                    Type::Int => Value::Int(value as i32),
                    Type::Bool => Value::Bool(value != 0.0),
                    _ => Value::Float(value),
                }
            }
            Type::String => self
                .written_quest_object(owner, name)
                .or_else(declared)
                .unwrap_or_else(|| Value::Str(String::new())),
            Type::Object(_) | Type::Var => self.written_quest_object(owner, name).unwrap_or(Value::None),
            // An array nothing has set yet is empty.
            Type::Array(_) => {
                let empty = Value::Array(Rc::new(RefCell::new(Vec::new())));
                self.aggregate(owner, name, empty)
            }
        })
    }

    /// The array or struct held by `owner`'s variable `name`: this run's
    /// shared value, else what a script last wrote, else `default`.
    fn aggregate(&self, owner: QuestFormId, name: &str, default: Value) -> Value {
        let key = crate::QuestScriptVariables::property_key(name);
        if let Some(held) = self.written_object(owner, key) {
            if matches!(held, Value::Array(_) | Value::Struct(_)) {
                return held;
            }
        }
        self.aggregates.borrow_mut().insert((owner.0, key), default.clone());
        default
    }

    /// The scripts the VM runs on the reference `form`, with their
    /// properties: (script, its VMAD), loaded references only.
    fn reference_scripts(&self, form: u32) -> Vec<(Arc<Script>, Arc<ScriptInstanceData>)> {
        let Some(entity) = super::ReferenceScriptIndex::lookup(self.world, form) else {
            return Vec::new();
        };
        let Some(attached) = self.world.get::<super::PapyrusReferenceScripts>(entity) else {
            return Vec::new();
        };
        attached
            .0
            .iter()
            .filter_map(|r| {
                let script = self.deferred.vm_scripts.get(&r.script.to_ascii_lowercase())?;
                Some((Arc::clone(script), Arc::clone(&r.vmad)))
            })
            .collect()
    }

    /// `quest.Variable` — another quest script's property or variable.
    fn read_member(&mut self, owner: &Value, name: &str) -> Run<Value> {
        let Value::Form(form) = owner else {
            return stop(format!("member {name} of {}", owner.text()));
        };
        if !self.is_quest(*form) {
            if let Some(value) = self.engine_constant(name) {
                return Ok(value);
            }
            // A member of a script the VM runs on that reference.
            let owner = QuestFormId(*form);
            for (script, vmad) in self.reference_scripts(*form) {
                if let Some(value) = self.read_script_member(owner, &script, Some(&vmad), name) {
                    return Ok(value);
                }
            }
        }
        let quest = QuestFormId(*form);
        let vmad = self.deferred.quest_scripts.get(&quest).cloned();
        // A member of a script attached to the quest the VM has installed:
        // what a script wrote, the VMAD's value, or what it declares.
        if let Some(vmad) = &vmad {
            for attached in &vmad.scripts {
                let Some(script) = self.deferred.vm_scripts.get(&attached.name.to_ascii_lowercase()).cloned() else {
                    continue;
                };
                if let Some(value) = self.read_script_member(quest, &script, Some(vmad), name) {
                    return Ok(value);
                }
            }
        }
        if let Some(value) = Self::property(vmad.as_ref(), name) {
            return Ok(value);
        }
        // `GlobalVariable.Value`.
        if name.eq_ignore_ascii_case("value") {
            return Ok(self
                .world
                .try_resource::<crate::Globals>()
                .and_then(|globals| globals.get(*form))
                .map_or(Value::None, Value::Float));
        }
        match self.read_quest_variable(quest, name) {
            Some(value) => Ok(Value::Float(value)),
            None => stop(format!("member {name} of {form:08X}")),
        }
    }

    /// A new instance of the struct type `name` (`Script#Struct`, or a
    /// struct of the running script's chain), its members at their defaults.
    fn new_struct(&self, frame: &Frame, name: &str) -> Option<Value> {
        let declaration = struct_declaration(&frame.script, name, &self.deferred.vm_scripts)?;
        let members = declaration
            .members
            .iter()
            .map(|member| {
                let value = member
                    .initial_value
                    .as_ref()
                    .and_then(|initial| literal_value(&initial.node))
                    .unwrap_or_else(|| Value::default_for(&member.ty.node));
                (member.name.node.0.clone(), value)
            })
            .collect();
        Some(Value::Struct(Rc::new(RefCell::new(members))))
    }

    /// An array method (CK *Arrays (Papyrus)*), when `callee` calls one on an
    /// array: `Find` / `RFind`, `FindStruct` / `RFindStruct`, `Add`, `Insert`,
    /// `Remove`, `RemoveLast`, `Clear`.
    fn array_call(&mut self, frame: &mut Frame, callee: &Expr, args: &[CallArg]) -> Run<Option<Value>> {
        let Expr::MemberAccess { object, member } = callee else {
            return Ok(None);
        };
        let name = member.node.0.to_ascii_lowercase();
        if !ARRAY_METHODS.contains(&name.as_str()) || static_class(&object.node).is_some() {
            return Ok(None);
        }
        let Value::Array(array) = self.eval(frame, &object.node)? else {
            return Ok(None);
        };
        let values = self.eval_args(frame, args)?;
        let int = |index: usize, default: i32| -> i32 { values.get(index).and_then(Value::int).unwrap_or(default) };
        let mut items = array.borrow_mut();
        let len = items.len() as i32;
        // The member a FindStruct compares.
        let matches = |item: &Value, member: Option<&str>, wanted: &Value| -> bool {
            match member {
                None => item.equals(wanted),
                Some(member) => match item {
                    Value::Struct(members) => {
                        let members = members.borrow();
                        Value::member(&members, member).is_some_and(|index| members[index].1.equals(wanted))
                    }
                    _ => false,
                },
            }
        };
        let (member, wanted, start_arg) = match name.as_str() {
            "findstruct" | "rfindstruct" => {
                let Some(Value::Str(member)) = values.first() else {
                    return stop(format!("{name} without a member name"));
                };
                (Some(member.clone()), values.get(1).cloned().unwrap_or(Value::None), 2)
            }
            _ => (None, values.first().cloned().unwrap_or(Value::None), 1),
        };
        Ok(Some(match name.as_str() {
            "find" | "findstruct" => {
                let start = int(start_arg, 0).max(0);
                (start..len)
                    .find(|&i| matches(&items[i as usize], member.as_deref(), &wanted))
                    .map_or(Value::Int(-1), Value::Int)
            }
            "rfind" | "rfindstruct" => {
                let start = int(start_arg, -1);
                let start = if start < 0 || start >= len { len - 1 } else { start };
                (0..=start)
                    .rev()
                    .find(|&i| matches(&items[i as usize], member.as_deref(), &wanted))
                    .map_or(Value::Int(-1), Value::Int)
            }
            "add" => {
                // "in the case of adding structs, they will all point at the
                // same struct" — the value is shared, not copied.
                let count = int(1, 1).max(0) as usize;
                if count > MAX_ARRAY_ELEMENTS.saturating_sub(items.len()) {
                    return stop(format!("array Add over {MAX_ARRAY_ELEMENTS} elements"));
                }
                for _ in 0..count {
                    items.push(wanted.clone());
                }
                Value::None
            }
            "insert" => {
                if items.len() >= MAX_ARRAY_ELEMENTS {
                    return stop(format!("array Insert over {MAX_ARRAY_ELEMENTS} elements"));
                }
                let index = int(1, 0).clamp(0, len) as usize;
                items.insert(index, wanted);
                Value::None
            }
            "remove" => {
                let index = int(0, 0);
                let count = int(1, 1).max(0);
                if index < 0 || index >= len {
                    return stop("array Remove out of range");
                }
                let end = (index + count).min(len);
                items.drain(index as usize..end as usize);
                Value::None
            }
            "removelast" => {
                items.pop();
                Value::None
            }
            "clear" => {
                items.clear();
                Value::None
            }
            _ => return Ok(None),
        }))
    }

    /// An engine class's constant `name`, from the installed classes.
    fn engine_constant(&self, name: &str) -> Option<Value> {
        CONSTANT_CLASSES.iter().find_map(|class| {
            let script = self.deferred.vm_scripts.get(*class)?;
            constant_property(script, name).and_then(property_constant)
        })
    }

    /// The quest script that defines `function` for the quest `quest`, if
    /// any, and the body it runs.
    fn quest_script_with(&self, quest: QuestFormId, function: &str) -> Option<(Arc<Script>, Code)> {
        let vmad = self.deferred.quest_scripts.get(&quest)?;
        vmad.scripts.iter().find_map(|attached| {
            let script = self.deferred.vm_scripts.get(&attached.name.to_ascii_lowercase())?;
            let code = self.resolve(script, quest, function)?;
            Some((Arc::clone(script), code))
        })
    }

    /// The reference FormID a per-actor script state is keyed by.
    fn actor_reference(&self, actor: u32) -> Option<u32> {
        let entity = self.entity_of(actor)?;
        if actor == PLAYER_REF {
            Some(PLAYER_REF)
        } else {
            self.entity_form(entity)
        }
    }

    fn eval_args(&mut self, frame: &mut Frame, args: &[CallArg]) -> Run<Vec<Value>> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(self.eval(frame, &arg.value.node)?);
        }
        Ok(values)
    }

    /// A call to a script function, when `callee` names one: the frame's own
    /// script (`Fn()`, `Self.Fn()`) or a quest's attached script.
    fn script_call(&mut self, frame: &mut Frame, callee: &Expr, args: &[CallArg]) -> Run<Option<Value>> {
        let Some((callee_frame, code)) = self.script_call_target(frame, callee)? else {
            return Ok(None);
        };
        let values = self.eval_args(frame, args)?;
        self.call_function(callee_frame, &code, &callee_name(callee), values).map(Some)
    }

    /// The fresh frame a call to a script function runs in, and the body it
    /// runs, when `callee` names one.
    fn script_call_target(&mut self, frame: &mut Frame, callee: &Expr) -> Run<Option<(Frame, Code)>> {
        let (receiver, name) = match callee {
            Expr::Ident(name) => (None, name.0.as_str()),
            Expr::MemberAccess { object, member } => (Some(&object.node), member.node.0.as_str()),
            _ => return Ok(None),
        };
        let own = |vm: &Self, frame: &Frame| {
            vm.resolve(&frame.script, frame.owner, name)
                .map(|code| (Arc::clone(&frame.script), frame.owner, frame.vmad.clone(), code))
        };
        let target = match receiver {
            None => own(self, frame),
            Some(object) if static_class(object).is_some() => None,
            // `Parent.Fn()`: the version of the script the running function is
            // defined in extends; an engine class's event (`Parent.OnInit()`)
            // has no body to run.
            Some(Expr::ParentAccess) => {
                let parent = frame
                    .defined_in
                    .as_ref()
                    .and_then(|defined| defined.parent.as_ref())
                    .and_then(|parent| self.deferred.vm_scripts.get(&parent.node.0.to_ascii_lowercase()).cloned());
                let state = self.script_state(frame.owner, &frame.script);
                parent
                    .and_then(|parent| resolve_in(&parent, &state, name, &self.deferred.vm_scripts))
                    .map(|code| (Arc::clone(&frame.script), frame.owner, frame.vmad.clone(), code))
            }
            // `ScriptName.Fn()`: a global function, which has no `Self`.
            Some(Expr::Ident(script_name)) if self.global_script(frame, &script_name.0).is_some() => {
                let script = self.global_script(frame, &script_name.0).expect("checked above");
                resolve_in(&script, "", name, &self.deferred.vm_scripts)
                    .map(|code| (script, QuestFormId(0), None, code))
            }
            Some(object) => match self.eval(frame, object)? {
                Value::Form(form) if form == frame.owner.0 && own(self, frame).is_some() => own(self, frame),
                // An alias script calling itself through `Self`.
                value @ Value::Alias { .. }
                    if self.self_value(frame.owner).same_alias(&value) && own(self, frame).is_some() =>
                {
                    own(self, frame)
                }
                Value::Form(form) if self.is_quest(form) => {
                    self.quest_script_with(QuestFormId(form), name).map(|(script, code)| {
                        let vmad = self.deferred.quest_scripts.get(&QuestFormId(form)).cloned();
                        (script, QuestFormId(form), vmad, code)
                    })
                }
                // A function of a script the VM runs on another reference.
                Value::Form(form) if form != 0 => self.reference_scripts(form).into_iter().find_map(|(script, vmad)| {
                    let code = self.resolve(&script, QuestFormId(form), name)?;
                    Some((script, QuestFormId(form), Some((*vmad).clone()), code))
                }),
                _ => None,
            },
        };
        Ok(target.map(|(script, owner, vmad, code)| {
            (
                Frame {
                    script,
                    owner,
                    vmad,
                    locals: HashMap::new(),
                    defined_in: None,
                },
                code,
            )
        }))
    }

    /// The script `name` names, when it is not a value in `frame`: the
    /// receiver of a global call.
    fn global_script(&self, frame: &Frame, name: &str) -> Option<Arc<Script>> {
        let key = name.to_ascii_lowercase();
        if key == "self" || frame.locals.contains_key(&key) {
            return None;
        }
        if Self::property(frame.vmad.as_ref(), name).is_some()
            || member_type_in(&frame.script, name, &self.deferred.vm_scripts).is_some()
        {
            return None;
        }
        self.deferred.vm_scripts.get(&key).cloned()
    }

    /// The object reads no effect models.
    fn object_read(&mut self, frame: &mut Frame, callee: &Expr, args: &[CallArg]) -> Run<Option<Value>> {
        // The Form natives a script calls on itself without a receiver
        // (`RegisterForAnimationEvent(Self, "Reset")`) read as `Self.<name>(...)`.
        if let Expr::Ident(called) = callee {
            if matches!(
                called.0.to_ascii_lowercase().as_str(),
                "registerforanimationevent" | "unregisterforanimationevent"
            ) {
                let on_self = Expr::MemberAccess {
                    object: Box::new(ident("self")),
                    member: spanned(called.clone()),
                };
                return self.object_read(frame, &on_self, args);
            }
        }
        let Expr::MemberAccess { object, member } = callee else {
            return Ok(None);
        };
        let name = member.node.0.to_ascii_lowercase();
        // CK *Create - InputEnableLayer*: a new layer, as its handle. The class
        // is an engine type the script names, not a value.
        if name == "create"
            && matches!(&object.node, Expr::Ident(class) if class.0.eq_ignore_ascii_case("inputenablelayer"))
            && !frame.locals.contains_key("inputenablelayer")
        {
            let Some(mut layers) = self.world.try_resource_mut::<crate::InputLayers>() else {
                return stop("InputEnableLayer.Create without the layer store");
            };
            return Ok(Some(Value::Int(layers.create() as i32)));
        }
        if let Some(class) = static_class(&object.node) {
            return Ok(match (class, name.as_str()) {
                ("game", "getplayer") => Some(Value::Form(PLAYER_REF)),
                // CK *ShakeCamera - Game*: shakes the camera "from the specified
                // object's location" (None: the player's) with `afStrength`
                // (default 0.5, clamped to 0..1) for `afDuration` seconds, where 0
                // (the default) means the `fCameraShakeTime` game setting - 1.25 in
                // SkyrimSE.exe's table (no plugin sets it). The camera subsystem
                // drains the player's `CameraShakeCommand`; the source's distance
                // is not modelled.
                ("game", "shakecamera") if args.len() <= 3 => {
                    let values = self.eval_args(frame, args)?;
                    let strength = values.get(1).and_then(Value::number).unwrap_or(0.5).clamp(0.0, 1.0);
                    let duration = values.get(2).and_then(Value::number).unwrap_or(0.0);
                    self.deferred.camera_shakes.push(crate::papyrus_demo::CameraShakeCommand {
                        intensity: strength,
                        duration_secs: if duration > 0.0 { duration } else { 1.25 },
                    });
                    Some(Value::None)
                }
                // CK *ShakeController - Game*: both motors clamped to 0..1, for the
                // duration; the input subsystem drains `ControllerRumbleCommand`.
                ("game", "shakecontroller") if args.len() == 3 => {
                    let values = self.eval_args(frame, args)?;
                    let motor = |i: usize| values.get(i).and_then(Value::number).unwrap_or(0.0).clamp(0.0, 1.0);
                    self.deferred.controller_shakes.push(crate::papyrus_demo::ControllerRumbleCommand {
                        left: motor(0),
                        right: motor(1),
                        duration_secs: values.get(2).and_then(Value::number).unwrap_or(0.0).max(0.0),
                    });
                    Some(Value::None)
                }
                // CK *GetCaps - Game*: "the MiscObject representing caps".
                // Fallout 4's is `Caps001`, form `0x0F` of `Fallout4.esm`
                // (checked against the record); no other game's page or
                // record is the one asked for.
                ("game", "getcaps") if args.is_empty() => {
                    let fallout4 = self
                        .world
                        .try_resource::<crate::ScriptGame>()
                        .is_some_and(|game| game.0 == byroredux_plugin::esm::reader::GameKind::Fallout4);
                    if !fallout4 {
                        return stop("Game.GetCaps outside Fallout 4");
                    }
                    Some(Value::Form(FO4_CAPS))
                }
                // `Game.GetCredits()` (Starfield, no CK page): the corpus uses it
                // as the currency *form* — `Player.RemoveItem(Game.GetCredits() as
                // Form, cost)` in `DialogueShipServicesScript` — the Starfield
                // twin of `GetCaps`. The form is `Credits`, `0x0F` of
                // `Starfield.esm` (checked against the record).
                ("game", "getcredits") if args.is_empty() => {
                    let starfield = self
                        .world
                        .try_resource::<crate::ScriptGame>()
                        .is_some_and(|game| game.0 == byroredux_plugin::esm::reader::GameKind::Starfield);
                    if !starfield {
                        return stop("Game.GetCredits outside Starfield");
                    }
                    Some(Value::Form(STARFIELD_CREDITS))
                }
                // CK *GetForm - Game*: the form of that ID, or None when the
                // ID names no loaded plugin.
                ("game", "getform") => {
                    let values = self.eval_args(frame, args)?;
                    let Some(Value::Int(id)) = values.first() else {
                        return stop("GetForm without an Int form id");
                    };
                    let id = *id as u32;
                    let valid = id != 0
                        && self
                            .world
                            .try_resource::<crate::load_order::LoadOrderIdentity>()
                            .is_some_and(|identity| identity.pair(id).is_some());
                    Some(if valid { Value::Form(id) } else { Value::None })
                }
                // CK *GetCurrentWeather - Weather*.
                ("weather", "getcurrentweather") => Some(
                    self.world
                        .try_resource::<crate::CurrentWeather>()
                        .and_then(|weather| weather.0)
                        .map_or(Value::None, Value::Form),
                ),
                ("math", function) if MATH_NATIVES.contains(&function) => {
                    let values = self.eval_args(frame, args)?;
                    math_native(function, &values)?
                }
                // CK *RandomFloat - Utility*: "a random floating point number
                // between the minimum and maximum values (inclusive)"; the
                // defaults are 0.0 and 1.0.
                ("utility", "randomfloat") => {
                    let values = self.eval_args(frame, args)?;
                    let bound = |index: usize, default: f32| values.get(index).and_then(Value::number).unwrap_or(default);
                    let (low, high) = (bound(0, 0.0), bound(1, 1.0));
                    let Some(draw) = self
                        .world
                        .try_resource_mut::<crate::story_manager::StoryManagerRng>()
                        .map(|mut rng| rng.unit_inclusive())
                    else {
                        return stop("Utility.RandomFloat without the random generator");
                    };
                    Some(Value::Float(low + draw * (high - low)))
                }
                // CK *GetCurrentGameTime - Utility*: "game days passed".
                // CK *GetCurrentRealTime - Utility*: "the number of real-world
                // seconds that have elapsed since the game has launched" (time in
                // a menu counts). `TotalTime` is the engine's wall-clock since
                // start.
                ("utility", "getcurrentrealtime") => {
                    let seconds = self.world.try_resource::<byroredux_core::ecs::TotalTime>().map_or(0.0, |t| t.0);
                    Some(Value::Float(seconds))
                }
                ("utility", "getcurrentgametime") => {
                    let hours = self
                        .world
                        .try_resource::<super::ScriptUpdates>()
                        .and_then(|updates| updates.game_hours())
                        .unwrap_or(0.0);
                    Some(Value::Float((hours / 24.0) as f32))
                }
                ("game", "getformfromfile") => {
                    let values = self.eval_args(frame, args)?;
                    // The id exactly: an f32 (`Value::number`) cannot hold
                    // every 32-bit form id.
                    let form_id = match values.first() {
                        Some(Value::Int(id)) => *id as u32,
                        _ => return stop("GetFormFromFile without an Int form id"),
                    };
                    let Some(Value::Str(file)) = values.get(1) else {
                        return stop("GetFormFromFile without a file");
                    };
                    let plugin = byroredux_core::form_id::PluginId::from_filename(file);
                    let form = self
                        .world
                        .try_resource::<crate::load_order::LoadOrderIdentity>()
                        .and_then(|identity| identity.form_from_file(form_id as u32, plugin));
                    Some(form.map_or(Value::None, Value::Form))
                }
                _ => None,
            });
        }
        const ALIAS_GETTERS: [&str; 6] = [
            "getref",
            "getreference",
            "getactorref",
            "getactorreference",
            "getshipref",
            "getshipreference",
        ];
        let reads = ALIAS_GETTERS.contains(&name.as_str())
            || matches!(
                name.as_str(),
                "getowningquest"
                    | "getlinkedref"
                    | "getnthlinkedref"
                    | "getbaseobject"
                    | "getactorbase"
                    | "getleveledactorbase"
                    | "getlocation"
                    | "getcurrentlocation"
                    | "getdialoguetarget"
                    | "placeatme"
                    | "ischild"
                    | "trytogetcurrentlocation"
                    | "getclassification"
                    | "getsize"
                    | "getat"
                    | "hasform"
                    | "find"
                    | "gettriggerobjectcount"
                    | "getrace"
                    | "getscale"
                    | "setnofavorallowed"
                    | "ignorefriendlyhits"
                    | "setcandocommand"
                    | "setactorcause"
                    | "getfactionowner"
                    | "getactorowner"
                    | "calculateencounterlevel"
                    | "damageobject"
                    | "processtraphit"
                    | "hasspell"
                    | "createdetectionevent"
                    | "fire"
                    | "getcurrentdestructionstage"
                    | "cleardestruction"
                    | "setdestroyed"
                    | "isdestroyed"
                    | "getbribeamount"
                    | "getgoldamount"
                    | "setbribed"
                    | "isbribed"
                    | "setintimidated"
                    | "modfavorpoints"
                    | "modfavorpointswithglobal"
                    | "makeplayerfriend"
                    | "isintimidated"
                    | "changeanimarchetype"
                    | "setscanned"
                    | "applyhavokimpulse"
                    | "istrue"
                    | "isscanned"
                    | "addtomapscanned"
                    | "setmarkervisibleonstarmap"
                    | "playanimation"
                    | "registerforanimationevent"
                    | "unregisterforanimationevent"
                    | "setanimationvariablefloat"
                    | "setanimationvariablebool"
                    | "setanimationvariableint"
                    | "getanimationvariablefloat"
                    | "getanimationvariablebool"
                    | "getanimationvariableint"
                    | "play"
                    | "enablelinkchain"
                    | "disablelinkchain"
                    | "geteditorlocation"
                    | "getcurrentpackage"
                    | "getfactionreaction"
                    | "getlinkedrefchain"
                    | "getlinkedrefchildren"
                    | "addform"
                    | "removeaddedform"
                    | "revert"
                    | "countlinkedrefchain"
                    | "isindialoguewithplayer"
                    | "dropobject"
                    | "isloaded"
                    | "setactorowner"
                    | "setfactionowner"
                    | "setcrimegold"
                    | "setcrimegoldviolent"
                    | "modcrimegold"
                    | "getcrimegold"
                    | "getcrimegoldviolent"
                    | "getcrimegoldnonviolent"
                    | "getsex"
                    | "getvoicetype"
                    | "isinlocation"
                    | "isfurnitureinuse"
                    | "isstarting"
                    | "isstopping"
                    | "getcrimefaction"
            );
        if !reads {
            return Ok(None);
        }
        let owner = self.eval(frame, &object.node)?;
        Ok(match (&owner, name.as_str()) {
            (Value::Alias { quest, alias }, getter) if ALIAS_GETTERS.contains(&getter) => Some(
                self.deferred
                    .scene_actor_bindings
                    .resolve(QuestFormId(*quest), i32::from(*alias))
                    .and_then(|entity| self.entity_form(entity))
                    .map_or(Value::None, Value::Form),
            ),
            (Value::Alias { quest, .. }, "getowningquest") => Some(Value::Form(*quest)),
            // `ReferenceAlias.TryToGetCurrentLocation()` (Starfield): the
            // `GetCurrentLocation` of the reference filling the alias, None
            // while it is empty.
            (Value::Alias { quest, alias }, "trytogetcurrentlocation") => Some(
                self.deferred
                    .scene_actor_bindings
                    .resolve(QuestFormId(*quest), i32::from(*alias))
                    .and_then(|entity| crate::location::current_location(self.world, entity))
                    .map_or(Value::None, Value::Form),
            ),
            // CK *FormList Script*: the list's own forms (see [`crate::form_lists`]).
            (Value::Form(list), "getsize" | "getat" | "hasform" | "find")
                if self.form_list(*list).is_some() =>
            {
                let forms = self.form_list(*list).unwrap_or_default();
                let values = self.eval_args(frame, args)?;
                let form = |value: Option<&Value>| match value {
                    Some(Value::Form(form)) => Some(*form),
                    _ => None,
                };
                Some(match name.as_str() {
                    "getsize" => Value::Int(forms.len() as i32),
                    "getat" => values
                        .first()
                        .and_then(Value::int)
                        .and_then(|index| usize::try_from(index).ok())
                        .and_then(|index| forms.get(index).copied())
                        .map_or(Value::None, Value::Form),
                    "hasform" => Value::Bool(form(values.first()).is_some_and(|form| forms.contains(&form))),
                    _ => Value::Int(
                        form(values.first())
                            .and_then(|form| forms.iter().position(|&member| member == form))
                            .map_or(-1, |index| index as i32),
                    ),
                })
            }
            // CK *SetNoFavorAllowed*: whether the player can ask a follower to
            // operate this object (default True). The state waits for the
            // follower favor system.
            (Value::Form(reference), "setnofavorallowed") if args.len() <= 1 => {
                let values = self.eval_args(frame, args)?;
                let no_favor = values.first().is_none_or(Value::truthy);
                self.deferred
                    .reference_flag_changes
                    .push(crate::ReferenceFlagChange::NoFavor { reference: *reference, no_favor });
                Some(Value::None)
            }
            // CK *IgnoreFriendlyHits* / *SetCanDoCommand - Actor*: boolean flags
            // (default True) recorded for the systems that will read them.
            (Value::Form(reference), "ignorefriendlyhits" | "setcandocommand") if args.len() <= 1 => {
                let values = self.eval_args(frame, args)?;
                let on = values.first().is_none_or(Value::truthy);
                let flag = if name == "ignorefriendlyhits" {
                    crate::ReferenceFlag::IgnoreFriendlyHits
                } else {
                    crate::ReferenceFlag::CanDoCommand
                };
                self.deferred
                    .reference_flag_changes
                    .push(crate::ReferenceFlagChange::Flag { reference: *reference, flag, on });
                Some(Value::None)
            }
            // CK *SetActorCause*: the actor responsible for this object; None
            // clears it.
            (Value::Form(reference), "setactorcause") if args.len() == 1 => {
                let actor = match self.eval(frame, &args[0].value.node)? {
                    Value::Form(actor) if actor != 0 => Some(actor),
                    _ => None,
                };
                self.deferred
                    .reference_flag_changes
                    .push(crate::ReferenceFlagChange::ActorCause { reference: *reference, actor });
                Some(Value::None)
            }
            // CK *GetScale*: "the current scale of the reference".
            (Value::Form(reference), "getscale") if args.is_empty() => {
                let entity = if *reference == PLAYER_REF {
                    crate::papyrus_demo::PapyrusPlayerEntity::get(self.world)
                } else {
                    self.entity_of(*reference)
                };
                Some(Value::Float(
                    entity
                        .and_then(|entity| self.world.get::<byroredux_core::ecs::Transform>(entity).map(|t| t.scale))
                        .unwrap_or(1.0),
                ))
            }
            // CK *GetRace - Actor*: "the Race the actor is" (its record's).
            (Value::Form(actor), "getrace") if args.is_empty() => {
                let entity = if *actor == PLAYER_REF {
                    crate::papyrus_demo::PapyrusPlayerEntity::get(self.world)
                } else {
                    self.entity_of(*actor)
                };
                Some(
                    entity
                        .and_then(|entity| self.world.get::<byroredux_core::character::Background>(entity).map(|b| b.race_form_id))
                        .filter(|race| *race != 0)
                        .map_or(Value::None, Value::Form),
                )
            }
            // CK *EnableLinkChain* / *DisableLinkChain*: the function is script
            // code over `GetLinkedRef` — follow the keyword's links from this
            // reference, enabling (or disabling) each. A chain that loops back
            // ends there.
            (Value::Form(start), "enablelinkchain" | "disablelinkchain") if args.len() <= 2 => {
                let values = self.eval_args(frame, args)?;
                let keyword = match values.first() {
                    Some(Value::Form(keyword)) => *keyword,
                    _ => 0,
                };
                let enabled = name == "enablelinkchain";
                let mut seen = std::collections::HashSet::new();
                let mut current = self.linked_ref(*start, keyword);
                while let Some(link) = current {
                    if !seen.insert(link) || seen.len() > MAX_LINK_CHAIN_HOPS {
                        break;
                    }
                    self.deferred.reference_enable_changes.push((link, enabled));
                    current = self.linked_ref(link, keyword);
                }
                Some(Value::None)
            }
            // CK *GetFactionReaction - Actor*: 0 Neutral, 1 Enemy, 2 Ally, 3 Friend,
            // by the engine-policy combination of the two actors' factions.
            (Value::Form(actor), "getfactionreaction") if args.len() == 1 => {
                let other = match self.eval(frame, &args[0].value.node)? {
                    Value::Form(other) => other,
                    _ => return stop("GetFactionReaction without an actor"),
                };
                let reaction = self
                    .entity_of(*actor)
                    .zip(self.entity_of(other))
                    .and_then(|(actor, other)| crate::faction_reaction(self.world, actor, other));
                match reaction {
                    Some(reaction) => Some(Value::Int(reaction)),
                    None => return stop("GetFactionReaction on an actor that is not loaded"),
                }
            }
            // CK *GetCurrentPackage - Actor*: the package the actor runs now,
            // from the AI's transition reports; None while it runs none.
            (Value::Form(actor), "getcurrentpackage") if args.is_empty() => Some(
                self.entity_of(*actor)
                    .and_then(|entity| {
                        self.world
                            .try_resource::<super::ActivePackages>()
                            .and_then(|active| active.0.get(&entity).copied())
                    })
                    .map_or(Value::None, Value::Form),
            ),
            // CK *GetEditorLocation*: the reference's editor location.
            (Value::Form(reference), "geteditorlocation") if args.is_empty() => Some(
                self.entity_of(*reference)
                    .and_then(|entity| crate::location::editor_location(self.world, entity))
                    .map_or(Value::None, Value::Form),
            ),
            // CK *Play - Sound*: "Plays the sound from the location of the
            // specified object reference." The call returns an instance id: 1 when
            // the sound was queued, 0 when it could not be (the check's note).
            (Value::Form(sound), "play") if args.len() == 1 => {
                let source = match self.eval(frame, &args[0].value.node)? {
                    Value::Form(form) if form != 0 => Some(form),
                    Value::Alias { quest, alias } => self
                        .deferred
                        .scene_actor_bindings
                        .resolve(QuestFormId(quest), i32::from(alias))
                        .and_then(|entity| self.entity_form(entity)),
                    _ => None,
                };
                if let Some(source) = source {
                    self.deferred.sound_plays.push(crate::SoundPlay { sound: *sound, source });
                }
                Some(Value::Int(i32::from(source.is_some())))
            }
            // CK *GetFactionOwner* / *GetActorOwner*: the reference's explicit
            // owner only (not its container's or cell's), None when the owner
            // is of the other kind.
            (Value::Form(reference), "getfactionowner" | "getactorowner") if args.is_empty() => {
                let pending = self.deferred.owner_changes.iter().rev().find(|(who, _)| who == reference).map(|&(_, owner)| owner);
                let owner = self.entity_of(*reference).and_then(|entity| {
                    if name == "getfactionowner" {
                        crate::faction_owner_with(self.world, entity, pending)
                    } else {
                        crate::actor_owner_with(self.world, entity, pending)
                    }
                });
                Some(owner.map_or(Value::None, Value::Form))
            }
            // CK *SetActorOwner* / *SetFactionOwner - ObjectReference*: the one
            // owner slot is replaced; None clears it.
            (Value::Form(reference), "setactorowner" | "setfactionowner") if args.len() == 1 => {
                let owner = match self.eval(frame, &args[0].value.node)? {
                    Value::Form(owner) if owner != 0 => owner,
                    Value::None => 0,
                    _ => return stop("SetActorOwner / SetFactionOwner without a form"),
                };
                self.deferred.owner_changes.push((*reference, owner));
                Some(Value::None)
            }
            // CK *Faction* crime gold natives (see [`crate::crime_gold`]).
            (Value::Form(faction), "setcrimegold" | "setcrimegoldviolent" | "modcrimegold" | "getcrimegold" | "getcrimegoldviolent" | "getcrimegoldnonviolent")
                if crate::is_faction(self.world, *faction) =>
            {
                let values = self.eval_args(frame, args)?;
                let amount = values.first().and_then(Value::int).unwrap_or(0);
                match name.as_str() {
                    "setcrimegold" | "setcrimegoldviolent" if args.len() == 1 => {
                        self.deferred.crime_gold_changes.push(crate::CrimeGoldChange::Set {
                            faction: *faction,
                            violent: name == "setcrimegoldviolent",
                            amount,
                        });
                        Some(Value::None)
                    }
                    "modcrimegold" if (1..=2).contains(&args.len()) => {
                        let violent = values.get(1).is_some_and(Value::truthy);
                        self.deferred.crime_gold_changes.push(crate::CrimeGoldChange::Mod { faction: *faction, violent, amount });
                        Some(Value::None)
                    }
                    "getcrimegold" | "getcrimegoldviolent" | "getcrimegoldnonviolent" if args.is_empty() => {
                        let (nonviolent, violent) = crate::crime_gold_with(self.world, *faction, &self.deferred.crime_gold_changes);
                        Some(Value::Int(match name.as_str() {
                            "getcrimegoldviolent" => violent,
                            "getcrimegoldnonviolent" => nonviolent,
                            _ => nonviolent.saturating_add(violent),
                        }))
                    }
                    _ => return stop(format!("{name} as written")),
                }
            }
            // Animation-graph natives (CK *PlayAnimation*, *RegisterForAnimationEvent*,
            // *SetAnimationVariable\**): see [`crate::animation_graph`]. `PlayAnimation`
            // sends the event to the object's behavior graph; the registrations
            // deliver `OnAnimationEvent` (see [`super::animation_event_system`]) and
            // a variable set has no graph variable to write.
            (Value::Form(reference), "playanimation") if args.len() == 1 => {
                let values = self.eval_args(frame, args)?;
                let event = values[0].text();
                let accepted = self
                    .entity_of(*reference)
                    .is_some_and(|entity| crate::animation_graph::play_animation(self.world, entity, &event));
                Some(Value::Bool(accepted))
            }
            // CK *RegisterForAnimationEvent - Form*: true "if it was successfully able
            // to register", false when "the animation graph may not be loaded". A
            // reference has a graph here when its prop attached one; the registrant
            // is the script that makes the call (a call on another form is not a
            // registration this script can own), and the event name is never
            // checked: "the game will allow you to register ANY animation event".
            (Value::Form(registrant), "registerforanimationevent") if args.len() == 2 => {
                let values = self.eval_args(frame, args)?;
                let sender = match values.first() {
                    Some(Value::Form(sender)) if *sender != 0 => Some(*sender),
                    _ => None,
                };
                let registered = sender.filter(|_| *registrant == frame.owner.0).filter(|sender| {
                    self.entity_of(*sender)
                        .is_some_and(|entity| crate::animation_graph::has_graph(self.world, entity))
                });
                match registered {
                    Some(sender) => {
                        let script = frame.script.name.node.0.clone();
                        let event = values[1].text();
                        self.deferred.animation_event_requests.push((
                            frame.owner.0,
                            script,
                            super::AnimationEventRequest::Register { sender, event },
                        ));
                        Some(Value::Bool(true))
                    }
                    None => Some(Value::Bool(false)),
                }
            }
            (Value::Form(registrant), "unregisterforanimationevent") if args.len() == 2 => {
                let values = self.eval_args(frame, args)?;
                if let (true, Some(Value::Form(sender))) = (*registrant == frame.owner.0, values.first()) {
                    let script = frame.script.name.node.0.clone();
                    let event = values[1].text();
                    self.deferred.animation_event_requests.push((
                        frame.owner.0,
                        script,
                        super::AnimationEventRequest::Unregister { sender: *sender, event },
                    ));
                }
                Some(Value::None)
            }
            // CK *SetAnimationVariable\** / *GetAnimationVariable\**: "the value of a variable
            // on the reference's animation graph". A reference with no graph, or a graph
            // without that variable, takes the write for nothing and reads as zero.
            (Value::Form(reference), "setanimationvariablefloat" | "setanimationvariablebool" | "setanimationvariableint")
                if args.len() == 2 =>
            {
                let values = self.eval_args(frame, args)?;
                let value = match &values[1] {
                    Value::Bool(flag) => f32::from(*flag),
                    Value::Int(whole) => *whole as f32,
                    Value::Float(real) => *real,
                    _ => return stop(format!("{name} with a non-number")),
                };
                if let Some(entity) = self.entity_of(*reference) {
                    crate::animation_graph::set_variable_value(self.world, entity, &values[0].text(), value);
                }
                Some(Value::None)
            }
            (Value::Form(reference), "getanimationvariablefloat" | "getanimationvariablebool" | "getanimationvariableint")
                if args.len() == 1 =>
            {
                let values = self.eval_args(frame, args)?;
                let value = self
                    .entity_of(*reference)
                    .and_then(|entity| crate::animation_graph::variable_value(self.world, entity, &values[0].text()))
                    .unwrap_or(0.0);
                Some(match name.as_str() {
                    "getanimationvariablebool" => Value::Bool(value != 0.0),
                    "getanimationvariableint" => Value::Int(value as i32),
                    _ => Value::Float(value),
                })
            }
            // CK *GetGoldAmount - Actor*: "the amount of gold on this actor's
            // person" — the count of Skyrim's `Gold001` in its inventory.
            (Value::Form(actor), "getgoldamount") if args.is_empty() => {
                let skyrim = self
                    .world
                    .try_resource::<crate::ScriptGame>()
                    .is_some_and(|game| game.0 == byroredux_plugin::esm::reader::GameKind::Skyrim);
                if !skyrim {
                    return stop("GetGoldAmount outside Skyrim");
                }
                let gold: u32 = self
                    .entity_of(*actor)
                    .and_then(|entity| self.world.get::<byroredux_core::ecs::components::Inventory>(entity).map(|inventory| {
                        inventory
                            .items
                            .iter()
                            .filter(|stack| stack.base_form_id == SKYRIM_GOLD)
                            .map(|stack| stack.count)
                            .sum()
                    }))
                    .unwrap_or(0);
                Some(Value::Int(gold as i32))
            }
            // CK *GetBribeAmount - Actor*: the gold to bribe this actor (the UESP
            // *Speech* formula; see `bribe`).
            (Value::Form(actor), "getbribeamount") if args.is_empty() => {
                match self.entity_of(*actor).and_then(|entity| crate::bribe_amount(self.world, entity)) {
                    Some(amount) => Some(Value::Int(amount)),
                    None => return stop("GetBribeAmount without bribery data or an actor level"),
                }
            }
            // CK *CalculateEncounterLevel - ObjectReference*: the level from the
            // player's level, the reference's encounter zone and the difficulty
            // (default 4, None); see `encounter_level`.
            (Value::Form(reference), "calculateencounterlevel") if args.len() <= 1 => {
                let difficulty = match args.first() {
                    None => 4,
                    Some(arg) => match self.eval(frame, &arg.value.node)? {
                        Value::Int(difficulty) => difficulty,
                        _ => return stop("CalculateEncounterLevel with a non-integer difficulty"),
                    },
                };
                let level = self
                    .entity_of(*reference)
                    .and_then(|entity| crate::calculate_encounter_level(self.world, entity, difficulty));
                match level {
                    Some(level) => Some(Value::Int(level as i32)),
                    None => return stop("CalculateEncounterLevel without the player's level or a difficulty multiplier"),
                }
            }
            // CK *GetTriggerObjectCount*: "the number of objects currently
            // inside this trigger volume".
            (Value::Form(trigger), "gettriggerobjectcount") if args.is_empty() => Some(Value::Int(
                self.entity_of(*trigger)
                    .map_or(0, |entity| crate::trigger_object_count(self.world, entity)) as i32,
            )),
            // CK *GetClassification - Weather*: -1 none, 0 Pleasant, 1 Cloudy,
            // 2 Rainy, 3 Snow.
            (Value::Form(weather), "getclassification") if args.is_empty() => Some(Value::Int(
                self.world
                    .try_resource::<crate::WeatherClassifications>()
                    .map_or(-1, |table| table.of(*weather)),
            )),
            // CK *IsChild - Location*: "Returns true if the other location is a
            // child of this one. If they are the same location, the function
            // returns false."
            (Value::Form(this), "ischild") if args.len() == 1 => {
                let other = match self.eval(frame, &args[0].value.node)? {
                    Value::Form(other) => other,
                    _ => 0,
                };
                let child = other != 0
                    && other != *this
                    && self
                        .world
                        .try_resource::<crate::location::LocationTable>()
                        .is_some_and(|table| table.is_within(other, *this));
                Some(Value::Bool(child))
            }
            // `LocationAlias.GetLocation()` — the location runtime's fill.
            (Value::Alias { quest, alias }, "getlocation") => Some(
                self.world
                    .try_resource::<crate::LocationAliasBindings>()
                    .and_then(|bindings| bindings.resolve(QuestFormId(*quest), i32::from(*alias)))
                    .map_or(Value::None, Value::Form),
            ),
            (Value::Form(form), "getcurrentlocation") => Some(
                self.entity_of(*form)
                    .and_then(|entity| crate::location::current_location(self.world, entity))
                    .map_or(Value::None, Value::Form),
            ),
            // CK *GetDialogueTarget*: "the actor this actor is currently in
            // dialogue with" — the player for the line's speaker, the speaker
            // for the player.
            (Value::Form(form), "getdialoguetarget") => {
                let speaker = self.deferred.speaker.and_then(|entity| self.entity_form(entity));
                Some(match speaker {
                    Some(speaker) if speaker == *form => Value::Form(PLAYER_REF),
                    Some(speaker) if *form == PLAYER_REF => Value::Form(speaker),
                    _ => Value::None,
                })
            }
            (Value::Form(_), "getowningquest") => Some(Value::Form(frame.owner.0)),
            (Value::Form(at), "placeatme") => {
                let values = self.eval_args(frame, args)?;
                Some(self.place_at_me(*at, &values)?)
            }
            (Value::Form(form), "getlinkedref") => {
                let keyword = match args.first() {
                    Some(arg) => match self.eval(frame, &arg.value.node)? {
                        Value::Form(keyword) => keyword,
                        _ => 0,
                    },
                    None => 0,
                };
                Some(self.linked_ref(*form, keyword).map_or(Value::None, Value::Form))
            }
            // CK *FormList Script*: `AddForm` "adds the given form to the form
            // list", `RemoveAddedForm` removes "only forms added ... via calls
            // to AddForm", `Revert` "removes all script added forms".
            (Value::Form(list), "addform" | "removeaddedform" | "revert")
                if self.form_list(*list).is_some() && args.len() == usize::from(name != "revert") =>
            {
                let change = if name == "revert" {
                    crate::FormListChange::Revert { list: *list }
                } else {
                    let form = match self.eval(frame, &args[0].value.node)? {
                        Value::Form(form) if form != 0 => form,
                        _ => return stop(format!("{name} without a form")),
                    };
                    if name == "addform" {
                        crate::FormListChange::Add { list: *list, form }
                    } else {
                        crate::FormListChange::RemoveAdded { list: *list, form }
                    }
                };
                self.deferred.form_list_changes.push(change);
                Some(Value::None)
            }
            // CK *GetLinkedRefChain*: the references the object is linked to
            // under the keyword, and whatever those are linked to, and so on;
            // "Will stop if it finds a loop", or at `iMaxExpectedLinkedRefs`
            // (default 100).
            (Value::Form(form), "getlinkedrefchain") if args.len() <= 2 => {
                let values = self.eval_args(frame, args)?;
                let keyword = match values.first() {
                    Some(Value::Form(keyword)) => *keyword,
                    _ => 0,
                };
                let limit = values
                    .get(1)
                    .and_then(Value::number)
                    .map_or(MAX_LINK_CHAIN_HOPS as f32, |max| max.max(0.0))
                    .min(MAX_LINK_CHAIN_HOPS as f32) as usize;
                let mut seen = vec![*form];
                let mut current = *form;
                while seen.len() - 1 < limit {
                    let Some(next) = self.linked_ref(current, keyword) else {
                        break;
                    };
                    if seen.contains(&next) {
                        break;
                    }
                    seen.push(next);
                    current = next;
                }
                let chain = seen.into_iter().skip(1).map(Value::Form).collect();
                Some(Value::Array(Rc::new(RefCell::new(chain))))
            }
            // CK *IsInDialogueWithPlayer*: "Is this actor or talking activator
            // currently talking to the player?" — the NPC whose response
            // surface is open.
            (Value::Form(actor), "isindialoguewithplayer") if args.is_empty() => {
                let partner = self
                    .world
                    .try_resource::<crate::DialogueSurfaceState>()
                    .and_then(|surface| surface.npc);
                Some(Value::Bool(partner.is_some() && partner == self.entity_of(*actor)))
            }
            // CK *DropObject - ObjectReference*: "Drops the specified object from
            // this object's inventory" and returns "the object that was
            // dropped". Up to `aiCount` (default 1) of the item leave the
            // inventory and appear in the world at the owner, one reference
            // each (the CK stacks a count above one into a single reference,
            // which created references have no count for); an item the
            // inventory does not hold drops nothing and reads None.
            (Value::Form(owner), "dropobject") if (1..=2).contains(&args.len()) => {
                let values = self.eval_args(frame, args)?;
                let Some(Value::Form(item)) = values.first() else {
                    return stop("DropObject without a form");
                };
                let wanted = values.get(1).and_then(Value::int).unwrap_or(1).clamp(0, MAX_PLACED_PER_CALL);
                let Some(entity) = self.entity_of(*owner) else {
                    return stop("DropObject on a reference that is not loaded");
                };
                let held: u32 = self
                    .world
                    .query::<byroredux_core::ecs::components::Inventory>()
                    .and_then(|inventories| {
                        inventories.get(entity).map(|inventory| {
                            inventory.items.iter().filter(|stack| stack.base_form_id == *item).map(|stack| stack.count).sum()
                        })
                    })
                    .unwrap_or(0);
                let take = held.min(wanted as u32);
                if take == 0 {
                    return Ok(Some(Value::None));
                }
                crate::fragment::remove_items(self.world, entity, *item, take, None);
                Some(self.place_at_me(*owner, &[Value::Form(*item), Value::Int(take as i32)])?)
            }
            // CK *IsInLocation - ObjectReference*, whose page gives its script:
            // false without a current location, else the location asked about or
            // one of its descendants.
            (Value::Form(reference), "isinlocation") if args.len() == 1 => {
                let Value::Form(asked) = self.eval(frame, &args[0].value.node)? else {
                    return stop("IsInLocation without a location");
                };
                let entity = if *reference == PLAYER_REF {
                    crate::papyrus_demo::PapyrusPlayerEntity::get(self.world)
                } else {
                    self.entity_of(*reference)
                };
                let current = entity.and_then(|entity| crate::location::current_location(self.world, entity));
                let inside = match current {
                    None => false,
                    Some(current) => {
                        current == asked
                            || self
                                .world
                                .try_resource::<crate::location::LocationTable>()
                                .is_some_and(|table| table.is_within(current, asked))
                    }
                };
                Some(Value::Bool(inside))
            }
            // CK *GetSex - ActorBase*: "-1: None, 0: Male, 1: Female", from the
            // base's female flag. A form that is no actor base stops the run.
            (Value::Form(base), "getsex") if args.is_empty() => {
                match self.world.try_resource::<crate::ActorBaseTraits>().and_then(|traits| traits.get(*base)) {
                    Some(base_trait) => Some(Value::Int(i32::from(base_trait.female))),
                    None => return stop("GetSex on a form that is not an actor base"),
                }
            }
            // CK *GetVoiceType - ObjectReference*: the voice type of the actor.
            (Value::Form(actor), "getvoicetype") if args.is_empty() => {
                let voice = self.entity_of(*actor).and_then(|entity| crate::voice_type(self.world, entity));
                Some(voice.map_or(Value::None, Value::Form))
            }
            // CK *IsLoaded - Location*: whether the location is loaded.
            (Value::Form(location), "isloaded") if args.is_empty() => {
                match crate::location::location_loaded(self.world, *location) {
                    Some(loaded) => Some(Value::Bool(loaded)),
                    None => return stop("IsLoaded on a form that is not a location"),
                }
            }
            // CK *IsFurnitureInUse*: "if any marker on the furniture is in use
            // (or reserved, if we aren't ignoring reservations)". The occupants
            // here are the actors seated on it and the seats the sandbox AI
            // has claimed; nothing else uses furniture.
            (Value::Form(furniture), "isfurnitureinuse") if args.len() <= 1 => {
                let ignore_reserved = match args.first() {
                    Some(arg) => self.eval(frame, &arg.value.node)?.truthy(),
                    None => false,
                };
                let Some(entity) = self.entity_of(*furniture) else {
                    return stop("IsFurnitureInUse on a reference that is not loaded");
                };
                let seated = self.world.query::<byroredux_core::ecs::components::Seated>().is_some_and(|seated| {
                    seated.iter().any(|(_, seat)| seat.furniture == entity)
                });
                let reserved = !ignore_reserved
                    && self
                        .world
                        .try_resource::<byroredux_core::ecs::components::SeatReservations>()
                        .is_some_and(|seats| seats.0.keys().any(|(held, _)| *held == entity));
                Some(Value::Bool(seated || reserved))
            }
            // CK *GetCrimeFaction - Actor*: the faction this actor reports its
            // crimes to — a script's `SetCrimeFaction`, else the base's `CRIF`.
            (Value::Form(actor), "getcrimefaction") if args.is_empty() => {
                let crime = self.entity_of(*actor).and_then(|entity| {
                    let reference = if *actor == PLAYER_REF { PLAYER_REF } else { self.entity_form(entity)? };
                    // A `SetCrimeFaction` earlier in this run is seen.
                    match self.deferred.crime_faction_changes.iter().rev().find(|(who, _)| *who == reference) {
                        Some(&(_, faction)) => (faction != 0).then_some(faction),
                        None => crate::crime_faction(self.world, entity, reference),
                    }
                });
                Some(crime.map_or(Value::None, Value::Form))
            }
            // Destructible objects (see [`crate::destruction`]): CK *DamageObject*
            // ("damages this object, which may fire OnDestructionStageChanged"),
            // *GetCurrentDestructionStage* (-1 undamaged), *ClearDestruction* (no
            // event), *SetDestroyed* ("sets or clears this object's destroyed
            // flag", default True) and *IsDestroyed*.
            // CK *Fire - Weapon*: fires the weapon from the source with the ammo (None:
            // the reference's current ammo); the host launches the projectile (see
            // [`crate::projectile_requests`]).
            // CK *CreateDetectionEvent*: a detection event at this object's location
            // for `akOwner` at `aiSoundLevel` (default 0); recorded for a hearing
            // system the engine does not have (see [`crate::detection_events`]).
            // CK *HasSpell - Actor*: whether the actor "knows" the spell or shout (not
            // whether one is acting on it) - the actor's spell list, which holds its
            // race's and what `AddSpell` gave it.
            (Value::Form(actor), "hasspell") if args.len() == 1 => {
                let values = self.eval_args(frame, args)?;
                let spell = match values.first() {
                    Some(Value::Form(spell)) => *spell,
                    _ => 0,
                };
                let knows = spell != 0
                    && self
                        .entity_of(*actor)
                        .and_then(|entity| self.world.get::<crate::SpellList>(entity).map(|list| list.0.contains(&spell)))
                        .unwrap_or(false);
                Some(Value::Bool(knows))
            }
            // CK *ProcessTrapHit*: "tells this reference to handle getting hit by a
            // trap, dealing the specified amount of damage and possibly applying a
            // pushback force" (`afStagger` 0 to 1). The damage lands as the target's
            // `HitEvent`; the pushback distance and the stagger have no model.
            (Value::Form(target), "processtraphit") if args.len() == 11 => {
                let values = self.eval_args(frame, args)?;
                let trap = match values.first() {
                    Some(Value::Form(trap)) if *trap != 0 => *trap,
                    _ => return stop("ProcessTrapHit without a trap reference"),
                };
                let damage = values.get(1).and_then(Value::number).unwrap_or(0.0);
                if let (Some(target), Some(trap)) = (self.entity_of(*target), self.entity_of(trap)) {
                    self.deferred.trap_hits.push((target, trap, damage));
                }
                Some(Value::None)
            }
            (Value::Form(source), "createdetectionevent") if (1..=2).contains(&args.len()) => {
                let values = self.eval_args(frame, args)?;
                let owner = match values.first() {
                    Some(Value::Form(owner)) => *owner,
                    _ => 0,
                };
                let sound_level = values.get(1).and_then(Value::int).unwrap_or(0);
                if let Some(mut pending) = self.world.try_resource_mut::<crate::PendingDetectionEvents>() {
                    pending.push(crate::DetectionEvent { source: *source, owner, sound_level });
                }
                Some(Value::None)
            }
            (Value::Form(weapon), "fire") if (1..=2).contains(&args.len()) => {
                let values = self.eval_args(frame, args)?;
                // Nothing to fire from (DartTrap asks for the Nth link of a chain, which
                // can run past its end): the call fails and the script goes on.
                let source = match values.first() {
                    Some(Value::Form(source)) if *source != 0 => *source,
                    _ => {
                        log::debug!("Papyrus: Weapon.Fire with no source reference, ignored");
                        return Ok(Some(Value::None));
                    }
                };
                let ammo = match values.get(1) {
                    Some(Value::Form(ammo)) => *ammo,
                    _ => 0,
                };
                if let Some(mut pending) = self.world.try_resource_mut::<crate::PendingProjectileLaunches>() {
                    pending.0.push(crate::ProjectileLaunch { source, weapon: *weapon, ammo });
                }
                Some(Value::None)
            }
            (Value::Form(reference), "damageobject") if args.len() == 1 => {
                let values = self.eval_args(frame, args)?;
                let amount = values.first().and_then(Value::number).unwrap_or(0.0);
                if let Some(entity) = self.entity_of(*reference) {
                    if let Some(outcome) = crate::destruction::damage_object(self.world, entity, *reference, amount) {
                        if outcome.new_stage != outcome.old_stage {
                            super::raise_reference_event(
                                self.world,
                                entity,
                                "OnDestructionStageChanged",
                                vec![VmStored::Int(outcome.old_stage), VmStored::Int(outcome.new_stage)],
                            );
                        }
                        if outcome.disable {
                            self.deferred.reference_enable_changes.push((*reference, false));
                        }
                    }
                }
                Some(Value::None)
            }
            (Value::Form(reference), "getcurrentdestructionstage") if args.is_empty() => {
                let stage = self
                    .entity_of(*reference)
                    .map_or(-1, |entity| crate::destruction::destruction_stage(self.world, entity, *reference));
                Some(Value::Int(stage))
            }
            (Value::Form(reference), "cleardestruction") if args.is_empty() => {
                crate::destruction::clear_destruction(self.world, *reference);
                Some(Value::None)
            }
            (Value::Form(reference), "setdestroyed") if args.len() <= 1 => {
                let values = self.eval_args(frame, args)?;
                crate::destruction::set_destroyed(self.world, *reference, values.first().is_none_or(Value::truthy));
                Some(Value::None)
            }
            (Value::Form(reference), "isdestroyed") if args.is_empty() => {
                Some(Value::Bool(crate::destruction::is_destroyed(self.world, *reference)))
            }
            // CK *SetBribed* / *SetIntimidated* ("flags this actor as bribed /
            // intimidated by the player", default True) and *IsBribed* /
            // *IsIntimidated*; see [`crate::speechcraft`].
            (Value::Form(actor), "setbribed" | "setintimidated") if args.len() <= 1 => {
                let values = self.eval_args(frame, args)?;
                let flagged = values.first().is_none_or(Value::truthy);
                let Some(reference) = self.actor_reference(*actor) else {
                    return stop(format!("{name} on an actor that is not loaded"));
                };
                let outcome = if name == "setbribed" {
                    crate::SpeechcraftOutcome::Bribed
                } else {
                    crate::SpeechcraftOutcome::Intimidated
                };
                self.deferred.speechcraft_changes.push((reference, outcome, flagged));
                Some(Value::None)
            }
            // Skyrim.esm's own `Actor.pex` defines these in Papyrus (not natives):
            // `ModFavorPoints(iFavorPoints)` is `If iFavorPoints > 0: MakePlayerFriend()`, and
            // `MakePlayerFriend()` (CK *MakePlayerFriend*) is "if the actor's base is unique
            // and its relationship rank with the player is 0, set it to 1".
            (Value::Form(actor), "modfavorpoints" | "modfavorpointswithglobal" | "makeplayerfriend")
                if args.len() == usize::from(name != "makeplayerfriend") =>
            {
                let values = self.eval_args(frame, args)?;
                let friend = match (name.as_str(), values.first()) {
                    // `ModFavorPointsWithGlobal(g)` is `ModFavorPoints(g.GetValueInt())`.
                    ("modfavorpointswithglobal", Some(Value::Form(global))) => self
                        .world
                        .try_resource::<crate::Globals>()
                        .and_then(|globals| globals.get(*global))
                        .is_some_and(|points| points as i32 > 0),
                    ("modfavorpointswithglobal", _) => return stop("ModFavorPointsWithGlobal without a global".to_owned()),
                    (_, Some(points)) => points.number().is_some_and(|points| points as i32 > 0),
                    (_, None) => true,
                };
                if friend {
                    let Some(entity) = self.entity_of(*actor) else {
                        return stop(format!("{name} on an actor that is not loaded"));
                    };
                    let Some(base) = crate::relationships::actor_base(self.world, entity) else {
                        return stop(format!("{name} on a reference with no actor base"));
                    };
                    let unique = self
                        .world
                        .try_resource::<crate::ActorBaseTraits>()
                        .and_then(|traits| traits.get(base))
                        .is_some_and(|base| base.unique);
                    let pair = crate::relationships::pair(base, crate::relationships::PLAYER_BASE);
                    let rank = match self.deferred.relationship_changes.iter().rev().find(|(changed, _)| *changed == pair) {
                        Some(&(_, rank)) => rank,
                        None => crate::relationships::base_relationship_rank(self.world, base, crate::relationships::PLAYER_BASE),
                    };
                    if unique && rank == 0 {
                        self.deferred.relationship_changes.push((pair, 1));
                    }
                }
                Some(Value::None)
            }
            (Value::Form(actor), "isbribed" | "isintimidated") if args.is_empty() => {
                let Some(reference) = self.actor_reference(*actor) else {
                    return stop(format!("{name} on an actor that is not loaded"));
                };
                let outcome = if name == "isbribed" {
                    crate::SpeechcraftOutcome::Bribed
                } else {
                    crate::SpeechcraftOutcome::Intimidated
                };
                // A `Set…` earlier in this run is seen.
                let flagged = match self
                    .deferred
                    .speechcraft_changes
                    .iter()
                    .rev()
                    .find(|(who, kind, _)| *who == reference && *kind == outcome)
                {
                    Some(&(_, _, flagged)) => flagged,
                    None => self
                        .world
                        .try_resource::<crate::SpeechcraftOutcomes>()
                        .is_some_and(|outcomes| outcomes.is(reference, outcome)),
                };
                Some(Value::Bool(flagged))
            }
            // CK *ApplyHavokImpulse - ObjectReference*: the direction normalized,
            // the magnitude in the game's own unit (see [`crate::havok_impulse`]).
            // The binary applies the queued request to the reference's body.
            (Value::Form(reference), "applyhavokimpulse") if args.len() == 4 => {
                let values = self.eval_args(frame, args)?;
                let mut numbers = [0.0_f32; 4];
                for (slot, value) in numbers.iter_mut().zip(&values) {
                    *slot = match value.number() {
                        Some(number) => number,
                        None => return stop("ApplyHavokImpulse with a non-numeric argument"),
                    };
                }
                if self.world.try_resource::<crate::HavokImpulseRules>().is_none() {
                    return stop("ApplyHavokImpulse without the game's impulse settings");
                }
                if let Some(entity) = self.entity_of(*reference) {
                    // A reference with no 3D, or a zero-length vector, is the
                    // CK's documented failure: a warning and no impulse.
                    crate::request_havok_impulse(self.world, entity, numbers[0], numbers[1], numbers[2], numbers[3]);
                }
                Some(Value::None)
            }
            // Starfield `ConditionForm.IsTrue(akRefObject, akTargetObject)`: the
            // form's conditions with the first reference as subject and the
            // second as target; see [`crate::condition_form`].
            (Value::Form(form), "istrue")
                if args.len() == 2
                    && self.world.try_resource::<crate::ConditionForms>().is_some_and(|forms| forms.contains(*form)) =>
            {
                let values = self.eval_args(frame, args)?;
                let entity = |value: &Value| match value {
                    Value::Form(reference) => self.entity_of(*reference),
                    _ => None,
                };
                let Some(subject) = entity(&values[0]) else {
                    return stop("IsTrue without a loaded reference");
                };
                let target = entity(&values[1]);
                match crate::condition_form_is_true(self.world, *form, subject, target) {
                    Some(result) => Some(Value::Bool(result)),
                    None => return stop("IsTrue on a condition form the engine cannot fully evaluate"),
                }
            }
            // Starfield `SetScanned(abValue)`, `IsScanned()` and the composite
            // `AddToMapScanned(abAllowFastTravel)` = `SetScanned(true)` then
            // `AddToMap(abAllowFastTravel)` (the game's own `ObjectReference.pex`);
            // see [`crate::scanning`].
            (Value::Form(reference), "setscanned") if args.len() == 1 => {
                let flagged = self.eval(frame, &args[0].value.node)?.truthy();
                self.deferred.scanned_changes.push((*reference, flagged));
                Some(Value::None)
            }
            (Value::Form(reference), "isscanned") if args.is_empty() => {
                let flagged = match self.deferred.scanned_changes.iter().rev().find(|(who, _)| who == reference) {
                    Some(&(_, flagged)) => flagged,
                    None => self
                        .world
                        .try_resource::<crate::ScannedReferences>()
                        .is_some_and(|scanned| scanned.is_scanned(*reference)),
                };
                Some(Value::Bool(flagged))
            }
            // Starfield `SetMarkerVisibleOnStarMap(abVisible)`: a native with no body in the
            // game's `ObjectReference.pex`; a per-marker flag, as its name says.
            (Value::Form(marker), "setmarkervisibleonstarmap") if args.len() == 1 => {
                let visible = self.eval(frame, &args[0].value.node)?.truthy();
                self.deferred.star_map_changes.push((*marker, visible));
                Some(Value::None)
            }
            (Value::Form(reference), "addtomapscanned") if args.len() == 1 => {
                let fast_travel = self.eval(frame, &args[0].value.node)?.truthy();
                self.deferred.scanned_changes.push((*reference, true));
                self.deferred.map_markers.push((*reference, fast_travel));
                Some(Value::None)
            }
            // CK *ChangeAnimArchetype - Actor* (Fallout 4): "adds the specified
            // AnimArchetype keyword to this actor"; none "removes any existing
            // AnimArchetype keyword". Returns true on success.
            (Value::Form(actor), "changeanimarchetype") if args.len() <= 1 => {
                let values = self.eval_args(frame, args)?;
                let keyword = match values.first() {
                    None | Some(Value::None) => 0,
                    Some(Value::Form(keyword)) => *keyword,
                    _ => return stop("ChangeAnimArchetype without a keyword"),
                };
                let Some(reference) = self.actor_reference(*actor) else {
                    return stop("ChangeAnimArchetype on an actor that is not loaded");
                };
                self.deferred.anim_archetype_changes.push((reference, keyword));
                Some(Value::Bool(true))
            }
            // CK *IsStarting* / *IsStopping - Quest*: "in the process of starting
            // up" / "shutting down". A quest here starts and stops in one step,
            // so it is never between the two states.
            (Value::Form(_), "isstarting" | "isstopping") if args.is_empty() => Some(Value::Bool(false)),
            // CK *CountLinkedRefChain*, which the page gives as script: starting
            // at the reference, each step takes the keyword's link and counts
            // it (a missing link too: A to B to C counts 3) unless the link is
            // the reference itself, and stops past the cap (default 100).
            (Value::Form(form), "countlinkedrefchain") if args.len() <= 2 => {
                let values = self.eval_args(frame, args)?;
                let keyword = match values.first() {
                    Some(Value::Form(keyword)) => *keyword,
                    _ => 0,
                };
                let cap = values.get(1).and_then(Value::number).map_or(MAX_LINK_CHAIN_HOPS as f32, |max| max).min(MAX_LINK_CHAIN_HOPS as f32) as i64;
                let mut count = 0i64;
                let mut current = Some(*form);
                while let Some(here) = current {
                    if count > cap {
                        break;
                    }
                    let next = self.linked_ref(here, keyword);
                    if next == Some(*form) {
                        current = None;
                    } else {
                        current = next;
                        count += 1;
                    }
                }
                Some(Value::Int(count as i32))
            }
            // CK *GetLinkedRefChildren*: the loaded references directly linked
            // to this one under the keyword.
            (Value::Form(form), "getlinkedrefchildren") if args.len() == 1 => {
                let keyword = match self.eval(frame, &args[0].value.node)? {
                    Value::Form(keyword) => keyword,
                    _ => 0,
                };
                let children = crate::linked_ref_children(self.world, *form, keyword)
                    .into_iter()
                    .map(Value::Form)
                    .collect();
                Some(Value::Array(Rc::new(RefCell::new(children))))
            }
            // CK *GetNthLinkedRef*: 0 is the object itself, n follows the link
            // under the keyword (default none) n times.
            (Value::Form(form), "getnthlinkedref") => {
                let values = self.eval_args(frame, args)?;
                let hops = values.first().and_then(Value::number).unwrap_or(0.0);
                let keyword = match values.get(1) {
                    Some(Value::Form(keyword)) => *keyword,
                    _ => 0,
                };
                let mut current = Some(*form);
                // #5844 — past the ceiling there is no Nth link: return
                // None rather than silently the 100th.
                if hops.max(0.0) as usize > MAX_LINK_CHAIN_HOPS {
                    return Ok(Some(Value::None));
                }
                for _ in 0..(hops.max(0.0) as usize).min(MAX_LINK_CHAIN_HOPS) {
                    current = current.and_then(|form| self.linked_ref(form, keyword));
                }
                Some(current.map_or(Value::None, Value::Form))
            }
            (Value::Form(form), "getbaseobject" | "getactorbase" | "getleveledactorbase") => {
                Some(self.base_of(*form).map_or(Value::None, Value::Form))
            }
            // CK *Papyrus Runtime Errors*: 'Cannot call X() on a None object, aborting
            // function call' - "since you cannot call a non-global function on
            // nothing, the call is ignored and the error printed out". A call for
            // effect is skipped and the script goes on; one whose result is used has
            // no result to give, so the run stops.
            (Value::None, _) if self.for_effect => {
                log::debug!("Papyrus: {name}() called on a None object, ignored");
                Some(Value::None)
            }
            (Value::None, _) => return stop(format!("{name} on None")),
            _ => None,
        })
    }

    /// CK *PlaceAtMe - ObjectReference*: "Makes X new references of the passed
    /// in form ... and places them at our reference's location, returning the
    /// last one it created. Or None if no references could be created". The
    /// `abInitiallyDisabled` flag "is only applied to the last reference
    /// created". The references are recorded after the quest guards drop
    /// ([`crate::CreatedReferences`]), where the engine places them at `at` and
    /// spawns them whenever their cell is loaded. Arguments past the FO4
    /// `abDeleteWhenAble` are Starfield's offsets, alias fill and navmesh snap,
    /// which the checker only lets through unset.
    fn place_at_me(&mut self, at: u32, values: &[Value]) -> Run<Value> {
        let Some(Value::Form(base)) = values.first() else {
            return Ok(Value::None);
        };
        let count = values.get(1).and_then(Value::int).unwrap_or(1).clamp(1, MAX_PLACED_PER_CALL);
        let disabled = values.get(3).is_some_and(Value::truthy);
        let delete_when_able = values.get(4).is_some_and(Value::truthy);
        if !matches!(values.get(5), None | Some(Value::None)) || !matches!(values.get(6), None | Some(Value::None)) {
            return stop("PlaceAtMe with offsets or an alias to fill");
        }
        let Some(ids) = self.deferred.scripted_ids.clone() else {
            return stop("PlaceAtMe without CreatedReferences");
        };
        let mut last = Value::None;
        for _ in 0..count {
            let Some(form_id) = ids.next() else {
                break;
            };
            self.deferred.created_references.push((form_id, *base, at, delete_when_able));
            last = Value::Form(form_id);
        }
        if disabled {
            if let Value::Form(form_id) = last {
                self.deferred.reference_enable_changes.push((form_id, false));
            }
        }
        Ok(last)
    }

    /// The forms of the `FLST` `list`, if it is one.
    fn form_list(&self, list: u32) -> Option<Vec<u32>> {
        crate::forms_of(self.world, list, &self.deferred.form_list_changes)
    }

    fn entity_form(&self, entity: EntityId) -> Option<u32> {
        let player = crate::papyrus_demo::PapyrusPlayerEntity::get(self.world);
        if Some(entity) == player {
            return Some(PLAYER_REF);
        }
        crate::condition::entity_global_form_id(self.world, entity)
    }

    fn entity_of(&self, form: u32) -> Option<EntityId> {
        crate::condition::resolve_entity_by_global_form_id(self.world, form)
    }

    fn linked_ref(&self, form: u32, keyword: u32) -> Option<u32> {
        crate::linked_ref(self.world, form, keyword)
    }

    fn base_of(&self, form: u32) -> Option<u32> {
        let entity = self.entity_of(form)?;
        crate::relationships::actor_base(self.world, entity)
    }

    /// Re-write `callee(args)` over bound values for the primitives.
    fn synthesize(
        &mut self,
        frame: &mut Frame,
        callee: &Expr,
        args: &[CallArg],
    ) -> Run<(Expr, NativeBindings, ScriptInstanceData)> {
        let mut synth = Synth::default();
        let mut bound_receiver = None;
        let callee = match callee {
            Expr::MemberAccess { object, member } => {
                let receiver = if static_class(&object.node).is_some() {
                    (**object).clone()
                } else {
                    let value = self.eval(frame, &object.node)?;
                    if matches!(value, Value::None) {
                        return stop(format!("{} on None", member.node.0));
                    }
                    let receiver = synth.bind(self, &value)?;
                    bound_receiver = Some(receiver.clone());
                    receiver
                };
                Expr::MemberAccess {
                    object: Box::new(receiver),
                    member: member.clone(),
                }
            }
            // A bare global native (`GetCurrentGameTime()` style).
            other => other.clone(),
        };
        let mut bound = Vec::with_capacity(args.len());
        for arg in args {
            // An engine constant passes as the named member it was written
            // as (`Self.Motion_Keyframed`): its number differs per game, the
            // name does not.
            let constant = constant_name(&arg.value.node).filter(|name| self.engine_constant(name).is_some());
            // `SetMotionType(4, …)`: the raw number is the game's (see
            // [`skyrim_motion_constant`]); it passes as the constant it names.
            if bound.is_empty() && callee_member_is(&callee, "setmotiontype") {
                if let Some(receiver) = &bound_receiver {
                    let raw = self.eval(frame, &arg.value.node)?;
                    let skyrim = self
                        .world
                        .try_resource::<crate::ScriptGame>()
                        .is_some_and(|game| game.0 == byroredux_plugin::esm::reader::GameKind::Skyrim);
                    if let (Value::Int(raw), true) = (&raw, skyrim) {
                        let Some(name) = skyrim_motion_constant(*raw) else {
                            return stop(format!("SetMotionType with the motion type {raw}"));
                        };
                        bound.push(CallArg {
                            name: None,
                            value: spanned(Expr::MemberAccess {
                                object: Box::new(receiver.clone()),
                                member: spanned(Identifier(name.to_owned())),
                            }),
                        });
                        continue;
                    }
                }
            }
            let value = match (constant, &bound_receiver) {
                (Some(name), Some(receiver)) => spanned(Expr::MemberAccess {
                    object: Box::new(receiver.clone()),
                    member: spanned(Identifier(name.to_owned())),
                }),
                _ => {
                    let value = self.eval(frame, &arg.value.node)?;
                    synth.bind(self, &value)?
                }
            };
            bound.push(CallArg { name: None, value });
        }
        let call = Expr::Call {
            callee: Box::new(spanned(callee)),
            args: bound,
        };
        let (bindings, vmad) = synth.vmad();
        Ok((call, bindings, vmad))
    }

    fn call_value(&mut self, frame: &mut Frame, callee: &Expr, args: &[CallArg]) -> Run<Value> {
        if matches!(guard_call(callee), Some(GuardCall::TryLock)) {
            return Ok(Value::Bool(true));
        }
        if let Some(value) = self.array_call(frame, callee, args)? {
            return Ok(value);
        }
        if let Some(value) = self.collection_call(frame, callee, args)? {
            return Ok(value);
        }
        if let Some(value) = self.layer_call(frame, callee, args)? {
            return Ok(value);
        }
        // (A call whose value is used is not "for effect", whatever encloses it.)
        let saved = std::mem::replace(&mut self.for_effect, false);
        let read = self.object_read(frame, callee, args);
        self.for_effect = saved;
        if let Some(value) = read? {
            return Ok(value);
        }
        if let Some(value) = self.script_call(frame, callee, args)? {
            return Ok(value);
        }
        if self_call_named(callee, "getstate") {
            let script = Arc::clone(&frame.script);
            return Ok(Value::Str(self.script_state(frame.owner, &script)));
        }
        let (call, bindings, vmad) = self.synthesize(frame, callee, args)?;
        if let Some(value) = classify_native_value(&call, &bindings) {
            let number = evaluate_value(&value, self.context, Some(&vmad), self.world, self.stages, self.deferred);
            return match number {
                Some(number) => Ok(Value::Float(number)),
                None => stop("a numeric read could not be resolved"),
            };
        }
        if let Some(guards) = classify_native_guard(&call, &bindings) {
            let mut all = true;
            for guard in &guards {
                match evaluate_guard(guard, self.context, Some(&vmad), self.world, self.stages, self.objectives, self.deferred) {
                    Some(pass) => all &= pass,
                    None => return stop("a Bool read could not be resolved"),
                }
            }
            return Ok(Value::Bool(all));
        }
        stop(format!("unsupported call {}", callee_name(callee)))
    }

    fn call_statement(&mut self, frame: &mut Frame, expression: &Expr) -> Run<Flow> {
        let Expr::Call { callee, args } = expression else {
            // A bare expression statement has no effect.
            self.eval(frame, expression)?;
            return Ok(Flow::Normal);
        };
        // `Utility.Wait(seconds)` suspends the run, every frame of it.
        if let Expr::MemberAccess { object, member } = &callee.node {
            if static_class(&object.node) == Some("utility") && member.node.0.eq_ignore_ascii_case("wait") {
                let seconds = match args.first() {
                    Some(arg) => self.eval(frame, &arg.value.node)?.number().unwrap_or(0.0),
                    None => 0.0,
                };
                // #5681 — the seconds are a run-time value here, so a literal
                // the lowerer already declined can still reach the VM. A wait
                // past the shared cap would park an immortal continuation in
                // `FragmentExecutionQueue`; stopping the run is the
                // decline-safe outcome, same choice the provider lowerer
                // makes for the same literal.
                if seconds > MAX_PROVIDER_WAIT_SECONDS {
                    return stop("Utility.Wait beyond MAX_PROVIDER_WAIT_SECONDS");
                }
                return Ok(Flow::Suspend(seconds.max(0.0), Vec::new(), Vec::new()));
            }
        }
        // CK *WaitForAnimationEvent*: "waits for the specified animation event from
        // this object to happen before returning". The graph's schedule is
        // deterministic, so the wait is computed now; with no graph (or an event it
        // never raises) the call returns false at once, which the CK gives as "the
        // animation graph unloaded before the event was sent".
        if let Expr::MemberAccess { object, member } = &callee.node {
            if member.node.0.eq_ignore_ascii_case("waitforanimationevent")
                && args.len() == 1
                && static_class(&object.node).is_none()
            {
                if let Value::Form(reference) = self.eval(frame, &object.node)? {
                    let values = self.eval_args(frame, args)?;
                    let event = values[0].text();
                    let seconds = self
                        .entity_of(reference)
                        .and_then(|entity| crate::animation_graph::wait_for_animation_event(self.world, entity, &event));
                    return match seconds {
                        Some(seconds) if seconds > MAX_PROVIDER_WAIT_SECONDS => {
                            stop("WaitForAnimationEvent beyond MAX_PROVIDER_WAIT_SECONDS")
                        }
                        Some(seconds) => Ok(Flow::Suspend(seconds.max(0.0), Vec::new(), Vec::new())),
                        None => Ok(Flow::Normal),
                    };
                }
            }
        }
        // CK *PlayAnimationAndWait*: send the event and wait until the graph raises
        // the named one (the schedule is deterministic, so the wait is computed
        // now). A graph that rejects the event, or never raises the other, makes
        // the call return false at once: no wait.
        if let Expr::MemberAccess { object, member } = &callee.node {
            if member.node.0.eq_ignore_ascii_case("playanimationandwait")
                && args.len() == 2
                && static_class(&object.node).is_none()
            {
                if let Value::Form(reference) = self.eval(frame, &object.node)? {
                    let values = self.eval_args(frame, args)?;
                    let (event, wait_event) = (values[0].text(), values[1].text());
                    let seconds = self.entity_of(reference).and_then(|entity| {
                        crate::animation_graph::play_animation_and_wait(self.world, entity, &event, &wait_event)
                    });
                    return match seconds {
                        Some(seconds) if seconds > MAX_PROVIDER_WAIT_SECONDS => {
                            stop("PlayAnimationAndWait beyond MAX_PROVIDER_WAIT_SECONDS")
                        }
                        Some(seconds) => Ok(Flow::Suspend(seconds.max(0.0), Vec::new(), Vec::new())),
                        None => Ok(Flow::Normal),
                    };
                }
            }
        }
        // Help-message housekeeping names the `Message` class or a message: the
        // receiver is never read.
        // CK *Add - MusicType* / *Remove - MusicType*: "Adds this music type to
        // the music stack, causing it to start playing if it's the highest
        // priority item". There is no music player (the music is xWMA, which the
        // engine cannot decode), so the stack has no observable state and the
        // call is a no-op. Only a receiver declared a `MusicType` is one (a
        // script's own `Add()` function is a different call).
        if let Expr::MemberAccess { object, member } = &callee.node {
            let name = member.node.0.to_ascii_lowercase();
            if matches!(name.as_str(), "add" | "remove") && args.is_empty() && declares_music_type(&frame.script, &object.node) {
                return Ok(Flow::Normal);
            }
        }
        if matches!(guard_call(&callee.node), Some(GuardCall::Lock | GuardCall::Unlock)) {
            return Ok(Flow::Normal);
        }
        // CK *StopDialogueCamera - Game*: "Stops all dialogue cameras". The
        // engine never starts one (a conversation here has no camera of its
        // own), so there is nothing to stop.
        if let Expr::MemberAccess { object, member } = &callee.node {
            if static_class(&object.node) == Some("game") && member.node.0.eq_ignore_ascii_case("stopdialoguecamera") {
                self.eval_args(frame, args)?;
                return Ok(Flow::Normal);
            }
        }
        if help_message_housekeeping(&callee.node).is_some() {
            self.eval_args(frame, args)?;
            return Ok(Flow::Normal);
        }
        if self.array_call(frame, &callee.node, args)?.is_some() {
            return Ok(Flow::Normal);
        }
        if self.collection_call(frame, &callee.node, args)?.is_some() {
            return Ok(Flow::Normal);
        }
        if self.layer_call(frame, &callee.node, args)?.is_some() {
            return Ok(Flow::Normal);
        }
        self.for_effect = true;
        let read = self.object_read(frame, &callee.node, args);
        self.for_effect = false;
        if read?.is_some() {
            return Ok(Flow::Normal);
        }
        // CK *UpdateCurrentInstanceGlobal - Quest*: "Attempts to update the
        // stored value of the given global in the instance data for the quest's
        // current instance", i.e. refreshes a cached copy of the global. This
        // engine keeps no cached copy (quest text reads the global live), so
        // the statement has nothing to refresh; only the Bool result (failure
        // with no active instance) is unmodelled, and the checker only lets
        // the call through with that result unused.
        if let Expr::MemberAccess { object, member } = &callee.node {
            if member.node.0.eq_ignore_ascii_case("updatecurrentinstanceglobal")
                && static_class(&object.node).is_none()
            {
                if let Value::Form(quest) = self.eval(frame, &object.node)? {
                    if self.is_quest(quest) {
                        self.eval_args(frame, args)?;
                        return Ok(Flow::Normal);
                    }
                }
            }
        }
        // A script function called as a statement may wait.
        if let Some((callee_frame, code)) = self.script_call_target(frame, &callee.node)? {
            let name = callee_name(&callee.node);
            let values = self.eval_args(frame, args)?;
            let mut callee_frame = self.enter(callee_frame, &code, &name, values)?;
            // The caller stays on the stack while the callee runs: if it holds
            // a local a suspension cannot store, the callee must not suspend.
            let blocked = !locals_storable(frame);
            self.blocked_callers += usize::from(blocked);
            let flow = self.run_frame(&mut callee_frame, &code, &name, None, &[]);
            self.blocked_callers -= usize::from(blocked);
            return self.callee_flow(flow?, &callee_frame, &code, &name);
        }
        // `Parent.OnInit()` and the like past the scripts to an engine class:
        // its events have no body, so the call does nothing.
        if let Expr::MemberAccess { object, member } = &callee.node {
            if matches!(object.node, Expr::ParentAccess) && is_event_name(&member.node.0) {
                self.eval_args(frame, args)?;
                return Ok(Flow::Normal);
            }
        }
        // `GotoState`'s `OnEndState` / `OnBeginState` the script does not
        // handle: the empty events every script inherits.
        if is_state_event(&callee.node) {
            return Ok(Flow::Normal);
        }
        if let Some((object, name)) = quest_update_call(&callee.node) {
            if let Value::Form(quest) = self.eval(frame, object)? {
                if self.is_quest(quest) {
                    return self.quest_update(frame, quest, &name, args).map(|_| Flow::Normal);
                }
            }
        }
        if self_call_named(&callee.node, "sendcustomevent") {
            return self.send_custom_event(frame, args).map(|_| Flow::Normal);
        }
        if let Some(name) = update_call(&callee.node) {
            if super::EventRequest::is_call(&name) {
                return self.event_registration(frame, None, &name, args).map(|_| Flow::Normal);
            }
            if super::DistanceRequest::handler(&name).is_some() {
                return self.distance_registration(frame, None, &name, args).map(|_| Flow::Normal);
            }
            if super::HitRequest::handler(&name).is_some() {
                return self.hit_registration(frame, &name, args).map(|_| Flow::Normal);
            }
            let mut numbers = Vec::with_capacity(args.len());
            for arg in args {
                numbers.push(self.eval(frame, &arg.value.node)?.number());
            }
            let Some(request) = super::UpdateRequest::of_call(&name, &numbers) else {
                return stop(format!("{name} without its numbers"));
            };
            let script = frame.script.name.node.0.clone();
            self.deferred.script_updates.push((frame.owner.0, script, request));
            return Ok(Flow::Normal);
        }
        if self_call_named(&callee.node, "gotostate") {
            let state = match args.first() {
                Some(arg) => self.eval(frame, &arg.value.node)?.text(),
                None => String::new(),
            };
            self.goto_state(frame, state)?;
            return Ok(Flow::Normal);
        }
        if is_trace(&callee.node) {
            return Ok(Flow::Normal);
        }
        let (call, bindings, vmad) = self.synthesize(frame, &callee.node, args)?;
        let Some(effect) = classify_native_effect(&call, &bindings) else {
            return stop(format!("unsupported statement {}", callee_name(&callee.node)));
        };
        if crate::translate::effects::is_latent(&effect) {
            return stop("a latent call");
        }
        let advances = apply_effects(
            std::slice::from_ref(&effect),
            self.context,
            Some(&vmad),
            self.world,
            self.stages,
            self.objectives,
            self.deferred,
        );
        self.advances.extend(advances);
        Ok(Flow::Normal)
    }
}

/// A Starfield guard operation, as the decompiler lifts it.
enum GuardCall {
    Lock,
    Unlock,
    TryLock,
}

/// `Guard.lock(...)` / `.unlock(...)` / `.trylock(...)`: the lifted form of
/// `LockGuards` / `UnlockGuards` / `TryLockGuards`, whose arguments are guard
/// names, not values. A guard serialises threads that interleave only when a run
/// suspends; this VM's runs interleave at a wait or a budget slice, and a guard
/// held across either is not modelled, so every operation succeeds at once (the
/// try-lock reads True).
fn guard_call(callee: &Expr) -> Option<GuardCall> {
    let Expr::MemberAccess { object, member } = callee else {
        return None;
    };
    if !matches!(&object.node, Expr::Ident(id) if id.0 == "Guard") {
        return None;
    }
    match member.node.0.as_str() {
        "lock" => Some(GuardCall::Lock),
        "unlock" => Some(GuardCall::Unlock),
        "trylock" => Some(GuardCall::TryLock),
        _ => None,
    }
}

/// `Global.Value = n` as the native it is: `Global.SetValue(n)`.
fn set_value_call(global: &Spanned<Expr>, value: &Spanned<Expr>) -> (Box<Spanned<Expr>>, Vec<CallArg>) {
    let callee = spanned(Expr::MemberAccess {
        object: Box::new(global.clone()),
        member: spanned(Identifier("SetValue".into())),
    });
    let args = vec![CallArg {
        name: None,
        value: value.clone(),
    }];
    (Box::new(callee), args)
}

/// An update (un)registration on another object: the receiver and the
/// lowercase name.
fn quest_update_call(callee: &Expr) -> Option<(&Expr, String)> {
    let Expr::MemberAccess { object, member } = callee else {
        return None;
    };
    let name = member.node.0.to_ascii_lowercase();
    let on_self = matches!(&object.node, Expr::Ident(id) if id.0.eq_ignore_ascii_case("self"));
    (registration_handler(&name).is_some() && !on_self && static_class(&object.node).is_none())
        .then_some((&object.node, name))
}

/// The handler a registration call delivers to (`""` for a cancellation):
/// an update, timer or distance registration, by lowercase name.
fn registration_handler(name: &str) -> Option<&'static str> {
    super::UpdateRequest::handler(name)
        .or_else(|| super::DistanceRequest::handler(name))
        .or_else(|| super::HitRequest::handler(name))
        .or_else(|| super::EventRequest::is_call(name).then_some(""))
}

/// An update (un)registration on the script itself, by lowercase name.
fn update_call(callee: &Expr) -> Option<String> {
    let name = callee_name(callee).to_ascii_lowercase();
    (registration_handler(&name).is_some() && self_call_named(callee, &name)).then_some(name)
}

/// A call of `name` (lowercase) on the script itself.
fn self_call_named(callee: &Expr, name: &str) -> bool {
    let (receiver, called) = match callee {
        Expr::Ident(called) => (None, called),
        Expr::MemberAccess { object, member } => (Some(&object.node), &member.node),
        _ => return false,
    };
    receiver.is_none_or(|r| matches!(r, Expr::Ident(id) if id.0.eq_ignore_ascii_case("self")))
        && called.0.eq_ignore_ascii_case(name)
}

/// `OnBeginState()` / `OnEndState()` on the script itself.
fn is_state_event(callee: &Expr) -> bool {
    let (receiver, name) = match callee {
        Expr::Ident(name) => (None, name),
        Expr::MemberAccess { object, member } => (Some(&object.node), &member.node),
        _ => return false,
    };
    let on_self = receiver.is_none_or(|r| matches!(r, Expr::Ident(id) if id.0.eq_ignore_ascii_case("self")));
    on_self && (name.0.eq_ignore_ascii_case("onbeginstate") || name.0.eq_ignore_ascii_case("onendstate"))
}

/// Whether `name` is written like an event (`OnInit`, `OnActivate`).
fn is_event_name(name: &str) -> bool {
    name.len() > 2 && name[..2].eq_ignore_ascii_case("on") && name[2..3].chars().all(|c| c.is_ascii_uppercase())
}

/// `Message.ResetHelpMessage(asEvent)`, `<message>.UnshowAsHelpMessage()` and
/// `Message.ClearHelpMessages()`, with the number of arguments each takes. A help
/// message is shown once here, with no repeat interval or event to end it, so
/// there is no shown-count to reset ("allowing a message to be displayed for that
/// input event") and nothing standing to hide: each is a no-op (an
/// `UnshowAsHelpMessage` / `ClearHelpMessages` that arrives while the one-shot is
/// still on screen does not dismiss it).
fn help_message_housekeeping(callee: &Expr) -> Option<usize> {
    let Expr::MemberAccess { member, .. } = callee else {
        return None;
    };
    match member.node.0.to_ascii_lowercase().as_str() {
        "resethelpmessage" => Some(1),
        "unshowashelpmessage" | "clearhelpmessages" => Some(0),
        _ => None,
    }
}

/// Whether `callee` is a member call named `name` (lowercase).
fn callee_member_is(callee: &Expr, name: &str) -> bool {
    matches!(callee, Expr::MemberAccess { member, .. } if member.node.0.eq_ignore_ascii_case(name))
}

/// The `Motion_*` constant Skyrim's raw `SetMotionType` number names (CK
/// *SetMotionType - ObjectReference*: Dynamic 1, SphereInertia 2, BoxInertia 3,
/// Keyframed 4, Fixed 5, ThinBoxInertia 6, Character 7). The inertia variants
/// are softened simulations the physics layer has no kind for, so they are not
/// mapped. Fallout 4's numbers differ, and no page here lists them.
fn skyrim_motion_constant(raw: i32) -> Option<&'static str> {
    Some(match raw {
        1 => "Motion_Dynamic",
        4 => "Motion_Keyframed",
        5 => "Motion_Fixed",
        7 => "Motion_Character",
        _ => return None,
    })
}

fn is_trace(callee: &Expr) -> bool {
    let Expr::MemberAccess { object, member } = callee else {
        return false;
    };
    let name = member.node.0.to_ascii_lowercase();
    // `Debug.Trace…` write the script log; `OpenUserLog` / `CloseUserLog` (CK:
    // "Opens a new user log on disk") open and close a file of the same kind.
    // Nothing else reads either, so a statement call has no game effect.
    static_class(&object.node) == Some("debug")
        && (name.starts_with("trace") || name == "openuserlog" || name == "closeuserlog")
}

fn callee_name(callee: &Expr) -> String {
    match callee {
        Expr::MemberAccess { member, .. } => member.node.0.clone(),
        Expr::Ident(name) => name.0.clone(),
        _ => "<call>".into(),
    }
}

fn cast(value: Value, ty: &Type) -> Value {
    match ty {
        Type::Bool => Value::Bool(value.truthy()),
        Type::Int => match value {
            Value::Str(s) => Value::Int(s.trim().parse::<f32>().unwrap_or(0.0) as i32),
            other => Value::Int(other.number().unwrap_or(0.0) as i32),
        },
        Type::Float => match value {
            Value::Str(s) => Value::Float(s.trim().parse::<f32>().unwrap_or(0.0)),
            other => Value::Float(other.number().unwrap_or(0.0)),
        },
        Type::String => Value::Str(value.text()),
        // Object casts keep the object; Papyrus yields None on a mismatch,
        // which needs the type of a form this VM does not track.
        _ => value,
    }
}

fn arithmetic(op: BinaryOp, l: &Value, r: &Value) -> Run<Value> {
    if op == BinaryOp::Add && (matches!(l, Value::Str(_)) || matches!(r, Value::Str(_))) {
        return concat_texts(l, r);
    }
    let (Some(a), Some(b)) = (l.number(), r.number()) else {
        return stop(format!("arithmetic on {} and {}", l.text(), r.text()));
    };
    if let (Some(a), Some(b)) = (l.int(), r.int()) {
        // Papyrus `Int` arithmetic is 32-bit and exact.
        return Ok(Value::Int(match op {
            BinaryOp::Add => a.wrapping_add(b),
            BinaryOp::Sub => a.wrapping_sub(b),
            BinaryOp::Mul => a.wrapping_mul(b),
            BinaryOp::Div if b == 0 => return stop("integer division by zero"),
            BinaryOp::Div => a.wrapping_div(b),
            BinaryOp::Mod if b == 0 => return stop("modulo by zero"),
            BinaryOp::Mod => a.wrapping_rem(b),
            _ => return stop("not an arithmetic operator"),
        }));
    }
    Ok(match op {
        BinaryOp::Add => Value::Float(a + b),
        BinaryOp::Sub => Value::Float(a - b),
        BinaryOp::Mul => Value::Float(a * b),
        BinaryOp::Div => Value::Float(a / b),
        BinaryOp::Mod => {
            if b as i32 == 0 {
                return stop("modulo by zero");
            }
            Value::Int((a as i32) % (b as i32))
        }
        _ => return stop("not an arithmetic operator"),
    })
}

fn binary(op: BinaryOp, l: &Value, r: &Value) -> Run<Value> {
    Ok(match op {
        BinaryOp::Eq => Value::Bool(l.equals(r)),
        BinaryOp::Ne => Value::Bool(!l.equals(r)),
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
            let ordering = match (l, r) {
                (Value::Str(a), Value::Str(b)) => a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()),
                _ if l.int().is_some() && r.int().is_some() => l.int().cmp(&r.int()),
                _ => {
                    let (Some(a), Some(b)) = (l.number(), r.number()) else {
                        return stop(format!("comparing {} and {}", l.text(), r.text()));
                    };
                    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
                }
            };
            Value::Bool(match op {
                BinaryOp::Lt => ordering.is_lt(),
                BinaryOp::Le => ordering.is_le(),
                BinaryOp::Gt => ordering.is_gt(),
                _ => ordering.is_ge(),
            })
        }
        BinaryOp::StrCat => concat_texts(l, r)?,
        op => arithmetic(op, l, r)?,
    })
}

/// String concatenation under the sandbox's byte ceiling, so one statement
/// cannot more than double a string past `MAX_STRING_BYTES`.
fn concat_texts(l: &Value, r: &Value) -> Run<Value> {
    let (a, b) = (l.text(), r.text());
    if a.len().saturating_add(b.len()) > MAX_STRING_BYTES {
        return stop(format!("string result over {MAX_STRING_BYTES} bytes"));
    }
    Ok(Value::Str(format!("{a}{b}")))
}

// ── Ahead-of-time check ──────────────────────────────────────────────

/// What the static check knows of an expression's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Num,
    Bool,
    Str,
    Object,
    Quest,
    Player,
    Alias,
    Array,
    /// A Fallout 4 struct.
    Struct,
    None,
}

/// Where the check resolves scripts: the library populate supplies.
pub use crate::translate::effects::ScriptLibrary as CheckLibrary;

struct Checker<'l> {
    library: Option<&'l dyn CheckLibrary>,
    /// Whether the function being checked waits (itself or in a function
    /// it calls as a statement).
    waits: bool,
    /// The functions that wait, by lowercased (script, function).
    waiting: HashSet<(String, String)>,
    /// The first reason the check failed, for diagnostics.
    reason: Option<String>,
    /// Scripts the run will need, by lowercased name.
    scripts: HashMap<String, Arc<Script>>,
    visiting: HashSet<(String, String)>,
}

struct CheckFrame {
    script: Arc<Script>,
    /// The script whose body is being checked: the instance's script or one it
    /// extends. `Parent.Fn()` calls past it.
    defined_in: Arc<Script>,
    /// What `Self` is: a quest, or a reference for a reference script.
    self_kind: Kind,
    locals: HashMap<String, Kind>,
    /// Locals' and parameters' declared types (a quest local's script type).
    types: HashMap<String, Type>,
}

/// Whether `function` of `script` (a fragment of a quest) runs whole on this
/// VM; `Some` carries every script the run needs.
pub fn check_function(
    script: Arc<Script>,
    function: &str,
    library: Option<&dyn CheckLibrary>,
) -> Option<Vec<Arc<Script>>> {
    check_function_reason(script, function, library).ok()
}

/// [`check_function`] with the first reason it fails.
pub fn check_function_reason(
    script: Arc<Script>,
    function: &str,
    library: Option<&dyn CheckLibrary>,
) -> Result<Vec<Arc<Script>>, String> {
    check_reason(script, function, library, Kind::Quest)
}

/// Whether a reference script runs on this VM: it handles at least one of
/// `events`, and every handler of them — in any state, down the `extends`
/// chain — runs whole. `Ok` carries the events it handles and every script
/// the runs need; `Err` the first reason it does not run.
pub fn check_reference_script(
    script: Arc<Script>,
    events: &[&str],
    library: Option<&dyn CheckLibrary>,
) -> Result<(Vec<String>, Vec<Arc<Script>>), String> {
    let mut checker = Checker::new(library, &script);
    let mut handled = Vec::new();
    for event in events {
        if checker.versions(&script, event).is_empty() || checker.hit_by_registration(&script, event) {
            continue;
        }
        if checker.check_call_into(&script, event, true, Kind::Object).is_none() {
            let reason = checker.reason.take().unwrap_or_else(|| "unsupported".into());
            return Err(format!("{event}: {reason}"));
        }
        handled.push((*event).to_owned());
    }
    if handled.is_empty() {
        return Err("handles no delivered event".into());
    }
    Ok((handled, checker.scripts.into_values().collect()))
}

/// Whether an alias script runs on this VM: [`check_reference_script`] with
/// `Self` a `ReferenceAlias`, over the events an alias script receives.
pub fn check_alias_script(
    script: Arc<Script>,
    events: &[&str],
    library: Option<&dyn CheckLibrary>,
) -> Result<(Vec<String>, Vec<Arc<Script>>), String> {
    let mut checker = Checker::new(library, &script);
    let mut handled = Vec::new();
    for event in events {
        if checker.versions(&script, event).is_empty() || checker.hit_by_registration(&script, event) {
            continue;
        }
        if checker.check_call_into(&script, event, true, Kind::Alias).is_none() {
            let reason = checker.reason.take().unwrap_or_else(|| "unsupported".into());
            return Err(format!("{event}: {reason}"));
        }
        handled.push((*event).to_owned());
    }
    if handled.is_empty() {
        return Err("handles no delivered event".into());
    }
    Ok((handled, checker.scripts.into_values().collect()))
}

/// The installed scripts as a [`CheckLibrary`].
struct Installed<'a>(&'a HashMap<String, Arc<Script>>);

impl crate::translate::effects::ScriptLibrary for Installed<'_> {
    fn script(&self, name: &str) -> Option<Arc<Script>> {
        self.0.get(&name.to_ascii_lowercase()).cloned()
    }
}

/// What an installed script instance is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum SelfKind {
    Quest,
    Object,
    Alias,
}

impl SelfKind {
    /// The kind of the instance owned by `owner`: a quest (`on_quest`), a quest
    /// alias, or a reference.
    pub(super) fn of_owner(owner: u32, on_quest: bool) -> Self {
        if on_quest {
            Self::Quest
        } else if super::is_alias_owner(owner) {
            Self::Alias
        } else {
            Self::Object
        }
    }
}

/// Whether `handler` of the installed `script` runs whole on this VM,
/// against the installed scripts — on a quest, a reference or an alias.
pub(super) fn handler_runs(scripts: &HashMap<String, Arc<Script>>, script: &str, handler: &str, self_kind: SelfKind) -> bool {
    let Some(script) = scripts.get(&script.to_ascii_lowercase()) else {
        return false;
    };
    let kind = match self_kind {
        SelfKind::Quest => Kind::Quest,
        SelfKind::Object => Kind::Object,
        SelfKind::Alias => Kind::Alias,
    };
    let library = Installed(scripts);
    let mut checker = Checker::new(Some(&library), script);
    checker.versions(script, handler).is_empty() || checker.check_call_into(script, handler, true, kind).is_some()
}

fn check_reason(
    script: Arc<Script>,
    function: &str,
    library: Option<&dyn CheckLibrary>,
    self_kind: Kind,
) -> Result<Vec<Arc<Script>>, String> {
    let mut checker = Checker::new(library, &script);
    match checker.check_call_into(&script, function, true, self_kind) {
        Some(_) => Ok(checker.scripts.into_values().collect()),
        None => Err(checker.reason.unwrap_or_else(|| "unsupported".into())),
    }
}

fn kind_of_type(ty: &Type, library: Option<&dyn CheckLibrary>) -> Kind {
    match ty {
        Type::Int | Type::Float => Kind::Num,
        Type::Bool => Kind::Bool,
        Type::String => Kind::Str,
        Type::Array(_) => Kind::Array,
        Type::Var => Kind::None,
        Type::Object(name) => {
            let lower = name.0.to_ascii_lowercase();
            if lower == "quest" || library.is_some_and(|library| extends_quest(library, &lower, 0)) {
                Kind::Quest
            } else if lower.contains("alias") {
                Kind::Alias
            } else {
                Kind::Object
            }
        }
    }
}

fn extends_quest(library: &dyn CheckLibrary, name: &str, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    let Some(script) = library.script(name) else {
        return false;
    };
    match &script.parent {
        Some(parent) if parent.node.0.eq_ignore_ascii_case("quest") => true,
        Some(parent) => extends_quest(library, &parent.node.0.to_ascii_lowercase(), depth + 1),
        None => false,
    }
}

impl<'l> Checker<'l> {
    fn new(library: Option<&'l dyn CheckLibrary>, script: &Arc<Script>) -> Self {
        let mut checker = Checker {
            library,
            reason: None,
            waits: false,
            waiting: HashSet::new(),
            scripts: HashMap::new(),
            visiting: HashSet::new(),
        };
        checker
            .scripts
            .insert(script.name.node.0.to_ascii_lowercase(), Arc::clone(script));
        checker
    }

    /// The type of `member` of the script an Object receiver is declared as
    /// (not an engine class).
    fn reference_member_type(&mut self, frame: &CheckFrame, owner: Kind, receiver: &Expr, member: &str) -> Option<Type> {
        if owner != Kind::Object {
            return None;
        }
        let script = self.declared_script(frame, receiver)?;
        if is_engine_class(&script) {
            return None;
        }
        match self.member_type(&script, member)? {
            Type::Array(_) => None,
            ty => Some(ty),
        }
    }

    /// The check's kind of the declared type `ty` in `script`: a struct type
    /// is a struct, the rest as [`kind_of_type`].
    fn kind_of_declared(&mut self, script: &Arc<Script>, ty: &Type) -> Kind {
        if let Type::Object(name) = ty {
            if self.struct_declaration(script, &name.0).is_some() {
                return Kind::Struct;
            }
        }
        kind_of_type(ty, self.library)
    }

    /// The declaration of the struct type `name` from `script`'s chain or,
    /// for `Script#Struct`, that script — registering it for the run.
    fn struct_declaration(&mut self, script: &Arc<Script>, name: &str) -> Option<byroredux_papyrus::ast::Struct> {
        let owners = match name.split_once('#') {
            Some((owner, _)) => {
                let owner = self.library?.script(owner)?;
                self.scripts
                    .insert(owner.name.node.0.to_ascii_lowercase(), Arc::clone(&owner));
                vec![owner]
            }
            None => self.chain(script),
        };
        let mut known: HashMap<String, Arc<Script>> = HashMap::new();
        for owner in &owners {
            known.insert(owner.name.node.0.to_ascii_lowercase(), Arc::clone(owner));
        }
        struct_declaration(&owners[0], name, &known)
    }

    /// The declared type of `e`, where the check can know it: a typed local
    /// or member, an array's element, a struct's member, a cast.
    fn static_type(&mut self, frame: &CheckFrame, e: &Expr) -> Option<Type> {
        match e {
            Expr::Ident(name) => match frame.types.get(&name.0.to_ascii_lowercase()) {
                Some(ty) => Some(ty.clone()),
                None => {
                    let script = Arc::clone(&frame.script);
                    self.member_type(&script, &name.0)
                }
            },
            Expr::Cast { target_type, .. } => Some(target_type.node.clone()),
            Expr::Index { object, .. } => match self.static_type(frame, &object.node)? {
                Type::Array(element) => Some(*element),
                _ => None,
            },
            Expr::MemberAccess { object, member } => {
                let owner = self.static_type(frame, &object.node)?;
                let Type::Object(owner) = owner else {
                    return None;
                };
                let script = Arc::clone(&frame.script);
                if let Some(declaration) = self.struct_declaration(&script, &owner.0) {
                    return declaration
                        .members
                        .iter()
                        .find(|m| m.name.node.0.eq_ignore_ascii_case(&member.node.0))
                        .map(|m| m.ty.node.clone());
                }
                let declared = self.library?.script(&owner.0)?;
                self.member_type(&declared, &member.node.0)
            }
            _ => None,
        }
    }

    /// An array method's result kind (CK *Arrays (Papyrus)*).
    fn check_array_call(&mut self, name: &str, args: &[Arg], statement: bool) -> Option<Kind> {
        let num = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num));
        let ok = match name {
            "find" | "rfind" => (1..=2).contains(&args.len()) && args.get(1).is_none_or(num),
            "findstruct" | "rfindstruct" => {
                (2..=3).contains(&args.len())
                    && matches!(args[0], Arg::Literal(Expr::StringLit(_)))
                    && args.get(2).is_none_or(num)
            }
            "add" => statement && (1..=2).contains(&args.len()) && args.get(1).is_none_or(num),
            "insert" => statement && args.len() == 2 && num(&args[1]),
            "remove" => statement && (1..=2).contains(&args.len()) && args.iter().all(num),
            "removelast" | "clear" => statement && args.is_empty(),
            _ => false,
        };
        if !ok {
            return self.fail(|| format!("array {name}/{}", args.len()));
        }
        Some(if name.contains("find") { Kind::Num } else { Kind::None })
    }

    /// An engine class's constant `name`'s type, registering the class.
    fn engine_constant_type(&mut self, name: &str) -> Option<Type> {
        CONSTANT_CLASSES.iter().find_map(|class| {
            let script = self.library?.script(class)?;
            let ty = constant_property(&script, name)?.ty.node.clone();
            self.scripts.insert((*class).to_owned(), Arc::clone(&script));
            Some(ty)
        })
    }

    /// `name`'s declared type in `script` or its parents (registering the
    /// parents the run will need).
    fn member_type(&mut self, script: &Script, name: &str) -> Option<Type> {
        if let Some(ty) = script_member_type(script, name) {
            return Some(ty);
        }
        let mut parent = script.parent.as_ref().map(|p| p.node.0.clone());
        for _ in 0..16 {
            let name_of_parent = parent?;
            let lower = name_of_parent.to_ascii_lowercase();
            if matches!(lower.as_str(), "quest" | "topicinfo" | "scene" | "objectreference" | "actor") {
                // An engine class lends only its constants.
                let class = self.library?.script(&name_of_parent)?;
                let ty = constant_property(&class, name)?.ty.node.clone();
                self.scripts.insert(lower, Arc::clone(&class));
                return Some(ty);
            }
            let script = self.library?.script(&name_of_parent)?;
            self.scripts.insert(lower, Arc::clone(&script));
            if let Some(ty) = script_member_type(&script, name) {
                return Some(ty);
            }
            parent = script.parent.as_ref().map(|p| p.node.0.clone());
        }
        None
    }

    fn fail<T>(&mut self, reason: impl FnOnce() -> String) -> Option<T> {
        if self.reason.is_none() {
            self.reason = Some(reason());
        }
        None
    }

    /// `script` and the scripts it extends, registering each for the run.
    fn chain(&mut self, script: &Arc<Script>) -> Vec<Arc<Script>> {
        let mut chain = vec![Arc::clone(script)];
        while chain.len() < 16 {
            let Some(parent) = chain.last().and_then(|last| last.parent.as_ref()).map(|p| p.node.0.clone()) else {
                break;
            };
            let Some(parent) = self.library.and_then(|library| library.script(&parent)) else {
                break;
            };
            self.scripts
                .insert(parent.name.node.0.to_ascii_lowercase(), Arc::clone(&parent));
            chain.push(parent);
        }
        chain
    }

    /// Every body a call of `name` on an instance of `script` may run — in
    /// any state, down the `extends` chain to the engine classes — as
    /// (script, state).
    fn versions(&mut self, script: &Arc<Script>, name: &str) -> Vec<(Arc<Script>, Option<String>)> {
        let mut versions = Vec::new();
        for script in self.chain(script).into_iter().take_while(|script| !is_engine_class(script)) {
            let states = std::iter::once(None).chain(state_names(&script).map(|state| Some(state.to_owned())));
            for state in states.collect::<Vec<_>>() {
                if callable_in(&script, state.as_deref(), name).is_some() {
                    versions.push((Arc::clone(&script), state));
                }
            }
        }
        versions
    }

    /// Whether `event` is Fallout 4's `OnHit(akTarget, akAggressor, akSource,
    /// akProjectile, abPowerAttack, abSneakAttack, abBashAttack, abHitBlocked,
    /// apMaterial)`: unlike Skyrim's seven-parameter `OnHit`, it is delivered only
    /// to the script that registered for it (`RegisterForHitEvent`), so the
    /// engine's automatic delivery leaves it alone and the registration site
    /// checks it.
    fn hit_by_registration(&mut self, script: &Arc<Script>, event: &str) -> bool {
        event.eq_ignore_ascii_case("onhit")
            && self
                .versions(script, event)
                .iter()
                .find_map(|(defined_in, state)| callable_in(defined_in, state.as_deref(), event).map(|c| c.params.len()))
                .is_some_and(|params| params > 7)
    }

    /// Check a call of `function` on an instance of `script` whose `Self`
    /// is `self_kind`: every version of it. `statement`: whether the call is
    /// a statement — a function that waits may not be called for its value.
    fn check_call_into(&mut self, script: &Arc<Script>, function: &str, statement: bool, self_kind: Kind) -> Option<Kind> {
        let key = (script.name.node.0.to_ascii_lowercase(), function.to_ascii_lowercase());
        let versions = self.versions(script, function);
        let returns = versions
            .iter()
            .find_map(|(defined_in, state)| callable_in(defined_in, state.as_deref(), function)?.returns.cloned())
            .map_or(Kind::None, |ty| kind_of_type(&ty.node, self.library));
        if versions.is_empty() {
            return self.fail(|| format!("no function {function}"));
        }
        if !self.visiting.insert(key.clone()) {
            // Checked already, or recursion: assume the declared return.
            return self.call_waits(self.waiting.contains(&key), statement).map(|_| returns);
        }
        let outer = std::mem::replace(&mut self.waits, false);
        for (defined_in, state) in &versions {
            let callable = callable_in(defined_in, state.as_deref(), function)?;
            let mut frame = CheckFrame {
                script: Arc::clone(script),
                defined_in: Arc::clone(defined_in),
                self_kind,
                locals: HashMap::new(),
                types: HashMap::new(),
            };
            for param in callable.params {
                let key = param.name.node.0.to_ascii_lowercase();
                let kind = self.kind_of_declared(script, &param.ty.node);
                frame.locals.insert(key.clone(), kind);
                frame.types.insert(key, param.ty.node.clone());
            }
            callable
                .body
                .iter()
                .try_for_each(|statement| self.check_stmt(&mut frame, &statement.node))?;
        }
        let waits = std::mem::replace(&mut self.waits, outer);
        if waits {
            self.waiting.insert(key);
        }
        self.call_waits(waits, statement).map(|_| returns)
    }

    /// A custom / remote event registration call on an instance of `script`:
    /// the handler it would deliver to must run, and a remote event must be one
    /// the engine raises.
    fn check_event_registration(
        &mut self,
        script: &Arc<Script>,
        name: &str,
        args: &[Arg],
        statement: bool,
        self_kind: Kind,
    ) -> Option<Kind> {
        let object = |arg: &Arg| matches!(arg, Arg::Kind(Kind::Object | Kind::Player | Kind::Alias | Kind::Quest));
        let literal = match args {
            [sender, Arg::Literal(Expr::StringLit(event))] if object(sender) => Some(event.clone()),
            _ => None,
        };
        let shaped = statement
            && args.len() == super::EventRequest::arity(name)
            && (name.starts_with("unregisterforall") || literal.is_some());
        if !shaped {
            return self.fail(|| format!("{name} as written"));
        }
        let remote = name.contains("remote");
        if name.starts_with("unregisterfor") {
            return Some(Kind::None);
        }
        let event = literal.unwrap_or_default();
        if remote && !super::REMOTE_EVENTS.contains(&event.to_ascii_lowercase().as_str()) {
            return self.fail(|| format!("remote event {event} is not raised by the engine"));
        }
        if let Some(handler) = super::events::handler_in(script, &event, remote) {
            self.check_handler(script, &handler, self_kind)?;
        }
        Some(Kind::None)
    }

    /// Check an event handler a registration delivers to later, installing
    /// the scripts it needs when it runs. A handler the VM cannot run does
    /// not fail the registering code — the delivery checks it again and
    /// skips it — so its failed check is rolled back. It may wait; the
    /// registering call does not.
    fn check_handler(&mut self, script: &Arc<Script>, handler: &str, self_kind: Kind) -> Option<()> {
        let saved = (
            self.waits,
            self.reason.clone(),
            self.visiting.clone(),
            self.waiting.clone(),
            self.scripts.clone(),
        );
        if self.check_call_into(script, handler, true, self_kind).is_none() {
            (self.reason, self.visiting, self.waiting, self.scripts) = (saved.1, saved.2, saved.3, saved.4);
        }
        self.waits = saved.0;
        Some(())
    }

    /// A call to a function that `waits` makes its caller wait too.
    fn call_waits(&mut self, waits: bool, statement: bool) -> Option<()> {
        if waits {
            if !statement {
                return self.fail(|| "a wait in a function called for its value".into());
            }
            self.waits = true;
        }
        Some(())
    }

    fn check_block(&mut self, frame: &mut CheckFrame, body: &[Spanned<Stmt>]) -> Option<()> {
        for statement in body {
            self.check_stmt(frame, &statement.node)?;
        }
        Some(())
    }

    fn check_stmt(&mut self, frame: &mut CheckFrame, statement: &Stmt) -> Option<()> {
        match statement {
            Stmt::VarDecl(variable) => {
                if let Some(init) = &variable.initial_value {
                    self.kind(frame, &init.node)?;
                }
                let key = variable.name.node.0.to_ascii_lowercase();
                let kind = self.kind_of_declared(&frame.script, &variable.ty.node);
                frame.locals.insert(key.clone(), kind);
                frame.types.insert(key, variable.ty.node.clone());
            }
            Stmt::Assign { target, value, .. } => {
                let value_kind = self.kind(frame, &value.node)?;
                match &target.node {
                    Expr::Ident(_) => {}
                    Expr::MemberAccess { object, member } => {
                        let owner = self.kind(frame, &object.node)?;
                        if owner == Kind::Struct {
                            if self.static_type(frame, &target.node).is_none() {
                                let member = member.node.0.clone();
                                return self.fail(|| format!("assignment to .{member} of an untyped struct"));
                            }
                        } else if owner == Kind::Object && member.node.0.eq_ignore_ascii_case("value") && value_kind == Kind::Num {
                            // `Global.Value = n`.
                            let (callee, args) = set_value_call(object, value);
                            self.check_call(frame, &callee.node, &args, true)?;
                        } else if value_kind != Kind::Array
                            && self.reference_member_type(frame, owner, &object.node, &member.node.0).is_some()
                        {
                            // A variable of a script on another reference.
                        } else if owner != Kind::Quest || matches!(value_kind, Kind::Str | Kind::Array) {
                            return self.fail(|| format!("{value_kind:?} assignment to a member of {owner:?}"));
                        }
                    }
                    Expr::Index { object, index } => {
                        self.kind(frame, &object.node)?;
                        self.kind(frame, &index.node)?;
                    }
                    _ => return None,
                }
            }
            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.kind(frame, &value.node)?;
                }
            }
            Stmt::If {
                condition,
                body,
                elseif_clauses,
                else_body,
            } => {
                self.kind(frame, &condition.node)?;
                self.check_block(frame, body)?;
                for (condition, body) in elseif_clauses {
                    self.kind(frame, &condition.node)?;
                    self.check_block(frame, body)?;
                }
                if let Some(body) = else_body {
                    self.check_block(frame, body)?;
                }
            }
            Stmt::While { condition, body } => {
                self.kind(frame, &condition.node)?;
                self.check_block(frame, body)?;
            }
            Stmt::ExprStmt(expression) => match &expression.node {
                Expr::Call { callee, args } => {
                    self.check_call(frame, &callee.node, args, true)?;
                }
                other => {
                    self.kind(frame, other)?;
                }
            },
        }
        Some(())
    }

    fn kind(&mut self, frame: &mut CheckFrame, e: &Expr) -> Option<Kind> {
        Some(match e {
            Expr::IntLit(_) | Expr::FloatLit(_) => Kind::Num,
            Expr::BoolLit(_) => Kind::Bool,
            Expr::StringLit(_) => Kind::Str,
            Expr::NoneLit => Kind::None,
            Expr::Ident(name) => {
                let key = name.0.to_ascii_lowercase();
                if let Some(kind) = frame.locals.get(&key) {
                    *kind
                } else if key == "self" {
                    frame.self_kind
                } else if is_state_variable(&name.0) {
                    Kind::Str
                } else {
                    let script = Arc::clone(&frame.script);
                    let Some(ty) = self.member_type(&script, &name.0) else {
                        let name = name.0.clone();
                        return self.fail(|| format!("unknown name {name}"));
                    };
                    self.kind_of_declared(&script, &ty)
                }
            }
            Expr::Cast { expr, target_type } => {
                let inner = self.kind(frame, &expr.node)?;
                match kind_of_type(&target_type.node, self.library) {
                    Kind::Object if inner == Kind::Quest => Kind::Quest,
                    Kind::Object if inner == Kind::Player => Kind::Player,
                    kind => kind,
                }
            }
            Expr::UnaryOp { op, operand } => {
                let inner = self.kind(frame, &operand.node)?;
                match op {
                    UnaryOp::Not => Kind::Bool,
                    UnaryOp::Neg => {
                        if inner != Kind::Num {
                            return None;
                        }
                        Kind::Num
                    }
                }
            }
            Expr::BinaryOp { left, op, right } => {
                let l = self.kind(frame, &left.node)?;
                let r = self.kind(frame, &right.node)?;
                match op {
                    BinaryOp::And | BinaryOp::Or => Kind::Bool,
                    BinaryOp::Eq | BinaryOp::Ne => Kind::Bool,
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                        if !matches!(l, Kind::Num | Kind::Bool | Kind::Str) || !matches!(r, Kind::Num | Kind::Bool | Kind::Str) {
                            return None;
                        }
                        Kind::Bool
                    }
                    BinaryOp::StrCat => Kind::Str,
                    BinaryOp::Add if l == Kind::Str || r == Kind::Str => Kind::Str,
                    _ => {
                        if !matches!(l, Kind::Num | Kind::Bool) || !matches!(r, Kind::Num | Kind::Bool) {
                            return None;
                        }
                        Kind::Num
                    }
                }
            }
            Expr::Index { object, index } => {
                if self.kind(frame, &object.node)? != Kind::Array {
                    return None;
                }
                self.kind(frame, &index.node)?;
                // The element's declared type when the array's is known;
                // else an untracked value.
                match self.static_type(frame, e) {
                    Some(ty) => {
                        let script = Arc::clone(&frame.script);
                        self.kind_of_declared(&script, &ty)
                    }
                    None => Kind::Object,
                }
            }
            Expr::NewStruct { ty } => {
                let script = Arc::clone(&frame.script);
                match &ty.node {
                    Type::Object(name) if self.struct_declaration(&script, &name.0).is_some() => Kind::Struct,
                    _ => return self.fail(|| "new of an unknown struct".into()),
                }
            }
            Expr::New { size, .. } => {
                self.kind(frame, &size.node)?;
                Kind::Array
            }
            Expr::ArrayLit(items) => {
                for item in items {
                    self.kind(frame, &item.node)?;
                }
                Kind::Array
            }
            Expr::MemberAccess { object, member } => {
                let owner = self.kind(frame, &object.node)?;
                if member.node.0.eq_ignore_ascii_case("length") && owner == Kind::Array {
                    return Some(Kind::Num);
                }
                if owner == Kind::Struct {
                    let Some(ty) = self.static_type(frame, e) else {
                        let member = member.node.0.clone();
                        return self.fail(|| format!("member .{member} of an untyped struct"));
                    };
                    let script = Arc::clone(&frame.script);
                    return Some(self.kind_of_declared(&script, &ty));
                }
                if owner == Kind::Quest {
                    // Another quest script's member: its declared type when
                    // the quest's script is known (an `ActorValue Property`
                    // is no number), else a numeric variable, what the VM
                    // reads from an untyped quest.
                    let declared = self
                        .declared_script(frame, &object.node)
                        .and_then(|script| self.member_type(&script, &member.node.0));
                    match declared {
                        Some(ty) => {
                            let script = Arc::clone(&frame.script);
                            self.kind_of_declared(&script, &ty)
                        }
                        None => Kind::Num,
                    }
                } else if member.node.0.eq_ignore_ascii_case("value") {
                    Kind::Num
                } else if let Some(ty) = self.engine_constant_type(&member.node.0).filter(|_| owner != Kind::None) {
                    kind_of_type(&ty, self.library)
                } else if let Some(ty) = self.reference_member_type(frame, owner, &object.node, &member.node.0) {
                    kind_of_type(&ty, self.library)
                } else {
                    let member = member.node.0.clone();
                    return self.fail(|| format!("member .{member} of {owner:?}"));
                }
            }
            Expr::Call { callee, args } => self.check_call(frame, &callee.node, args, false)?,
            Expr::Is { .. } | Expr::ParentAccess => return self.fail(|| "Is / Parent".into()),
        })
    }

    /// A call's result kind, or `None` when the VM cannot run it.
    fn check_call(&mut self, frame: &mut CheckFrame, callee: &Expr, args: &[CallArg], statement: bool) -> Option<Kind> {
        // Starfield guards: the decompiler lifts `LockGuards` / `UnlockGuards` /
        // `TryLockGuards` to `Guard.lock(...)` / `.unlock(...)` / `.trylock(...)`
        // whose arguments name guards, which are not values (see `guard_call`).
        if let Some(call) = guard_call(callee) {
            return match call {
                GuardCall::Lock | GuardCall::Unlock if statement => Some(Kind::None),
                GuardCall::TryLock => Some(Kind::Bool),
                _ => self.fail(|| "a guard call used for its value".into()),
            };
        }
        let mut arg_kinds = Vec::with_capacity(args.len());
        for arg in args {
            let kind = self.kind(frame, &arg.value.node)?;
            // A literal argument is checked as written: the run sees it too;
            // so is an engine constant, which the run passes by name.
            let constant = constant_name(&arg.value.node)
                .filter(|name| !frame.locals.contains_key(&name.to_ascii_lowercase()))
                .filter(|name| self.engine_constant_type(name).is_some());
            arg_kinds.push(match (constant, literal_of(&arg.value.node)) {
                (Some(name), _) => Arg::Constant(name.to_owned()),
                (None, Some(literal)) => Arg::Literal(literal),
                (None, None) => Arg::Kind(kind),
            });
        }
        let (receiver, name) = match callee {
            Expr::Ident(name) => (None, name.0.to_ascii_lowercase()),
            Expr::MemberAccess { object, member } => (Some(&object.node), member.node.0.to_ascii_lowercase()),
            _ => return None,
        };
        // A hit registration's aggressor filter is matched against a reference
        // or a faction, so a property or local declared as one (an `Actor`, an
        // `ObjectReference`, a `ReferenceAlias`, a `Faction`) counts as one; a
        // form of any other type (a keyword, a base object) is a filter this
        // runtime does not match.
        if name == "registerforhitevent" {
            if let (Some(arg), Some(Arg::Kind(Kind::Object))) = (args.get(1), arg_kinds.get(1)) {
                // A cast to `ScriptObject` hides the declared type: look through it.
                let mut declared = &arg.value.node;
                while let Expr::Cast { expr, .. } = declared {
                    declared = &expr.node;
                }
                let is_reference = self.static_type(frame, declared).is_some_and(|ty| match ty {
                    Type::Object(name) => {
                        let lower = name.0.to_ascii_lowercase();
                        lower == "objectreference" || lower == "actor" || lower == "faction" || lower.contains("alias")
                    }
                    _ => false,
                });
                if is_reference {
                    arg_kinds[1] = Arg::Kind(Kind::Player);
                }
            }
        }
        // Help-message housekeeping (see [`help_message_housekeeping`]).
        if statement {
            if let Some(arity) = help_message_housekeeping(callee) {
                if arg_kinds.len() != arity {
                    return self.fail(|| format!("{name} as written"));
                }
                return Some(Kind::None);
            }
        }
        // A function of an `InputEnableLayer` the receiver is declared as.
        if let (Some(native), Some(receiver)) = (layer_native(&name), receiver) {
            let mut declared = receiver;
            while let Expr::Cast { expr, .. } = declared {
                declared = &expr.node;
            }
            if matches!(
                self.static_type(frame, declared),
                Some(Type::Object(ty)) if ty.0.eq_ignore_ascii_case("inputenablelayer")
            ) {
                let flag = |arg: &Arg| matches!(arg, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool | Kind::Num));
                let shaped = match native {
                    LayerNative::Disable | LayerNative::Enable => arg_kinds.len() <= 11 && arg_kinds.iter().all(flag),
                    LayerNative::Set(_) => arg_kinds.len() <= 1 && arg_kinds.iter().all(flag),
                    LayerNative::Reset | LayerNative::Delete | LayerNative::Query(_) => arg_kinds.is_empty(),
                };
                if !shaped {
                    return self.fail(|| format!("InputEnableLayer.{name} as written"));
                }
                return Some(if matches!(native, LayerNative::Query(_)) { Kind::Bool } else { Kind::None });
            }
        }
        // `<Sound>.Play(akSource)`: the receiver must be declared a `Sound`
        // (`VisualEffect.Play` has the same name and a different meaning). The
        // call returns the playback instance id (0: "the sound failed to play");
        // here that is 1 when the sound was queued, which is right for the scripts
        // that keep it or test it and moot for the ones that would steer the
        // instance - `StopInstance` and `SetInstanceVolume` stay unsupported, so a
        // script that does is declined whole.
        if name == "play" && matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object | Kind::Player | Kind::Alias)]) {
            if let Some(receiver) = receiver {
                let mut declared = receiver;
                while let Expr::Cast { expr, .. } = declared {
                    declared = &expr.node;
                }
                if matches!(
                    self.static_type(frame, declared),
                    Some(Type::Object(ty)) if ty.0.eq_ignore_ascii_case("sound")
                ) {
                    return Some(if statement { Kind::None } else { Kind::Num });
                }
            }
        }
        // `<MusicType>.Add()` / `.Remove()` as a statement (see the runtime arm).
        if matches!(name.as_str(), "add" | "remove") && statement && arg_kinds.is_empty()
            && receiver.is_some_and(|receiver| declares_music_type(&frame.script, receiver)) {
                return Some(Kind::None);
            }
        // The frame's own script function.
        if receiver.is_none()
            || receiver.is_some_and(|r| matches!(r, Expr::Ident(id) if id.0.eq_ignore_ascii_case("self")))
        {
            let script = Arc::clone(&frame.script);
            if !self.versions(&script, &name).is_empty() {
                return self.check_call_into(&script, &name, statement, frame.self_kind);
            }
            if is_state_event(callee) {
                return Some(Kind::None);
            }
            if name == "sendcustomevent" {
                let ok = statement
                    && matches!(arg_kinds.first(), Some(Arg::Literal(Expr::StringLit(_))))
                    && match arg_kinds.get(1) {
                        None | Some(Arg::Kind(Kind::Array | Kind::None)) | Some(Arg::Literal(Expr::NoneLit)) => arg_kinds.len() <= 2,
                        _ => false,
                    };
                if !ok {
                    return self.fail(|| "SendCustomEvent as written".into());
                }
                return Some(Kind::None);
            }
            if name == "getstate" {
                return Some(Kind::Str);
            }
            // An update registration: the handler it delivers to must run.
            if super::EventRequest::is_call(&name) {
                return self.check_event_registration(&script, &name, &arg_kinds, statement, frame.self_kind);
            }
            if let Some(handler) = registration_handler(&name) {
                let interval_ok = update_args_ok(&name, &arg_kinds);
                if !statement || !interval_ok {
                    return self.fail(|| format!("{name} as written"));
                }
                if !handler.is_empty() && !self.versions(&script, handler).is_empty() {
                    self.check_handler(&script, handler, frame.self_kind)?;
                }
                return Some(Kind::None);
            }
            // CK *RegisterForAnimationEvent - Form* on the script itself: the
            // `OnAnimationEvent` it delivers must run whole (see the runtime arm).
            if matches!(name.as_str(), "registerforanimationevent" | "unregisterforanimationevent")
                && matches!(
                    arg_kinds.as_slice(),
                    [Arg::Kind(Kind::Object | Kind::Player), Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]
                )
            {
                if name == "registerforanimationevent" && !self.versions(&script, "onanimationevent").is_empty() {
                    self.check_handler(&script, "onanimationevent", frame.self_kind)?;
                }
                return Some(if name == "registerforanimationevent" { Kind::Bool } else { Kind::None });
            }
            if name == "gotostate" && statement {
                // The switch runs the state events, called for no value.
                for event in ["onendstate", "onbeginstate"] {
                    if !self.versions(&script, event).is_empty() {
                        self.check_call_into(&script, event, false, frame.self_kind)?;
                    }
                }
                return Some(Kind::None);
            }
        }
        let Some(receiver) = receiver else {
            return self.fail(|| format!("global {name}"));
        };
        // `Parent.Fn()`: the version past the script this body is defined in.
        if matches!(receiver, Expr::ParentAccess) {
            let defined = Arc::clone(&frame.defined_in);
            let parent = defined
                .parent
                .as_ref()
                .and_then(|parent| self.library.and_then(|library| library.script(&parent.node.0)));
            if let Some(parent) = parent {
                self.scripts.insert(parent.name.node.0.to_ascii_lowercase(), Arc::clone(&parent));
                if !is_engine_class(&parent) && !self.versions(&parent, &name).is_empty() {
                    return self.check_call_into(&parent, &name, statement, frame.self_kind);
                }
            }
            // An engine class's event has no body to run.
            if statement && is_event_name(&callee_name(callee)) {
                return Some(Kind::None);
            }
            return self.fail(|| format!("Parent.{name}"));
        }
        // `ScriptName.Fn()`: a global function of a script the library has.
        if let Expr::Ident(script_name) = receiver {
            let key = script_name.0.to_ascii_lowercase();
            let is_value = key == "self"
                || frame.locals.contains_key(&key)
                || static_class(receiver).is_some()
                || {
                    let script = Arc::clone(&frame.script);
                    self.member_type(&script, &script_name.0).is_some()
                };
            if !is_value {
                if let Some(script) = self.library.and_then(|library| library.script(&script_name.0)) {
                    if !is_engine_class(&script) && !self.versions(&script, &name).is_empty() {
                        self.scripts.insert(key, Arc::clone(&script));
                        return self.check_call_into(&script, &name, statement, Kind::None);
                    }
                }
            }
        }
        if let Some(class) = static_class(receiver) {
            if class == "game" && name == "getplayer" {
                return Some(Kind::Player);
            }
            let numeric_arg = |a: &Arg| matches!(a, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num));
            if class == "game"
                && name == "shakecamera"
                && statement
                && arg_kinds.len() <= 3
                && arg_kinds.first().is_none_or(|a| {
                    matches!(a, Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit))
                })
                && arg_kinds.iter().skip(1).all(numeric_arg)
            {
                return Some(Kind::None);
            }
            if class == "game" && name == "shakecontroller" && statement && arg_kinds.len() == 3 && arg_kinds.iter().all(numeric_arg) {
                return Some(Kind::None);
            }
            if class == "game"
                && name == "stopdialoguecamera"
                && statement
                && arg_kinds.len() <= 2
                && arg_kinds.iter().all(|a| matches!(a, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool)))
            {
                return Some(Kind::None);
            }
            if class == "game" && matches!(name.as_str(), "getcaps" | "getcredits") && arg_kinds.is_empty() {
                return Some(Kind::Object);
            }
            if class == "game"
                && name == "getformfromfile"
                && matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num), Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)])
            {
                return Some(Kind::Object);
            }
            if class == "debug" && (name.starts_with("trace") || name == "openuserlog" || name == "closeuserlog") && statement {
                return Some(Kind::None);
            }
            if class == "game"
                && name == "getform"
                && matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num)])
            {
                return Some(Kind::Object);
            }
            if class == "weather" && name == "getcurrentweather" && arg_kinds.is_empty() {
                return Some(Kind::Object);
            }
            if class == "math" && MATH_NATIVES.contains(&name.as_str()) {
                let numeric = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num));
                let arity = if matches!(name.as_str(), "min" | "max" | "pow") { 2 } else { 1 };
                if arg_kinds.len() == arity && arg_kinds.iter().all(numeric) {
                    return Some(Kind::Num);
                }
                return self.fail(|| format!("Math.{name} as written"));
            }
            if class == "utility"
                && name == "randomfloat"
                && arg_kinds.len() <= 2
                && arg_kinds.iter().all(|a| matches!(a, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num)))
            {
                return Some(Kind::Num);
            }
            if class == "utility" && matches!(name.as_str(), "getcurrentgametime" | "getcurrentrealtime") && arg_kinds.is_empty() {
                return Some(Kind::Num);
            }
            if class == "utility" && name == "wait" {
                if statement && matches!(arg_kinds.as_slice(), [Arg::Literal(_)] | [Arg::Kind(Kind::Num)]) {
                    self.waits = true;
                    return Some(Kind::None);
                }
                return self.fail(|| "Utility.Wait called for its value".into());
            }
            let result = self.check_native(receiver.clone(), Kind::None, &name, &arg_kinds, statement);
            if result.is_none() {
                return self.fail(|| {
                    // A literal string argument names what was asked for
                    // (`Game.QueryStat("Bribes")`), which the census groups by.
                    let named = arg_kinds.iter().find_map(|arg| match arg {
                        Arg::Literal(Expr::StringLit(text)) => Some(format!(" \"{text}\"")),
                        _ => None,
                    });
                    format!("native {class}.{name}/{}{}", arg_kinds.len(), named.unwrap_or_default())
                });
            }
            return result;
        }
        // A static native on an *engine* class the corpus spells inline —
        // `InputEnableLayer.GetOrCreate(3)` (the FO4+/SF player-controls
        // idiom), `Message.Show(…)`. `static_class` above covers only the
        // four global-function classes; the engine's other builtin types
        // ride the archive as native-only stubs, so the Ident names the
        // CLASS, not a value: it has no local/member binding, and `kind`
        // below would fail the whole check as `unknown name <class>` —
        // even though the stub being an engine class is exactly what says
        // "no script body to run; this is a native". Route it through the
        // same placeholder-native classification any other engine receiver
        // gets; the effect table decides, exactly as for `Game.*`.
        if let Expr::Ident(id) = receiver {
            let key = id.0.to_ascii_lowercase();
            if key == "inputenablelayer" && name == "create" && arg_kinds.is_empty() && !frame.locals.contains_key(&key) {
                return Some(Kind::Object);
            }
            let names_a_value = frame.locals.contains_key(&key)
                || id.0.eq_ignore_ascii_case("self")
                || is_state_variable(&id.0)
                || self.member_type(&frame.script, &id.0).is_some();
            if !names_a_value
                && self
                    .library
                    .and_then(|library| library.script(&key))
                    .is_some_and(|script| is_engine_class(&script))
            {
                let result =
                    self.check_native(receiver.clone(), Kind::None, &name, &arg_kinds, statement);
                if result.is_none() {
                    return self.fail(|| format!("static native {name}/{}", arg_kinds.len()));
                }
                return result;
            }
        }
        let receiver_kind = self.kind(frame, receiver)?;
        if receiver_kind == Kind::Array {
            return self.check_array_call(&name, &arg_kinds, statement);
        }
        // A script-filled `RefCollectionAlias` (declared as one).
        if receiver_kind == Kind::Alias && COLLECTION_NATIVES.contains(&name.as_str()) {
            let collection = self
                .static_type(frame, receiver)
                .is_some_and(|ty| matches!(&ty, Type::Object(n) if n.0.eq_ignore_ascii_case("RefCollectionAlias")));
            if collection {
                let object = |arg: &Arg| matches!(arg, Arg::Kind(Kind::Object | Kind::Player | Kind::Alias));
                let ok = match name.as_str() {
                    "getcount" | "getarray" | "getactorarray" => arg_kinds.is_empty(),
                    "enableall" | "disableall" => {
                        statement && matches!(arg_kinds.as_slice(), [] | [Arg::Literal(Expr::BoolLit(_))])
                    }
                    "evaluateall" => statement && arg_kinds.is_empty(),
                    "getat" => matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num)]),
                    "find" => arg_kinds.len() == 1 && object(&arg_kinds[0]),
                    "addref" | "removeref" => statement && arg_kinds.len() == 1 && object(&arg_kinds[0]),
                    "addarray" => statement && matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Array)]),
                    "addrefcollection" => statement && matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Alias)]),
                    "removeall" => statement && arg_kinds.is_empty(),
                    _ => false,
                };
                if !ok {
                    return self.fail(|| format!("collection {name} as written"));
                }
                return Some(match name.as_str() {
                    "getcount" | "find" => Kind::Num,
                    "getat" => Kind::Object,
                    "getarray" | "getactorarray" => Kind::Array,
                    _ => Kind::None,
                });
            }
        }
        match (receiver_kind, name.as_str()) {
            (Kind::Alias, "getref" | "getreference" | "getactorref" | "getactorreference" | "getshipref" | "getshipreference") => {
                return Some(Kind::Object);
            }
            (_, "getowningquest") => return Some(Kind::Quest),
            (
                Kind::Object | Kind::Player,
                "getlinkedref" | "getbaseobject" | "getactorbase" | "getleveledactorbase" | "getcurrentlocation"
                | "getdialoguetarget",
            ) => {
                return Some(Kind::Object);
            }
            (Kind::Object | Kind::Player, "getcurrentpackage") if arg_kinds.is_empty() => return Some(Kind::Object),
            (Kind::Object | Kind::Player, "getlinkedrefchain")
                if arg_kinds.len() <= 2
                    && arg_kinds.first().is_none_or(|keyword| {
                        matches!(keyword, Arg::Literal(Expr::NoneLit) | Arg::Kind(Kind::Object))
                    })
                    && arg_kinds
                        .get(1)
                        .is_none_or(|max| matches!(max, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num))) =>
            {
                return Some(Kind::Array);
            }
            (Kind::Object | Kind::Player, "isindialoguewithplayer") if arg_kinds.is_empty() => return Some(Kind::Bool),
            (Kind::Object, "isloaded") if arg_kinds.is_empty() => return Some(Kind::Bool),
            (Kind::Object | Kind::Player, "setactorowner" | "setfactionowner")
                if statement && matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object, "setcrimegold" | "setcrimegoldviolent")
                if statement && matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object, "modcrimegold")
                if statement
                    && matches!(arg_kinds.first(), Some(Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num)))
                    && arg_kinds.get(1).is_none_or(|a| matches!(a, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool)))
                    && arg_kinds.len() <= 2 =>
            {
                return Some(Kind::None);
            }
            (Kind::Object, "getcrimegold" | "getcrimegoldviolent" | "getcrimegoldnonviolent") if arg_kinds.is_empty() => {
                return Some(Kind::Num);
            }
            (Kind::Object, "getsex") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object | Kind::Player, "getvoicetype") if arg_kinds.is_empty() => return Some(Kind::Object),
            (Kind::Object | Kind::Player, "isinlocation") if matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object)]) => {
                return Some(Kind::Bool);
            }
            (Kind::Object | Kind::Player, "dropobject")
                if matches!(arg_kinds.first(), Some(Arg::Kind(Kind::Object | Kind::Player)))
                    && arg_kinds.get(1).is_none_or(|count| matches!(count, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num)))
                    && arg_kinds.len() <= 2 =>
            {
                return Some(Kind::Object);
            }
            (Kind::Object | Kind::Player, "isfurnitureinuse")
                if arg_kinds.len() <= 1
                    && arg_kinds.first().is_none_or(|arg| matches!(arg, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool))) =>
            {
                return Some(Kind::Bool);
            }
            (Kind::Object | Kind::Player, "getcrimefaction") if arg_kinds.is_empty() => return Some(Kind::Object),
            (Kind::Quest, "isstarting" | "isstopping") if arg_kinds.is_empty() => return Some(Kind::Bool),
            (Kind::Object | Kind::Player, "countlinkedrefchain")
                if arg_kinds.len() <= 2
                    && arg_kinds.first().is_none_or(|keyword| {
                        matches!(keyword, Arg::Literal(Expr::NoneLit) | Arg::Kind(Kind::Object))
                    })
                    && arg_kinds
                        .get(1)
                        .is_none_or(|max| matches!(max, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num))) =>
            {
                return Some(Kind::Num);
            }
            (Kind::Object | Kind::Player, "getlinkedrefchildren")
                if matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::NoneLit) | Arg::Kind(Kind::Object)]) =>
            {
                return Some(Kind::Array);
            }
            (Kind::Object | Kind::Player, "getbribeamount" | "getgoldamount") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object | Kind::Player, "hasspell")
                if matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit)]) =>
            {
                return Some(Kind::Bool);
            }
            (Kind::Object | Kind::Player, "processtraphit")
                if statement
                    && arg_kinds.len() == 11
                    && matches!(arg_kinds[0], Arg::Kind(Kind::Object | Kind::Player))
                    && arg_kinds[1..]
                        .iter()
                        .all(|a| matches!(a, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num))) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "createdetectionevent")
                if statement
                    && (1..=2).contains(&arg_kinds.len())
                    && matches!(
                        arg_kinds[0],
                        Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit)
                    )
                    && arg_kinds
                        .get(1)
                        .is_none_or(|a| matches!(a, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num))) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "fire")
                if statement
                    && (1..=2).contains(&arg_kinds.len())
                    && matches!(arg_kinds[0], Arg::Kind(Kind::Object | Kind::Player))
                    && arg_kinds
                        .get(1)
                        .is_none_or(|a| matches!(a, Arg::Kind(Kind::Object | Kind::None) | Arg::Literal(Expr::NoneLit))) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "damageobject")
                if statement
                    && matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "getcurrentdestructionstage") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object | Kind::Player, "cleardestruction") if statement && arg_kinds.is_empty() => return Some(Kind::None),
            (Kind::Object | Kind::Player, "setdestroyed")
                if statement
                    && matches!(arg_kinds.as_slice(), [] | [Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "isdestroyed") if arg_kinds.is_empty() => return Some(Kind::Bool),
            (Kind::Object | Kind::Player, "isbribed" | "isintimidated") if arg_kinds.is_empty() => return Some(Kind::Bool),
            (Kind::Object | Kind::Player, "isscanned") if arg_kinds.is_empty() => return Some(Kind::Bool),
            (Kind::Object | Kind::Player, "applyhavokimpulse")
                if statement
                    && arg_kinds.len() == 4
                    && arg_kinds
                        .iter()
                        .all(|a| matches!(a, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num))) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object, "istrue")
                if matches!(
                    arg_kinds.as_slice(),
                    [Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit), Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit)]
                ) =>
            {
                return Some(Kind::Bool);
            }
            (Kind::Object | Kind::Player, "setscanned" | "addtomapscanned" | "setmarkervisibleonstarmap")
                if statement && matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "modfavorpoints")
                if statement && matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "modfavorpointswithglobal")
                if statement && matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "makeplayerfriend") if statement && arg_kinds.is_empty() => {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "setbribed" | "setintimidated")
                if statement
                    && arg_kinds.len() <= 1
                    && arg_kinds.iter().all(|a| matches!(a, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool))) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "changeanimarchetype")
                if arg_kinds.len() <= 1
                    && arg_kinds.iter().all(|a| {
                        matches!(a, Arg::Literal(Expr::NoneLit) | Arg::Kind(Kind::Object | Kind::None))
                    }) =>
            {
                return Some(Kind::Bool);
            }
            // The animation-graph natives (see `animation_graph`): the CK's
            // failure results, since no behavior graph runs.
            (Kind::Object | Kind::Player, "playanimation") if matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]) => {
                return Some(Kind::Bool);
            }
            (Kind::Object | Kind::Player, "playanimationandwait")
                if statement
                    && matches!(
                        arg_kinds.as_slice(),
                        [Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str), Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]
                    ) =>
            {
                // Latent: it waits for the graph's event, so only as a statement.
                self.waits = true;
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "waitforanimationevent")
                if statement && matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]) =>
            {
                // Latent, like PlayAnimationAndWait: only as a statement.
                self.waits = true;
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "registerforanimationevent" | "unregisterforanimationevent")
                if matches!(
                    arg_kinds.as_slice(),
                    [Arg::Kind(Kind::Object | Kind::Player), Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]
                ) =>
            {
                return Some(if name == "registerforanimationevent" { Kind::Bool } else { Kind::None });
            }
            (Kind::Object | Kind::Player, "setanimationvariablefloat" | "setanimationvariableint")
                if statement
                    && matches!(
                        arg_kinds.as_slice(),
                        [
                            Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str),
                            Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num)
                        ]
                    ) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "getanimationvariablefloat" | "getanimationvariableint")
                if matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]) =>
            {
                return Some(Kind::Num);
            }
            (Kind::Object | Kind::Player, "getanimationvariablebool")
                if matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]) =>
            {
                return Some(Kind::Bool);
            }
            (Kind::Object | Kind::Player, "setanimationvariablebool")
                if statement
                    && matches!(
                        arg_kinds.as_slice(),
                        [Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str), Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool)]
                    ) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "calculateencounterlevel")
                if arg_kinds.len() <= 1
                    && arg_kinds
                        .first()
                        .is_none_or(|difficulty| matches!(difficulty, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num))) =>
            {
                return Some(Kind::Num);
            }
            (Kind::Object | Kind::Player, "getfactionreaction")
                if matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object | Kind::Player)]) =>
            {
                return Some(Kind::Num);
            }
            (Kind::Object | Kind::Player, "getnthlinkedref")
                if match arg_kinds.as_slice() {
                    [hops] | [hops, _] => {
                        matches!(hops, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num))
                            && arg_kinds.get(1).is_none_or(|keyword| {
                                matches!(keyword, Arg::Literal(Expr::NoneLit) | Arg::Kind(Kind::Object))
                            })
                    }
                    _ => false,
                } =>
            {
                return Some(Kind::Object);
            }
            (Kind::Alias, "getlocation") => return Some(Kind::Object),
            (Kind::Alias, "trytogetcurrentlocation") if arg_kinds.is_empty() => return Some(Kind::Object),
            (Kind::Object, "getclassification") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object, "getsize") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object, "gettriggerobjectcount") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object | Kind::Player, "getrace") if arg_kinds.is_empty() => return Some(Kind::Object),
            (Kind::Object | Kind::Player, "getscale") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object | Kind::Player, "setnofavorallowed")
                if statement && arg_kinds.len() <= 1 && arg_kinds.iter().all(|a| matches!(a, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool))) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "ignorefriendlyhits" | "setcandocommand")
                if statement && arg_kinds.len() <= 1 && arg_kinds.iter().all(|a| matches!(a, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool))) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "setactorcause")
                if statement && matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object, "getfactionowner" | "getactorowner" | "geteditorlocation") if arg_kinds.is_empty() => {
                return Some(Kind::Object);
            }
            (Kind::Object | Kind::Player, "enablelinkchain" | "disablelinkchain")
                if statement && arg_kinds.len() <= 2 && arg_kinds.iter().all(|a| matches!(a, Arg::Kind(Kind::Object | Kind::Bool) | Arg::Literal(Expr::NoneLit | Expr::BoolLit(_)))) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object, "getat") if matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num)]) => {
                return Some(Kind::Object);
            }
            (Kind::Object, "addform" | "removeaddedform")
                if statement && matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object | Kind::Player | Kind::Quest)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object, "revert") if statement && arg_kinds.is_empty() => return Some(Kind::None),
            (Kind::Object, "hasform") if matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object | Kind::Player | Kind::Quest)]) => {
                return Some(Kind::Bool);
            }
            (Kind::Object, "find") if matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object | Kind::Player)]) => {
                return Some(Kind::Num);
            }
            (Kind::Object | Kind::Player, "ischild")
                if matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object | Kind::Player)]) =>
            {
                return Some(Kind::Bool);
            }
            (Kind::Object | Kind::Player, "placeatme") if place_at_me_args_ok(&arg_kinds) => {
                return Some(Kind::Object);
            }
            // The statement form of CK *UpdateCurrentInstanceGlobal - Quest*,
            // which has no effect here: see `call_statement`.
            (Kind::Quest, "updatecurrentinstanceglobal")
                if statement && matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object)]) =>
            {
                return Some(Kind::None);
            }
            _ => {}
        }
        // Another quest's update registration: its handler must run.
        if receiver_kind == Kind::Quest {
            if super::EventRequest::is_call(&name) {
                let Some(script) = self.declared_script(frame, receiver) else {
                    return self.fail(|| format!("{name} on a quest of unknown script"));
                };
                return self.check_event_registration(&script, &name, &arg_kinds, statement, Kind::Quest);
            }
            if let Some(handler) = registration_handler(&name) {
                let interval_ok = update_args_ok(&name, &arg_kinds);
                if !statement || !interval_ok {
                    return self.fail(|| format!("{name} as written"));
                }
                if !handler.is_empty() {
                    let Some(script) = self.declared_script(frame, receiver) else {
                        return self.fail(|| format!("{name} on a quest of unknown script"));
                    };
                    if !self.versions(&script, handler).is_empty() {
                        self.check_handler(&script, handler, Kind::Quest)?;
                    }
                }
                return Some(Kind::None);
            }
        }
        // A function of a script on another reference, through a cast or a
        // typed variable: it runs as that reference's.
        if receiver_kind == Kind::Object {
            if let Some(script) = self.declared_script(frame, receiver) {
                if !is_engine_class(&script) && !self.versions(&script, &name).is_empty() {
                    // A script on an alias (`Alias_X as MyAliasScript`) runs with an
                    // alias as `Self`, so its `GetActorRef()` and the like read.
                    let self_kind = if self.script_extends_alias(&script) { Kind::Alias } else { Kind::Object };
                    return self.check_call_into(&script, &name, statement, self_kind);
                }
            }
        }
        // A quest script's function, through a cast or a typed property.
        if receiver_kind == Kind::Quest {
            if let Some(script) = self.quest_script_for(frame, receiver, &name) {
                return self.check_call_into(&script, &name, statement, Kind::Quest);
            }
        }
        let Some(placeholder) = spanned_placeholder(receiver_kind) else {
            return self.fail(|| format!("{name} on {receiver_kind:?}"));
        };
        let result = self.check_native(placeholder, receiver_kind, &name, &arg_kinds, statement);
        if result.is_none() {
            return self.fail(|| {
                let shapes: Vec<String> = arg_kinds
                    .iter()
                    .map(|arg| match arg {
                        Arg::Literal(_) => "lit".to_string(),
                        Arg::Kind(kind) => format!("{kind:?}"),
                        Arg::Constant(_) => "const".to_string(),
                    })
                    .collect();
                format!("native {name}/{} on {receiver_kind:?} ({})", arg_kinds.len(), shapes.join(","))
            });
        }
        result
    }

    /// The script a quest-typed receiver's `name` lives in: a cast's target
    /// type, or the declared type of a property / local.
    fn quest_script_for(&mut self, frame: &CheckFrame, receiver: &Expr, name: &str) -> Option<Arc<Script>> {
        let script = self.declared_script(frame, receiver)?;
        if self.versions(&script, name).is_empty() {
            return None;
        }
        Some(script)
    }

    /// The script a receiver is declared as — a cast's target type, or a
    /// property's / local's type — registered for the run.
    /// Whether `script` extends an alias class through its chain.
    fn script_extends_alias(&mut self, script: &Arc<Script>) -> bool {
        let mut parent = script.parent.as_ref().map(|p| p.node.0.clone());
        for _ in 0..16 {
            let Some(name) = parent else {
                return false;
            };
            let lower = name.to_ascii_lowercase();
            if lower.contains("alias") {
                return true;
            }
            parent = self
                .library
                .and_then(|library| library.script(&lower))
                .and_then(|script| script.parent.as_ref().map(|p| p.node.0.clone()));
        }
        false
    }

    fn declared_script(&mut self, frame: &CheckFrame, receiver: &Expr) -> Option<Arc<Script>> {
        let type_name = match receiver {
            Expr::Cast { target_type, .. } => match &target_type.node {
                Type::Object(ty) => ty.0.clone(),
                _ => return None,
            },
            Expr::Ident(id) if id.0.eq_ignore_ascii_case("self") => frame.script.name.node.0.clone(),
            Expr::Ident(id) => match frame
                .types
                .get(&id.0.to_ascii_lowercase())
                .cloned()
                .or_else(|| self.member_type(&frame.script, &id.0))
            {
                Some(Type::Object(ty)) => ty.0.clone(),
                _ => return None,
            },
            // `Owner.Member`: the script type `Member` is declared as in
            // `Owner`'s script (`(GetOwningQuest() as REScript).REParent`).
            Expr::MemberAccess { object, member } => {
                let owner = self.declared_script(frame, &object.node)?;
                match self.member_type(&owner, &member.node.0)? {
                    Type::Object(ty) => ty.0,
                    _ => return None,
                }
            }
            _ => return None,
        };
        let script = self.library?.script(&type_name)?;
        self.scripts
            .insert(script.name.node.0.to_ascii_lowercase(), Arc::clone(&script));
        Some(script)
    }

    /// Whether the primitives take this native call, over placeholders.
    fn check_native(
        &mut self,
        receiver: Expr,
        _receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        let mut bindings = NativeBindings::default();
        let name_is_help_message = name.eq_ignore_ascii_case("showashelpmessage");
        let placeholder = |kind: Kind, index: usize, bindings: &mut NativeBindings| -> Option<Spanned<Expr>> {
            let name = format!("__vm{index}");
            Some(match kind {
                Kind::Num => spanned(Expr::IntLit(1)),
                Kind::Bool => spanned(Expr::BoolLit(false)),
                // `ShowAsHelpMessage`'s context is shown only when empty or
                // `"NoMenu"`; a run-time one is checked as the unrestricted
                // `""` (a menu name stops the run when it arrives).
                Kind::Str if name_is_help_message && index == 5 => spanned(Expr::StringLit(String::new())),
                Kind::Str => spanned(Expr::StringLit("x".into())),
                Kind::None => spanned(Expr::NoneLit),
                Kind::Player => {
                    bindings.players.insert(name.clone());
                    ident(&name)
                }
                Kind::Quest => {
                    bindings.quests.insert(name.clone());
                    ident(&name)
                }
                Kind::Object | Kind::Alias => ident(&name),
                Kind::Array | Kind::Struct => return None,
            })
        };
        let receiver = match receiver {
            Expr::Ident(id) if id.0.starts_with("__vm_kind_") => {
                let kind = match id.0.as_str() {
                    "__vm_kind_quest" => Kind::Quest,
                    "__vm_kind_player" => Kind::Player,
                    _ => Kind::Object,
                };
                placeholder(kind, 0, &mut bindings)?
            }
            other => spanned(other),
        };
        let mut args = Vec::with_capacity(arg_kinds.len());
        for (index, arg) in arg_kinds.iter().enumerate() {
            args.push(CallArg {
                name: None,
                value: match arg {
                    // Skyrim's raw motion type is checked as the constant it
                    // names; the run maps it by the game (`synthesize`).
                    Arg::Literal(Expr::IntLit(raw))
                        if index == 0 && name.eq_ignore_ascii_case("setmotiontype") && skyrim_motion_constant(*raw as i32).is_some() =>
                    {
                        spanned(Expr::MemberAccess {
                            object: Box::new(receiver.clone()),
                            member: spanned(Identifier(skyrim_motion_constant(*raw as i32).unwrap_or_default().to_owned())),
                        })
                    }
                    Arg::Literal(literal) => spanned(literal.clone()),
                    Arg::Kind(kind) => placeholder(*kind, index + 1, &mut bindings)?,
                    Arg::Constant(name) => spanned(Expr::MemberAccess {
                        object: Box::new(receiver.clone()),
                        member: spanned(Identifier(name.clone())),
                    }),
                },
            });
        }
        let call = Expr::Call {
            callee: Box::new(spanned(Expr::MemberAccess {
                object: Box::new(receiver),
                member: spanned(Identifier(name.to_owned())),
            })),
            args,
        };
        if statement {
            if let Some(effect) = classify_native_effect(&call, &bindings) {
                return (!crate::translate::effects::is_latent(&effect)).then_some(Kind::None);
            }
        }
        if classify_native_value(&call, &bindings).is_some() {
            return Some(Kind::Num);
        }
        if classify_native_guard(&call, &bindings).is_some() {
            return Some(Kind::Bool);
        }
        None
    }
}

/// The aggressor a hit registration filters on, when the filter is one this
/// runtime matches: a reference (the check proved the filter's declared type is
/// one) or an alias.
fn target_of_aggressor(value: &Value) -> Option<super::DistanceTarget> {
    match value {
        Value::Form(form) if *form != 0 => Some(super::DistanceTarget::Reference(*form)),
        Value::Alias { quest, alias } => Some(super::DistanceTarget::Alias {
            quest: *quest,
            alias: *alias,
        }),
        _ => None,
    }
}

/// An update or timer call's arguments: numbers, at most as many as it
/// takes, and the interval when it starts one.
fn update_args_ok(name: &str, args: &[Arg]) -> bool {
    // A hit registration: the target (a reference, the player or an alias), an
    // aggressor filter that is `None`, the player or an alias, `None` source and
    // projectile filters, four numbers and the `abMatch` Bool.
    if super::HitRequest::handler(name).is_some() {
        let object = |arg: &Arg| matches!(arg, Arg::Kind(Kind::Object | Kind::Player | Kind::Alias));
        let unset = |arg: &Arg| matches!(arg, Arg::Literal(Expr::NoneLit));
        let number = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num));
        let flag = |arg: &Arg| matches!(arg, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool));
        return match name {
            "unregisterforallhitevents" => args.is_empty(),
            "unregisterforhitevent" => args.len() == 1 && object(&args[0]),
            _ => {
                !args.is_empty()
                    && args.len() <= 9
                    && object(&args[0])
                    && args.get(1).is_none_or(|a| unset(a) || matches!(a, Arg::Kind(Kind::Player | Kind::Alias)))
                    && args.get(2).is_none_or(unset)
                    && args.get(3).is_none_or(unset)
                    && args.iter().skip(4).take(4).all(number)
                    && args.get(8).is_none_or(flag)
            }
        };
    }
    // A distance registration: two objects (a reference, a player or an alias)
    // and, registering, a number.
    if super::DistanceRequest::handler(name).is_some() {
        let object = |arg: &Arg| matches!(arg, Arg::Kind(Kind::Object | Kind::Player | Kind::Alias));
        let number = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num));
        let arity = super::DistanceRequest::arity(name);
        return args.len() == arity && args[..2].iter().all(object) && args.get(2).is_none_or(number);
    }
    let numeric = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num));
    let starts = !super::UpdateRequest::handler(name).unwrap_or_default().is_empty();
    args.len() <= super::UpdateRequest::max_args(name) && (!starts || !args.is_empty()) && args.iter().all(numeric)
}

/// A native call's argument as the check sees it.
enum Arg {
    Literal(Expr),
    Kind(Kind),
    /// An engine constant, passed as the receiver's named member.
    Constant(String),
}

/// Whether `PlaceAtMe`'s arguments are ones the VM runs: the form, then the
/// count, persist, initially-disabled and delete-when-able flags (Skyrim sends
/// the first four, Fallout 4 five), then Starfield's offsets, alias fill and
/// navmesh snap, where the first two must be `None` and the snap is moot.
fn place_at_me_args_ok(args: &[Arg]) -> bool {
    let unset = |arg: &Arg| matches!(arg, Arg::Literal(Expr::NoneLit));
    let flag = |arg: &Arg| matches!(arg, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool | Kind::Num));
    let count = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num));
    let Some(Arg::Kind(Kind::Object)) = args.first() else {
        return false;
    };
    match args.len() {
        1 => true,
        2..=5 => count(&args[1]) && args[2..].iter().all(flag),
        8 => count(&args[1]) && args[2..5].iter().all(flag) && unset(&args[5]) && unset(&args[6]) && flag(&args[7]),
        _ => false,
    }
}

/// The name an argument reads an engine constant by, if it is written as
/// one: `X.Motion_Keyframed` or a bare `Motion_Keyframed`.
fn constant_name(e: &Expr) -> Option<&str> {
    match e {
        Expr::MemberAccess { member, .. } => Some(member.node.0.as_str()),
        Expr::Ident(name) if !name.0.eq_ignore_ascii_case("self") => Some(name.0.as_str()),
        _ => None,
    }
}

/// `e` as a literal (through casts and negation), if it is one.
fn literal_of(e: &Expr) -> Option<Expr> {
    match e {
        Expr::IntLit(_) | Expr::FloatLit(_) | Expr::BoolLit(_) | Expr::StringLit(_) | Expr::NoneLit => {
            Some(e.clone())
        }
        Expr::Cast { expr, target_type } => match (literal_of(&expr.node)?, &target_type.node) {
            (Expr::IntLit(i), Type::Float) => Some(Expr::FloatLit(i as f64)),
            (Expr::FloatLit(f), Type::Int) => Some(Expr::IntLit(f as i64)),
            (literal, Type::Int | Type::Float | Type::Bool | Type::String) => Some(literal),
            _ => None,
        },
        Expr::UnaryOp { op: UnaryOp::Neg, operand } => match literal_of(&operand.node)? {
            Expr::IntLit(i) => Some(Expr::IntLit(-i)),
            Expr::FloatLit(f) => Some(Expr::FloatLit(-f)),
            _ => None,
        },
        _ => None,
    }
}

/// A receiver placeholder the native check binds by kind.
fn spanned_placeholder(kind: Kind) -> Option<Expr> {
    let name = match kind {
        Kind::Quest => "__vm_kind_quest",
        Kind::Player => "__vm_kind_player",
        Kind::Object | Kind::Alias => "__vm_kind_object",
        _ => return None,
    };
    Some(Expr::Ident(Identifier(name.to_owned())))
}

#[cfg(test)]
mod tests;
