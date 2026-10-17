//! Actor natives: race, package, dialogue, voice, spells and command flags.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "getdialoguetarget",
    "ischild",
    "getrace",
    "ignorefriendlyhits",
    "setcandocommand",
    "hasspell",
    "getcurrentpackage",
    "getfactionreaction",
    "isindialoguewithplayer",
    "getsex",
    "getvoicetype",
];

impl Vm<'_> {
    /// The actor reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_actor(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
            // CK *IgnoreFriendlyHits* / *SetCanDoCommand - Actor*: boolean flags
            // (default True) recorded for the systems that will read them.
            (Value::Form(reference), "ignorefriendlyhits" | "setcandocommand") if args.len() <= 1 => {
                let values = self.eval_args(frame, args)?;
                let on = values.first().is_none_or(Value::truthy);
                let flag = if name == "ignorefriendlyhits" {
                    crate::ReferenceFlag::IgnoreFriendlyHits
                } else {
                    crate::ReferenceFlag::CanDoCommand
                };
                self.deferred
                    .reference_flag_changes
                    .push(crate::ReferenceFlagChange::Flag { reference: *reference, flag, on });
                Some(Value::None)
            }
            // CK *GetRace - Actor*: "the Race the actor is" (its record's).
            (Value::Form(actor), "getrace") if args.is_empty() => {
                let entity = if *actor == PLAYER_REF {
                    crate::papyrus_demo::PapyrusPlayerEntity::get(self.world)
                } else {
                    self.entity_of(*actor)
                };
                Some(
                    entity
                        .and_then(|entity| self.world.get::<byroredux_core::character::Background>(entity).map(|b| b.race_form_id))
                        .filter(|race| *race != 0)
                        .map_or(Value::None, Value::Form),
                )
            }
            // CK *GetFactionReaction - Actor*: 0 Neutral, 1 Enemy, 2 Ally, 3 Friend,
            // by the engine-policy combination of the two actors' factions.
            (Value::Form(actor), "getfactionreaction") if args.len() == 1 => {
                let other = match self.eval(frame, &args[0].value.node)? {
                    Value::Form(other) => other,
                    _ => return stop("GetFactionReaction without an actor"),
                };
                let reaction = self
                    .entity_of(*actor)
                    .zip(self.entity_of(other))
                    .and_then(|(actor, other)| crate::faction_reaction(self.world, actor, other));
                match reaction {
                    Some(reaction) => Some(Value::Int(reaction)),
                    None => return stop("GetFactionReaction on an actor that is not loaded"),
                }
            }
            // CK *GetCurrentPackage - Actor*: the package the actor runs now,
            // from the AI's transition reports; None while it runs none.
            (Value::Form(actor), "getcurrentpackage") if args.is_empty() => Some(
                self.entity_of(*actor)
                    .and_then(|entity| {
                        self.world
                            .try_resource::<crate::fragment::ActivePackages>()
                            .and_then(|active| active.0.get(&entity).copied())
                    })
                    .map_or(Value::None, Value::Form),
            ),
            // CK *IsChild - Location*: "Returns true if the other location is a
            // child of this one. If they are the same location, the function
            // returns false."
            (Value::Form(this), "ischild") if args.len() == 1 => {
                let other = match self.eval(frame, &args[0].value.node)? {
                    Value::Form(other) => other,
                    _ => 0,
                };
                let child = other != 0
                    && other != *this
                    && self
                        .world
                        .try_resource::<crate::location::LocationTable>()
                        .is_some_and(|table| table.is_within(other, *this));
                Some(Value::Bool(child))
            }
            // CK *GetDialogueTarget*: "the actor this actor is currently in
            // dialogue with" — the player for the line's speaker, the speaker
            // for the player.
            (Value::Form(form), "getdialoguetarget") => {
                let speaker = self.deferred.speaker.and_then(|entity| self.entity_form(entity));
                Some(match speaker {
                    Some(speaker) if speaker == *form => Value::Form(PLAYER_REF),
                    Some(speaker) if *form == PLAYER_REF => Value::Form(speaker),
                    _ => Value::None,
                })
            }
            // CK *IsInDialogueWithPlayer*: "Is this actor or talking activator
            // currently talking to the player?" — the NPC whose response
            // surface is open.
            (Value::Form(actor), "isindialoguewithplayer") if args.is_empty() => {
                let partner = self
                    .world
                    .try_resource::<crate::DialogueSurfaceState>()
                    .and_then(|surface| surface.npc);
                Some(Value::Bool(partner.is_some() && partner == self.entity_of(*actor)))
            }
            // CK *GetSex - ActorBase*: "-1: None, 0: Male, 1: Female", from the
            // base's female flag. A form that is no actor base stops the run.
            (Value::Form(base), "getsex") if args.is_empty() => {
                match self.world.try_resource::<crate::ActorBaseTraits>().and_then(|traits| traits.get(*base)) {
                    Some(base_trait) => Some(Value::Int(i32::from(base_trait.female))),
                    None => return stop("GetSex on a form that is not an actor base"),
                }
            }
            // CK *GetVoiceType - ObjectReference*: the voice type of the actor.
            (Value::Form(actor), "getvoicetype") if args.is_empty() => {
                let voice = self.entity_of(*actor).and_then(|entity| crate::voice_type(self.world, entity));
                Some(voice.map_or(Value::None, Value::Form))
            }
            // Destructible objects (see [`crate::destruction`]): CK *DamageObject*
            // ("damages this object, which may fire OnDestructionStageChanged"),
            // *GetCurrentDestructionStage* (-1 undamaged), *ClearDestruction* (no
            // event), *SetDestroyed* ("sets or clears this object's destroyed
            // flag", default True) and *IsDestroyed*.
            // CK *Fire - Weapon*: fires the weapon from the source with the ammo (None:
            // the reference's current ammo); the host launches the projectile (see
            // [`crate::projectile_requests`]).
            // CK *CreateDetectionEvent*: a detection event at this object's location
            // for `akOwner` at `aiSoundLevel` (default 0); recorded for a hearing
            // system the engine does not have (see [`crate::detection_events`]).
            // CK *HasSpell - Actor*: whether the actor "knows" the spell or shout (not
            // whether one is acting on it) - the actor's spell list, which holds its
            // race's and what `AddSpell` gave it.
            (Value::Form(actor), "hasspell") if args.len() == 1 => {
                let values = self.eval_args(frame, args)?;
                let spell = match values.first() {
                    Some(Value::Form(spell)) => *spell,
                    _ => 0,
                };
                let knows = spell != 0
                    && self
                        .entity_of(*actor)
                        .and_then(|entity| self.world.get::<crate::SpellList>(entity).map(|list| list.0.contains(&spell)))
                        .unwrap_or(false);
                Some(Value::Bool(knows))
            }
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The actor calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_actor(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (Kind::Object | Kind::Player, "getdialoguetarget") => {
                return Some(Kind::Object);
            }
            (Kind::Object | Kind::Player, "getcurrentpackage") if arg_kinds.is_empty() => return Some(Kind::Object),
            (Kind::Object | Kind::Player, "isindialoguewithplayer") if arg_kinds.is_empty() => return Some(Kind::Bool),
            (Kind::Object, "getsex") if arg_kinds.is_empty() => return Some(Kind::Num),
            (Kind::Object | Kind::Player, "getvoicetype") if arg_kinds.is_empty() => return Some(Kind::Object),
            (Kind::Object | Kind::Player, "hasspell")
                if matches!(arg_kinds, [Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit)]) =>
            {
                return Some(Kind::Bool);
            }
            (Kind::Object | Kind::Player, "getfactionreaction")
                if matches!(arg_kinds, [Arg::Kind(Kind::Object | Kind::Player)]) =>
            {
                return Some(Kind::Num);
            }
            (Kind::Object | Kind::Player, "getrace") if arg_kinds.is_empty() => return Some(Kind::Object),
            (Kind::Object | Kind::Player, "ignorefriendlyhits" | "setcandocommand")
                if statement && arg_kinds.len() <= 1 && arg_kinds.iter().all(|a| matches!(a, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool))) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "ischild")
                if matches!(arg_kinds, [Arg::Kind(Kind::Object | Kind::Player)]) =>
            {
                return Some(Kind::Bool);
            }
            _ => {}
        }
        None
    }
}
