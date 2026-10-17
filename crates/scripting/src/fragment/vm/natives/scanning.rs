//! Starfield scanning natives: the scanned flag and the map / star-map markers.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "setscanned",
    "isscanned",
    "addtomapscanned",
    "setmarkervisibleonstarmap",
];

impl Vm<'_> {
    /// The scanning reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_scanning(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
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
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The scanning calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_scanning(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (Kind::Object | Kind::Player, "isscanned") if arg_kinds.is_empty() => return Some(Kind::Bool),
            (Kind::Object | Kind::Player, "setscanned" | "addtomapscanned" | "setmarkervisibleonstarmap")
                if statement && matches!(arg_kinds, [Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool)]) =>
            {
                return Some(Kind::None);
            }
            _ => {}
        }
        None
    }
}
