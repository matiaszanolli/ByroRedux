//! Alias natives: the reference filling a `ReferenceAlias` and its location.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

const ALIAS_GETTERS: [&str; 6] = [
    "getref",
    "getreference",
    "getactorref",
    "getactorreference",
    "getshipref",
    "getshipreference",
];

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "getref",
    "getreference",
    "getactorref",
    "getactorreference",
    "getshipref",
    "getshipreference",
    "getlocation",
    "trytogetcurrentlocation",
];

impl Vm<'_> {
    /// The alias reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_alias(
        &mut self,
        _frame: &mut Frame,
        owner: &Value,
        name: &str,
        _args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
            (Value::Alias { quest, alias }, getter) if ALIAS_GETTERS.contains(&getter) => Some(
                self.deferred
                    .scene_actor_bindings
                    .resolve(QuestFormId(*quest), i32::from(*alias))
                    .and_then(|entity| self.entity_form(entity))
                    .map_or(Value::None, Value::Form),
            ),
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
            // `LocationAlias.GetLocation()` — the location runtime's fill.
            (Value::Alias { quest, alias }, "getlocation") => Some(
                self.world
                    .try_resource::<crate::LocationAliasBindings>()
                    .and_then(|bindings| bindings.resolve(QuestFormId(*quest), i32::from(*alias)))
                    .map_or(Value::None, Value::Form),
            ),
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The alias calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_alias(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        _statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (Kind::Alias, "getref" | "getreference" | "getactorref" | "getactorreference" | "getshipref" | "getshipreference") => {
                return Some(Kind::Object);
            }
            (Kind::Alias, "getlocation") => return Some(Kind::Object),
            (Kind::Alias, "trytogetcurrentlocation") if arg_kinds.is_empty() => return Some(Kind::Object),
            _ => {}
        }
        None
    }
}
