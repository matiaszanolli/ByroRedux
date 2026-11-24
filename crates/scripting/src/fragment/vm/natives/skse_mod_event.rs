//! SKSE's `ModEvent` handle builder (LMC-SCR-004 catalog growth, `docs/engine/extender-catalog.md` §11):
//! `Create(eventName)` answers a handle, `Push{Bool,Int,Float,String,Form}(handle, value)` appends an argument,
//! `Send(handle)` sends the event with those arguments and answers true, `Release(handle)` drops it.
//!
//! The store is [`crate::fragment::VmModEventBuilders`], the SDK's builder table (bounded at 64 builders and 128
//! arguments, `0` the failure handle); the behaviour is SKSE64's `PapyrusModEvent.cpp` (see
//! [`crate::fragment::mod_events`]). A send publishes the built event under the calling script's source principal,
//! after the run's guards drop.

use super::*;
use crate::fragment::mod_events::{ModEventPayload, ModEventRequest, ModEventSend, VmModEventBuilders};
use byroredux_sdk::event::LegacySkseModEventValue;

/// The method names the other-object registrations read (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &["registerformodevent", "unregisterformodevent", "unregisterforallmodevents"];

/// The natives, lowercase, with their arguments' kinds (after the handle) and what each returns.
const NATIVES: &[(&str, &[Slot], Kind)] = &[
    ("create", &[], Kind::Num),
    ("send", &[], Kind::Bool),
    ("release", &[], Kind::None),
    ("pushbool", &[Slot::Flag], Kind::None),
    ("pushint", &[Slot::Number], Kind::None),
    ("pushfloat", &[Slot::Number], Kind::None),
    ("pushstring", &[Slot::Text], Kind::None),
    ("pushform", &[Slot::Form], Kind::None),
];

/// What an argument after the handle may be.
#[derive(Clone, Copy)]
enum Slot {
    Flag,
    Number,
    Text,
    Form,
}

impl Vm<'_> {
    /// A `ModEvent.<name>(…)` call.
    pub(in crate::fragment::vm) fn read_mod_event_builder(&mut self, frame: &mut Frame, name: &str, args: &[CallArg]) -> Run<Option<Value>> {
        let values = self.eval_args(frame, args)?;
        let world = self.world;
        if world.try_resource::<VmModEventBuilders>().is_none() {
            return stop("ModEvent without the builder store");
        }
        if name == "create" {
            let event = match values.first() {
                Some(Value::Str(event)) => event.clone(),
                _ => return stop("ModEvent.Create without an event name"),
            };
            let handle = world.resource_mut::<VmModEventBuilders>().0.create(&event);
            return Ok(Some(Value::Int(handle as i32)));
        }
        // SKSE's handles are positive `SInt32`; anything else is "Invalid handle." and ignored.
        let handle = values.first().and_then(Value::int).filter(|handle| *handle > 0).map(|handle| handle as u32);
        let Some(handle) = handle else {
            return Ok(Some(if name == "send" { Value::Bool(false) } else { Value::None }));
        };
        Ok(Some(match name {
            "release" => {
                world.resource_mut::<VmModEventBuilders>().0.release(handle);
                Value::None
            }
            "send" => {
                let command = world.resource_mut::<VmModEventBuilders>().0.send(handle);
                match command {
                    Some(command) => {
                        let script = frame.script.name.node.0.clone();
                        self.deferred.mod_event_sends.push(ModEventSend { script, owner: frame.owner.0, payload: ModEventPayload::Built(command) });
                        Value::Bool(true)
                    }
                    None => Value::Bool(false),
                }
            }
            _ => {
                let value = match (name, values.get(1)) {
                    ("pushbool", Some(value)) => LegacySkseModEventValue::Bool(value.truthy()),
                    ("pushint", Some(value)) => LegacySkseModEventValue::Int(value.int().or_else(|| value.number().map(|n| n as i32)).unwrap_or(0)),
                    ("pushfloat", Some(value)) => LegacySkseModEventValue::float(value.number().unwrap_or(0.0)),
                    ("pushstring", Some(Value::Str(text))) => LegacySkseModEventValue::String(text.clone()),
                    ("pushstring", _) => LegacySkseModEventValue::String(String::new()),
                    ("pushform", Some(Value::Form(form))) => {
                        let form_ref = world.try_resource::<crate::LoadOrderIdentity>().and_then(|identity| identity.form_ref(*form));
                        LegacySkseModEventValue::Form(form_ref)
                    }
                    ("pushform", _) => LegacySkseModEventValue::Form(None),
                    _ => return stop(format!("ModEvent.{name} without its value")),
                };
                world.resource_mut::<VmModEventBuilders>().0.push(handle, value);
                Value::None
            }
        }))
    }
}

impl Vm<'_> {
    /// `<object>.RegisterForModEvent(event, callback)` / `UnregisterForModEvent` / `UnregisterForAllModEvents` on
    /// another quest, reference or alias (a helper function's `akReceiver`), registered on that object as SKSE's
    /// handle-based registration is. An alias without scripts of its own has no owner id, so nothing could hear the
    /// event and the call changes nothing; a `None` receiver is the hub's.
    pub(in crate::fragment::vm) fn read_mod_event_other(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        if !READS.contains(&name) {
            return Ok(None);
        }
        let target = match owner {
            Value::Form(form) if *form != 0 => Some(*form),
            Value::Alias { quest, alias } => {
                self.world.try_resource::<crate::fragment::AliasScriptOwners>().and_then(|owners| owners.owner_of(QuestFormId(*quest), *alias))
            }
            _ => return Ok(None),
        };
        let values = self.eval_args(frame, args)?;
        let text = |index: usize| match values.get(index) {
            Some(Value::Str(text)) => Some(text.clone()),
            _ => None,
        };
        let request = match name {
            "registerformodevent" => match (text(0), text(1)) {
                (Some(event), Some(callback)) => ModEventRequest::Register { event, callback },
                _ => return stop("RegisterForModEvent without its event and callback"),
            },
            "unregisterformodevent" => match text(0) {
                Some(event) => ModEventRequest::Unregister { event },
                None => return stop("UnregisterForModEvent without its event"),
            },
            _ => ModEventRequest::UnregisterAll,
        };
        if let Some(target) = target {
            self.deferred.mod_event_requests.push((target, request));
        }
        Ok(Some(Value::None))
    }
}

impl Checker<'_> {
    /// Whether the VM runs a mod-event registration on another quest, reference or alias.
    pub(in crate::fragment::vm) fn check_mod_event_other(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        if !READS.contains(&name) || !matches!(receiver_kind, Kind::Object | Kind::Player | Kind::Quest | Kind::Alias) {
            return None;
        }
        let text = |arg: &Arg| matches!(arg, Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str));
        let shaped = match (name, arg_kinds) {
            ("registerformodevent", [event, callback]) => text(event) && text(callback),
            ("unregisterformodevent", [event]) => text(event),
            ("unregisterforallmodevents", []) => true,
            _ => false,
        };
        if !statement || !shaped {
            return self.fail(|| format!("{name} on another object as written"));
        }
        Some(Kind::None)
    }

    /// Whether the VM runs `ModEvent.<name>` with these arguments.
    pub(in crate::fragment::vm) fn check_mod_event_builder(&mut self, name: &str, arg_kinds: &[Arg], statement: bool) -> Option<Kind> {
        let number = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num));
        let text = |arg: &Arg| matches!(arg, Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str));
        let Some((_, slots, kind)) = NATIVES.iter().find(|(native, ..)| *native == name) else {
            return self.fail(|| format!("ModEvent.{name}"));
        };
        let ok = if name == "create" {
            matches!(arg_kinds, [event] if text(event))
        } else {
            arg_kinds.len() == slots.len() + 1
                && number(&arg_kinds[0])
                && slots.iter().zip(&arg_kinds[1..]).all(|(slot, arg)| match slot {
                    Slot::Flag => matches!(arg, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool | Kind::Num)),
                    Slot::Number => number(arg),
                    Slot::Text => text(arg),
                    Slot::Form => matches!(arg, Arg::Literal(Expr::NoneLit) | Arg::Kind(Kind::Object | Kind::Player | Kind::Quest | Kind::None)),
                })
        };
        // A native with no result (`Release`, `Push*`) is called as a statement.
        if !ok || (*kind == Kind::None && !statement) {
            return self.fail(|| format!("ModEvent.{name} as written"));
        }
        Some(*kind)
    }
}
