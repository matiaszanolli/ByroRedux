//! Quest natives: the owning quest and its start / stop transitions.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "getowningquest",
    "isstarting",
    "isstopping",
];

impl Vm<'_> {
    /// The quest reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_quest(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
            (Value::Alias { quest, .. }, "getowningquest") => Some(Value::Form(*quest)),
            (Value::Form(_), "getowningquest") => Some(Value::Form(frame.owner.0)),
            // CK *IsStarting* / *IsStopping - Quest*: "in the process of starting
            // up" / "shutting down". A quest here starts and stops in one step,
            // so it is never between the two states.
            (Value::Form(_), "isstarting" | "isstopping") if args.is_empty() => Some(Value::Bool(false)),
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The quest calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_quest(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (_, "getowningquest") => return Some(Kind::Quest),
            (Kind::Quest, "isstarting" | "isstopping") if arg_kinds.is_empty() => return Some(Kind::Bool),
            // The statement form of CK *UpdateCurrentInstanceGlobal - Quest*,
            // which has no effect here: see `call_statement`.
            (Kind::Quest, "updatecurrentinstanceglobal")
                if statement && matches!(arg_kinds, [Arg::Kind(Kind::Object)]) =>
            {
                return Some(Kind::None);
            }
            _ => {}
        }
        None
    }
}
