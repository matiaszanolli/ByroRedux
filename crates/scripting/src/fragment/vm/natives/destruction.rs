//! Destruction natives: damage, the destruction stage and the destroyed flag.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "damageobject",
    "getcurrentdestructionstage",
    "cleardestruction",
    "setdestroyed",
    "isdestroyed",
];

impl Vm<'_> {
    /// The destruction reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_destruction(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
            (Value::Form(reference), "damageobject") if args.len() == 1 => {
                let values = self.eval_args(frame, args)?;
                let amount = values.first().and_then(Value::number).unwrap_or(0.0);
                if let Some(entity) = self.entity_of(*reference) {
                    if let Some(outcome) = crate::destruction::damage_object(self.world, entity, *reference, amount) {
                        if outcome.new_stage != outcome.old_stage {
                            crate::fragment::raise_reference_event(
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
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The destruction calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_destruction(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (Kind::Object | Kind::Player, "damageobject")
                if statement
                    && matches!(arg_kinds, [Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "getcurrentdestructionstage") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object | Kind::Player, "cleardestruction") if statement && arg_kinds.is_empty() => return Some(Kind::None),
            (Kind::Object | Kind::Player, "setdestroyed")
                if statement
                    && matches!(arg_kinds, [] | [Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "isdestroyed") if arg_kinds.is_empty() => return Some(Kind::Bool),
            _ => {}
        }
        None
    }
}
