//! Favor natives: gold, bribery, intimidation and the favor a speaker owes.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "setnofavorallowed",
    "getbribeamount",
    "getgoldamount",
    "setbribed",
    "isbribed",
    "setintimidated",
    "modfavorpoints",
    "modfavorpointswithglobal",
    "makeplayerfriend",
    "isintimidated",
];

impl Vm<'_> {
    /// The favor reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_favor(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
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
                    .and_then(|entity| {
                        self.world
                            .get::<byroredux_core::ecs::components::Inventory>(entity)
                            .map(|inventory| inventory.count_of(SKYRIM_GOLD))
                    })
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
                let friend = match (name, values.first()) {
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
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The favor calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_favor(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (Kind::Object | Kind::Player, "getbribeamount" | "getgoldamount") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object | Kind::Player, "isbribed" | "isintimidated") if arg_kinds.is_empty() => return Some(Kind::Bool),
            (Kind::Object | Kind::Player, "modfavorpoints")
                if statement && matches!(arg_kinds, [Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num)]) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "modfavorpointswithglobal")
                if statement && matches!(arg_kinds, [Arg::Kind(Kind::Object)]) =>
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
            (Kind::Object | Kind::Player, "setnofavorallowed")
                if statement && arg_kinds.len() <= 1 && arg_kinds.iter().all(|a| matches!(a, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool))) =>
            {
                return Some(Kind::None);
            }
            _ => {}
        }
        None
    }
}
