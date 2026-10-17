//! Object-reference natives: scale, placement, inventory drops, furniture, base objects.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "getbaseobject",
    "getactorbase",
    "getleveledactorbase",
    "placeatme",
    "gettriggerobjectcount",
    "getscale",
    "calculateencounterlevel",
    "applyhavokimpulse",
    "dropobject",
    "isfurnitureinuse",
];

/// The most references one `PlaceAtMe` call creates.
pub(in crate::fragment::vm) const MAX_PLACED_PER_CALL: i32 = 32;

/// Whether `PlaceAtMe`'s arguments are ones the VM runs: the form, then the
/// count, persist, initially-disabled and delete-when-able flags (Skyrim sends
/// the first four, Fallout 4 five), then Starfield's offsets, alias fill and
/// navmesh snap, where the first two must be `None` and the snap is moot.
pub(in crate::fragment::vm) fn place_at_me_args_ok(args: &[Arg]) -> bool {
    let unset = |arg: &Arg| matches!(arg, Arg::Literal(Expr::NoneLit));
    let flag = |arg: &Arg| matches!(arg, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool | Kind::Num));
    let count = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num));
    let Some(Arg::Kind(Kind::Object)) = args.first() else {
        return false;
    };
    match args.len() {
        1 => true,
        2..=5 => count(&args[1]) && args[2..].iter().all(flag),
        8 => count(&args[1]) && args[2..5].iter().all(flag) && unset(&args[5]) && unset(&args[6]) && flag(&args[7]),
        _ => false,
    }
}

impl Vm<'_> {
    /// CK *PlaceAtMe - ObjectReference*: "Makes X new references of the passed
    /// in form ... and places them at our reference's location, returning the
    /// last one it created. Or None if no references could be created". The
    /// `abInitiallyDisabled` flag "is only applied to the last reference
    /// created". The references are recorded after the quest guards drop
    /// ([`crate::CreatedReferences`]), where the engine places them at `at` and
    /// spawns them whenever their cell is loaded. Arguments past the FO4
    /// `abDeleteWhenAble` are Starfield's offsets, alias fill and navmesh snap,
    /// which the checker only lets through unset.
    pub(in crate::fragment::vm) fn place_at_me(&mut self, at: u32, values: &[Value]) -> Run<Value> {
        let Some(Value::Form(base)) = values.first() else {
            return Ok(Value::None);
        };
        let count = values.get(1).and_then(Value::int).unwrap_or(1).clamp(1, MAX_PLACED_PER_CALL);
        let disabled = values.get(3).is_some_and(Value::truthy);
        let delete_when_able = values.get(4).is_some_and(Value::truthy);
        if !matches!(values.get(5), None | Some(Value::None)) || !matches!(values.get(6), None | Some(Value::None)) {
            return stop("PlaceAtMe with offsets or an alias to fill");
        }
        let Some(ids) = self.deferred.scripted_ids.clone() else {
            return stop("PlaceAtMe without CreatedReferences");
        };
        let mut last = Value::None;
        for _ in 0..count {
            let Some(form_id) = ids.next() else {
                break;
            };
            self.deferred.created_references.push((form_id, *base, at, delete_when_able));
            last = Value::Form(form_id);
        }
        if disabled {
            if let Value::Form(form_id) = last {
                self.deferred.reference_enable_changes.push((form_id, false));
            }
        }
        Ok(last)
    }

    /// The reference reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_reference(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
            // CK *GetScale*: "the current scale of the reference".
            (Value::Form(reference), "getscale") if args.is_empty() => {
                let entity = if *reference == PLAYER_REF {
                    crate::papyrus_demo::PapyrusPlayerEntity::get(self.world)
                } else {
                    self.entity_of(*reference)
                };
                Some(Value::Float(
                    entity
                        .and_then(|entity| self.world.get::<byroredux_core::ecs::Transform>(entity).map(|t| t.scale))
                        .unwrap_or(1.0),
                ))
            }
            // CK *CalculateEncounterLevel - ObjectReference*: the level from the
            // player's level, the reference's encounter zone and the difficulty
            // (default 4, None); see `encounter_level`.
            (Value::Form(reference), "calculateencounterlevel") if args.len() <= 1 => {
                let difficulty = match args.first() {
                    None => 4,
                    Some(arg) => match self.eval(frame, &arg.value.node)? {
                        Value::Int(difficulty) => difficulty,
                        _ => return stop("CalculateEncounterLevel with a non-integer difficulty"),
                    },
                };
                let level = self
                    .entity_of(*reference)
                    .and_then(|entity| crate::calculate_encounter_level(self.world, entity, difficulty));
                match level {
                    Some(level) => Some(Value::Int(level as i32)),
                    None => return stop("CalculateEncounterLevel without the player's level or a difficulty multiplier"),
                }
            }
            // CK *GetTriggerObjectCount*: "the number of objects currently
            // inside this trigger volume".
            (Value::Form(trigger), "gettriggerobjectcount") if args.is_empty() => Some(Value::Int(
                self.entity_of(*trigger)
                    .map_or(0, |entity| crate::trigger_object_count(self.world, entity)) as i32,
            )),
            (Value::Form(at), "placeatme") => {
                let values = self.eval_args(frame, args)?;
                Some(self.place_at_me(*at, &values)?)
            }
            // CK *DropObject - ObjectReference*: "Drops the specified object from
            // this object's inventory" and returns "the object that was
            // dropped". Up to `aiCount` (default 1) of the item leave the
            // inventory and appear in the world at the owner, one reference
            // each (the CK stacks a count above one into a single reference,
            // which created references have no count for); an item the
            // inventory does not hold drops nothing and reads None.
            (Value::Form(owner), "dropobject") if (1..=2).contains(&args.len()) => {
                let values = self.eval_args(frame, args)?;
                let Some(Value::Form(item)) = values.first() else {
                    return stop("DropObject without a form");
                };
                let wanted = values.get(1).and_then(Value::int).unwrap_or(1).clamp(0, MAX_PLACED_PER_CALL);
                let Some(entity) = self.entity_of(*owner) else {
                    return stop("DropObject on a reference that is not loaded");
                };
                let held: u32 = self
                    .world
                    .query::<byroredux_core::ecs::components::Inventory>()
                    .and_then(|inventories| {
                        inventories.get(entity).map(|inventory| {
                            inventory.items.iter().filter(|stack| stack.base_form_id == *item).map(|stack| stack.count).sum()
                        })
                    })
                    .unwrap_or(0);
                let take = held.min(wanted as u32);
                if take == 0 {
                    return Ok(Some(Value::None));
                }
                crate::fragment::remove_items(self.world, entity, *item, take, None);
                Some(self.place_at_me(*owner, &[Value::Form(*item), Value::Int(take as i32)])?)
            }
            // CK *IsFurnitureInUse*: "if any marker on the furniture is in use
            // (or reserved, if we aren't ignoring reservations)". The occupants
            // here are the actors seated on it and the seats the sandbox AI
            // has claimed; nothing else uses furniture.
            (Value::Form(furniture), "isfurnitureinuse") if args.len() <= 1 => {
                let ignore_reserved = match args.first() {
                    Some(arg) => self.eval(frame, &arg.value.node)?.truthy(),
                    None => false,
                };
                let Some(entity) = self.entity_of(*furniture) else {
                    return stop("IsFurnitureInUse on a reference that is not loaded");
                };
                let seated = self.world.query::<byroredux_core::ecs::components::Seated>().is_some_and(|seated| {
                    seated.iter().any(|(_, seat)| seat.furniture == entity)
                });
                let reserved = !ignore_reserved
                    && self
                        .world
                        .try_resource::<byroredux_core::ecs::components::SeatReservations>()
                        .is_some_and(|seats| seats.0.keys().any(|(held, _)| *held == entity));
                Some(Value::Bool(seated || reserved))
            }
            // CK *ApplyHavokImpulse - ObjectReference*: the direction normalized,
            // the magnitude in the game's own unit (see [`crate::havok_impulse`]).
            // The binary applies the queued request to the reference's body.
            (Value::Form(reference), "applyhavokimpulse") if args.len() == 4 => {
                let values = self.eval_args(frame, args)?;
                let mut numbers = [0.0_f32; 4];
                for (slot, value) in numbers.iter_mut().zip(&values) {
                    *slot = match value.number() {
                        Some(number) => number,
                        None => return stop("ApplyHavokImpulse with a non-numeric argument"),
                    };
                }
                if self.world.try_resource::<crate::HavokImpulseRules>().is_none() {
                    return stop("ApplyHavokImpulse without the game's impulse settings");
                }
                if let Some(entity) = self.entity_of(*reference) {
                    // A reference with no 3D, or a zero-length vector, is the
                    // CK's documented failure: a warning and no impulse.
                    crate::request_havok_impulse(self.world, entity, numbers[0], numbers[1], numbers[2], numbers[3]);
                }
                Some(Value::None)
            }
            (Value::Form(form), "getbaseobject" | "getactorbase" | "getleveledactorbase") => {
                Some(self.base_of(*form).map_or(Value::None, Value::Form))
            }
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The reference calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_reference(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (Kind::Object | Kind::Player, "getbaseobject" | "getactorbase" | "getleveledactorbase") => {
                return Some(Kind::Object);
            }
            (Kind::Object | Kind::Player, "dropobject")
                if matches!(arg_kinds.first(), Some(Arg::Kind(Kind::Object | Kind::Player)))
                    && arg_kinds.get(1).is_none_or(|count| matches!(count, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num)))
                    && arg_kinds.len() <= 2 =>
            {
                return Some(Kind::Object);
            }
            (Kind::Object | Kind::Player, "isfurnitureinuse")
                if arg_kinds.len() <= 1
                    && arg_kinds.first().is_none_or(|arg| matches!(arg, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool))) =>
            {
                return Some(Kind::Bool);
            }
            (Kind::Object | Kind::Player, "applyhavokimpulse")
                if statement
                    && arg_kinds.len() == 4
                    && arg_kinds
                        .iter()
                        .all(|a| matches!(a, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num))) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "calculateencounterlevel")
                if arg_kinds.len() <= 1
                    && arg_kinds
                        .first()
                        .is_none_or(|difficulty| matches!(difficulty, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num))) =>
            {
                return Some(Kind::Num);
            }
            (Kind::Object, "gettriggerobjectcount") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object | Kind::Player, "getscale") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object | Kind::Player, "placeatme") if place_at_me_args_ok(&arg_kinds) => {
                return Some(Kind::Object);
            }
            _ => {}
        }
        None
    }
}
