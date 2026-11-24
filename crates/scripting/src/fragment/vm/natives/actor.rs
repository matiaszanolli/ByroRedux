//! Actor natives: race, package, dialogue, voice, spells and command flags.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;
use byroredux_plugin::esm::reader::GameKind;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "getdialoguetarget",
    "ischild",
    "getrace",
    "ignorefriendlyhits",
    "setcandocommand",
    "hasspell",
    "hasmagiceffectwithkeyword",
    "getcurrentpackage",
    "getfactionreaction",
    "isindialoguewithplayer",
    "getsex",
    "getvoicetype",
    "iscommandedactor",
    "getequippedshield",
    "getequippedweapon",
    "getequippeditemtype",
    "getequippedspell",
    "isweapondrawn",
    "gettemplate",
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
            // CK *IsCommandedActor - Actor*: "Whether this actor is commanded or not" — the condition's
            // model: no summon or reanimate mechanic exists, so no actor is.
            (Value::Form(actor), "iscommandedactor") if args.is_empty() => {
                let entity = if *actor == PLAYER_REF {
                    crate::papyrus_demo::PapyrusPlayerEntity::get(self.world)
                } else {
                    self.entity_of(*actor)
                };
                match entity {
                    Some(entity) => Some(Value::Bool(crate::condition::actor_is_commanded(entity, self.world))),
                    None => return stop("IsCommandedActor on an actor that is not loaded"),
                }
            }
            // Skyrim reads canonical hand occupancy (HND-READ-006). Oblivion uses its block model's biped slot.
            (Value::Form(actor), "getequippedshield") if args.is_empty() => {
                let entity = if *actor == PLAYER_REF {
                    crate::papyrus_demo::PapyrusPlayerEntity::get(self.world)
                } else {
                    self.entity_of(*actor)
                };
                let Some(entity) = entity else {
                    return stop("GetEquippedShield on an actor that is not loaded");
                };
                let game = self.world.try_resource::<crate::ScriptGame>().map(|game| game.0);
                let shield = match game {
                    Some(GameKind::Skyrim) => match crate::hands::hand_item(self.world, entity, crate::hands::Hand::Left) {
                        crate::hands::HandItem::Shield { base, .. } => Some(base),
                        _ => None,
                    },
                    Some(GameKind::Oblivion) => {
                        let bit = byroredux_core::character::block::BlockModel::OBLIVION.shield_bit;
                        let index = self.world
                            .get::<byroredux_core::ecs::components::EquipmentSlots>(entity)
                            .and_then(|slots| slots.at(bit));
                        index.and_then(|index| {
                            self.world
                                .get::<byroredux_core::ecs::components::Inventory>(entity)
                                .and_then(|inventory| inventory.get(index).map(|stack| stack.base_form_id))
                        })
                    }
                    _ => None,
                };
                Some(shield.filter(|&form| form != 0).map_or(Value::None, Value::Form))
            }
            // CK *GetEquippedWeapon - Actor*: "If true, will return the weapon equipped in the actor's
            // left hand. If false, will return the right-hand weapon instead" (the default); None when
            // the hand holds no weapon. A two-handed weapon is in both (Hand Equipment PRD, HND-READ-002).
            // CK *GetEquippedItemType - Actor*: the hand's item type, 0 left and 1 right (HND-READ-001).
            // CK *GetEquippedSpell - Actor*: the spell in source 0 (left) or 1 (right) (HND-READ-005);
            // the Other and Instant sources (2, 3) stop. CK *IsWeaponDrawn - Actor*: "whether this actor
            // has its weapon drawn", the condition function IsWeaponOut's state.
            (Value::Form(actor), "getequippedweapon" | "getequippeditemtype" | "getequippedspell" | "isweapondrawn")
                if args.len() <= 1 =>
            {
                let entity = if *actor == PLAYER_REF {
                    crate::papyrus_demo::PapyrusPlayerEntity::get(self.world)
                } else {
                    self.entity_of(*actor)
                };
                let Some(entity) = entity else {
                    return stop(format!("{name} on an actor that is not loaded"));
                };
                if name == "isweapondrawn" {
                    if !args.is_empty() {
                        return stop("IsWeaponDrawn takes no argument");
                    }
                    return Ok(Some(Value::Bool(crate::obscript_state::is_weapon_out(self.world, entity))));
                }
                let values = self.eval_args(frame, args)?;
                let hand = if name == "getequippedweapon" {
                    if values.first().is_some_and(Value::truthy) { crate::hands::Hand::Left } else { crate::hands::Hand::Right }
                } else {
                    match values.first().and_then(Value::number).map(|hand| hand as i32) {
                        Some(0) => crate::hands::Hand::Left,
                        Some(1) => crate::hands::Hand::Right,
                        _ => return stop(format!("{name} of a hand other than 0 or 1")),
                    }
                };
                let item = crate::hands::hand_item(self.world, entity, hand);
                match name {
                    "getequippedweapon" => Some(item.weapon().filter(|&form| form != 0).map_or(Value::None, Value::Form)),
                    "getequippedspell" => Some(item.spell().filter(|&form| form != 0).map_or(Value::None, Value::Form)),
                    _ => Some(Value::Int(item.item_type_code())),
                }
            }
            // CK *GetTemplate - Package*: "The Package that this one is templated on. If this package
            // isn't templated on another package, it will return None."
            (Value::Form(package), "gettemplate") if args.is_empty() => {
                let Some(registry) = self.world.try_resource::<crate::package::PackageRegistry>() else {
                    return stop("GetTemplate without the package registry");
                };
                let Some(record) = registry.package(*package) else {
                    return stop("GetTemplate on a form that is not a package");
                };
                Some(record.package_template_form_id.filter(|&form| form != 0).map_or(Value::None, Value::Form))
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

            // CK *HasMagicEffectWithKeyword - Actor*: the keyword belongs to an applied magic effect.
            (Value::Form(actor), "hasmagiceffectwithkeyword") if args.len() == 1 => {
                let Some(entity) = self.entity_of(*actor) else {
                    return stop("HasMagicEffectWithKeyword on an actor that is not loaded");
                };
                let values = self.eval_args(frame, args)?;
                let has_effect = matches!(values.as_slice(), [Value::Form(keyword)]
                    if *keyword != 0 && crate::magic_effects::has_effect_with_keyword(self.world, entity, *keyword));
                Some(Value::Bool(has_effect))
            }
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
            (Kind::Object | Kind::Player, "hasspell" | "hasmagiceffectwithkeyword")
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
            (Kind::Object | Kind::Player, "iscommandedactor") if arg_kinds.is_empty() => return Some(Kind::Bool),
            (Kind::Object | Kind::Player, "getequippedshield") if arg_kinds.is_empty() => return Some(Kind::Object),
            (Kind::Object | Kind::Player, "getequippedweapon")
                if matches!(arg_kinds, [] | [Arg::Literal(Expr::BoolLit(_))] | [Arg::Kind(Kind::Bool)]) =>
            {
                return Some(Kind::Object)
            }
            (Kind::Object | Kind::Player, "getequippeditemtype")
                if matches!(arg_kinds, [Arg::Literal(Expr::IntLit(0 | 1))] | [Arg::Kind(Kind::Num)]) =>
            {
                return Some(Kind::Num)
            }
            (Kind::Object | Kind::Player, "getequippedspell")
                if matches!(arg_kinds, [Arg::Literal(Expr::IntLit(0 | 1))] | [Arg::Kind(Kind::Num)]) =>
            {
                return Some(Kind::Object)
            }
            (Kind::Object | Kind::Player, "isweapondrawn") if arg_kinds.is_empty() => return Some(Kind::Bool),
            (Kind::Object, "gettemplate") if arg_kinds.is_empty() => return Some(Kind::Object),
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
