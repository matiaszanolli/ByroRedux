//! Combat natives: trap hits, detection events and firing a weapon.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "processtraphit",
    "createdetectionevent",
    "fire",
];

impl Vm<'_> {
    /// The combat reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_combat(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
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
                    pending.0.push(crate::ProjectileLaunch { source, weapon: *weapon, ammo, aim_at: 0, harmless: false });
                }
                Some(Value::None)
            }
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The combat calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_combat(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
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
            _ => {}
        }
        None
    }
}
