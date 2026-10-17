//! `FormList` natives: its forms as a script reads and edits them.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "getsize",
    "getat",
    "hasform",
    "find",
    "addform",
    "removeaddedform",
    "revert",
];

impl Vm<'_> {
    /// The forms of the `FLST` `list`, if it is one.
    pub(in crate::fragment::vm) fn form_list(&self, list: u32) -> Option<Vec<u32>> {
        crate::forms_of(self.world, list, &self.deferred.form_list_changes)
    }

    /// The form-list reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_form_list(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
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
                Some(match name {
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
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The form-list calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_form_list(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (Kind::Object, "getsize") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object, "getat") if matches!(arg_kinds, [Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num)]) => {
                return Some(Kind::Object);
            }
            (Kind::Object, "addform" | "removeaddedform")
                if statement && matches!(arg_kinds, [Arg::Kind(Kind::Object | Kind::Player | Kind::Quest)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object, "revert") if statement && arg_kinds.is_empty() => return Some(Kind::None),
            (Kind::Object, "hasform") if matches!(arg_kinds, [Arg::Kind(Kind::Object | Kind::Player | Kind::Quest)]) => {
                return Some(Kind::Bool);
            }
            (Kind::Object, "find") if matches!(arg_kinds, [Arg::Kind(Kind::Object | Kind::Player)]) => {
                return Some(Kind::Num);
            }
            _ => {}
        }
        None
    }
}
