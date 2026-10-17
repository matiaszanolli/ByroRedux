//! Location natives: where a reference is, was placed, and whether it is loaded.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "getcurrentlocation",
    "geteditorlocation",
    "isloaded",
    "isinlocation",
];

impl Vm<'_> {
    /// The location reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_location(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
            // CK *GetEditorLocation*: the reference's editor location.
            (Value::Form(reference), "geteditorlocation") if args.is_empty() => Some(
                self.entity_of(*reference)
                    .and_then(|entity| crate::location::editor_location(self.world, entity))
                    .map_or(Value::None, Value::Form),
            ),
            (Value::Form(form), "getcurrentlocation") => Some(
                self.entity_of(*form)
                    .and_then(|entity| crate::location::current_location(self.world, entity))
                    .map_or(Value::None, Value::Form),
            ),
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
            // CK *IsLoaded - Location*: whether the location is loaded.
            (Value::Form(location), "isloaded") if args.is_empty() => {
                match crate::location::location_loaded(self.world, *location) {
                    Some(loaded) => Some(Value::Bool(loaded)),
                    None => return stop("IsLoaded on a form that is not a location"),
                }
            }
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The location calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_location(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        _statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (Kind::Object | Kind::Player, "getcurrentlocation") => {
                return Some(Kind::Object);
            }
            (Kind::Object, "isloaded") if arg_kinds.is_empty() => return Some(Kind::Bool),
            (Kind::Object | Kind::Player, "isinlocation") if matches!(arg_kinds, [Arg::Kind(Kind::Object)]) => {
                return Some(Kind::Bool);
            }
            (Kind::Object, "geteditorlocation") if arg_kinds.is_empty() => {
                return Some(Kind::Object);
            }
            _ => {}
        }
        None
    }
}
