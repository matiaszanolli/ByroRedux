//! Ownership and crime natives: owners, the cause of an act, faction crime gold.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "setactorcause",
    "getfactionowner",
    "getactorowner",
    "setactorowner",
    "setfactionowner",
    "setcrimegold",
    "setcrimegoldviolent",
    "modcrimegold",
    "getcrimegold",
    "getcrimegoldviolent",
    "getcrimegoldnonviolent",
    "getcrimefaction",
];

impl Vm<'_> {
    /// The ownership / crime reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_ownership_crime(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
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
                match name {
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
                        Some(Value::Int(match name {
                            "getcrimegoldviolent" => violent,
                            "getcrimegoldnonviolent" => nonviolent,
                            _ => nonviolent.saturating_add(violent),
                        }))
                    }
                    _ => return stop(format!("{name} as written")),
                }
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
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The ownership / crime calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_ownership_crime(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (Kind::Object | Kind::Player, "setactorowner" | "setfactionowner")
                if statement && matches!(arg_kinds, [Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object, "setcrimegold" | "setcrimegoldviolent")
                if statement && matches!(arg_kinds, [Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num)]) =>
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
            (Kind::Object | Kind::Player, "getcrimefaction") if arg_kinds.is_empty() => return Some(Kind::Object),
            (Kind::Object | Kind::Player, "setactorcause")
                if statement && matches!(arg_kinds, [Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object, "getfactionowner" | "getactorowner") if arg_kinds.is_empty() => {
                return Some(Kind::Object);
            }
            _ => {}
        }
        None
    }
}
