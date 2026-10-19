//! Actor-state primitives (#5801): factions, relationship rank, perks,
//! actor values and skills, teammate / favor flags, kill and protection,
//! spells and race spells.

use super::*;

/// `AddToFaction(akFaction)` / `RemoveFromFaction(akFaction)` /
/// `SetFactionRank(akFaction, aiRank)` with a literal rank in `i8` range.
pub(super) fn prim_change_faction(e: &Expr, scope: &Scope) -> Option<Effect> {
    // `RemoveFromAllFactions()` names no faction; the effect's faction is
    // unused.
    if let Some((object, args)) = method_call(e, "RemoveFromAllFactions") {
        if !args.is_empty() {
            return None;
        }
        return Some(Effect::ChangeFaction {
            actor: receiver_actor(object, scope)?,
            faction: ObjectRef::Property(String::new()),
            change: crate::FactionChange::RemoveAll,
        });
    }
    let (object, args, change) = if let Some((object, args)) = method_call(e, "AddToFaction") {
        (object, args, crate::FactionChange::Add)
    } else if let Some((object, args)) = method_call(e, "RemoveFromFaction") {
        (object, args, crate::FactionChange::Remove)
    } else if let Some((object, args)) = method_call(e, "SetFactionRank") {
        if args.len() != 2 {
            return None;
        }
        let rank = i8::try_from(int_arg(args, 1).or_else(|| {
            // `-1` reaches the AST as a negated literal.
            as_num(&args[1].value.node).filter(|n| n.fract() == 0.0).map(|n| n as i64)
        })?)
        .ok()?;
        (object, &args[..1], crate::FactionChange::SetRank(rank))
    } else {
        return None;
    };
    if args.len() != 1 {
        return None;
    }
    Some(Effect::ChangeFaction {
        actor: receiver_actor(object, scope)?,
        faction: receiver_object(&args[0].value.node, scope)?,
        change,
    })
}

/// `Actor.SetPlayerTeammate(bool abTeammate = true, bool abCanDoFavor =
/// true)` with literal arguments.
pub(super) fn prim_set_player_teammate(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetPlayerTeammate")?;
    if args.len() > 2 {
        return None;
    }
    Some(Effect::SetPlayerTeammate {
        actor: receiver_actor(object, scope)?,
        teammate: bool_arg(args, 0)?.unwrap_or(true),
        can_do_favor: bool_arg(args, 1)?.unwrap_or(true),
    })
}

/// `Actor.SetDoingFavor(bool abDoingFavor = true)` with a literal argument.
pub(super) fn prim_set_doing_favor(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetDoingFavor")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetDoingFavor {
        actor: receiver_actor(object, scope)?,
        doing: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// `Game.AdvanceSkill(string asSkillName, float afMagnitude)` with literal
/// arguments.
pub(super) fn prim_advance_skill(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Game", "AdvanceSkill")?;
    if args.len() != 2 {
        return None;
    }
    let Expr::StringLit(skill) = &args[0].value.node else {
        return None;
    };
    Some(Effect::AdvanceSkill {
        skill: skill.clone(),
        magnitude: as_num(&args[1].value.node)?,
    })
}

/// `Actor.SetRelationshipRank(Actor akOther, int aiRank)` with a literal rank
/// in the CK's −4 … +4.
pub(super) fn prim_set_relationship_rank(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetRelationshipRank")?;
    if args.len() != 2 {
        return None;
    }
    let (other, rank) = (&args[0], &args[1]);
    let mut other = &other.value.node;
    while let Expr::Cast { expr, .. } = other {
        other = &expr.node;
    }
    let rank = int_arg(args, 1).or_else(|| {
        // `-1` reaches the AST as a negated literal.
        as_num(&rank.value.node)
            .filter(|n| n.fract() == 0.0)
            .map(|n| n as i64)
    })?;
    if !(-4..=4).contains(&rank) {
        return None;
    }
    Some(Effect::SetRelationshipRank {
        actor: receiver_actor(object, scope)?,
        other: receiver_actor(other, scope)?,
        rank: rank as i8,
    })
}

/// `Actor.Kill(Actor akKiller = None)` and `KillSilent(Actor akKiller = None)`.
/// A player victim declines: the player's death is not a quest effect the
/// runtime models.
pub(super) fn prim_kill(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args, silent) = match method_call(e, "Kill") {
        Some((object, args)) => (object, args, false),
        None => {
            let (object, args) = method_call(e, "KillSilent")?;
            (object, args, true)
        }
    };
    if args.len() > 1 {
        return None;
    }
    let actor = receiver_actor(object, scope)?;
    if actor == ActorRef::Player {
        return None;
    }
    let killer = match args.first().map(|arg| &arg.value.node) {
        None | Some(Expr::NoneLit) => None,
        Some(killer) => Some(receiver_actor(killer, scope)?),
    };
    Some(Effect::Kill { actor, killer, silent })
}

/// `ActorBase.SetEssential(bool abEssential = true)` /
/// `SetProtected(bool abProtected = true)`, on `<actor>.GetActorBase()` or an
/// ActorBase property.
pub(super) fn prim_set_base_protection(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args, protected) = match method_call(e, "SetEssential") {
        Some((object, args)) => (object, args, false),
        None => {
            let (object, args) = method_call(e, "SetProtected")?;
            (object, args, true)
        }
    };
    if args.len() > 1 {
        return None;
    }
    let base = match method_call(object, "GetActorBase") {
        Some((actor, [])) => {
            ActorBaseRef::OfActor(receiver_actor(actor, scope)?)
        }
        Some(_) => return None,
        None => ActorBaseRef::Form(receiver_object(object, scope)?),
    };
    Some(Effect::SetBaseProtection {
        base,
        protected,
        value: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// `AddPerk(akPerk)` (Skyrim) / `AddPerk(akPerk, abNotify = false)` (FO4,
/// Starfield) and `RemovePerk(akPerk)`. The notify flag is a literal-only UI
/// hint.
pub(super) fn prim_change_perk(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args, add) = if let Some((object, args)) = method_call(e, "AddPerk") {
        (object, args, true)
    } else {
        let (object, args) = method_call(e, "RemovePerk")?;
        (object, args, false)
    };
    if args.is_empty() || args.len() > if add { 2 } else { 1 } {
        return None;
    }
    bool_arg(args, 1)?;
    Some(Effect::ChangePerk {
        actor: receiver_actor(object, scope)?,
        perk: receiver_object(&args[0].value.node, scope)?,
        add,
    })
}

pub(super) fn prim_set_actor_value(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) =
        method_call_any(e, &["SetAV", "SetActorValue", "SetValue", "TryToSetActorValue", "TryToSetValue"])?;
    if args.len() != 2 {
        return None;
    }
    let actor = receiver_actor(object, scope)?;
    let value = classify_value(&args[1].value.node, scope)?;
    Some(match actor_value_arg(&args[0].value.node, scope)? {
        ActorValueArg::Form(actor_value) => Effect::SetActorValue {
            actor,
            actor_value,
            value,
        },
        ActorValueArg::Named(name) => Effect::SetActorValueNamed { actor, name, value },
    })
}

/// `DamageValue` / `DamageActorValue` / `DamageAV` and `RestoreValue` /
/// `RestoreActorValue` / `RestoreAV` with an actor value and an amount.
///
/// #5973 — Skyrim's spellings (`DamageActorValue`, `DamageAV`,
/// `RestoreActorValue`, `RestoreAV`) discard the amount's sign (CK wiki:
/// "Negative numbers will be converted to positive so -100 and 100 will have
/// the same effect"), so a script's `DamageActorValue("Health", -N)` is damage,
/// not a heal. Those lower the amount's magnitude. The Fallout 4 / Starfield
/// `DamageValue` / `RestoreValue` pages are silent on sign, so they keep the
/// amount as written until measured.
pub(super) fn prim_damage_actor_value(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args, restore, sign_discarded) =
        method_call_any(e, &["DamageActorValue", "DamageAV"])
            .map(|(object, args)| (object, args, false, true))
            .or_else(|| {
                method_call_any(e, &["RestoreActorValue", "RestoreAV"])
                    .map(|(object, args)| (object, args, true, true))
            })
            .or_else(|| {
                method_call_any(e, &["DamageValue"]).map(|(object, args)| (object, args, false, false))
            })
            .or_else(|| {
                method_call_any(e, &["RestoreValue"]).map(|(object, args)| (object, args, true, false))
            })?;
    if args.len() != 2 {
        return None;
    }
    let amount = classify_value(&args[1].value.node, scope)?;
    let amount = match amount {
        ValueExpr::Literal(value) if sign_discarded => ValueExpr::Literal(value.abs()),
        other if sign_discarded => ValueExpr::Abs(Box::new(other)),
        other => other,
    };
    Some(Effect::DamageActorValue {
        actor: receiver_actor(object, scope)?,
        actor_value: actor_value_arg(&args[0].value.node, scope)?,
        amount,
        restore,
    })
}

/// #4415 — Skyrim's `actor.psc` declares `AddSpell(Spell akSpell, Bool
/// abVerbose = true)`; the flag only picks whether the "spell added"
/// message shows, so either form lowers.
pub(super) fn prim_add_spell(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "AddSpell")?;
    if !(1..=2).contains(&args.len()) || (args.len() == 2 && bool_arg(args, 1)?.is_none()) {
        return None;
    }
    Some(Effect::AddSpell {
        actor: receiver_actor(object, scope)?,
        spell: receiver_object(&args[0].value.node, scope)?,
    })
}

pub(super) fn prim_remove_spell(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "RemoveSpell")?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::RemoveSpell {
        actor: receiver_actor(object, scope)?,
        spell: receiver_object(&args[0].value.node, scope)?,
    })
}

/// #4415 — `AddRaceSpells` / `RemoveRaceSpells` are *script-defined*
/// helpers on quest scripts (MQ101QuestScript authors both), not
/// `actor.psc` methods — the translator sees an unqualified call to the
/// own script's function. Lowering by name is sound without inlining the
/// helper's body because the semantic is stateless: re-apply / clear the
/// actor's race `SPLO` set, which spawn pre-stamped as `RaceSpells`.
/// #5407 — that soundness extends ONLY to the vanilla shape: the
/// unqualified call, in a script that itself defines the function
/// ARGLESS. A member-call spelling (`x.AddRaceSpells()` on any object)
/// and a same-named helper some other script defines with different
/// semantics are declined — both used to lower onto the player
/// unconditionally.
fn lower_race_spells(e: &Expr, method: &str, adding: bool, scope: &Scope) -> Option<Effect> {
    if !prim_race_spells_arity_ok(e, method, scope) {
        return None;
    }
    // The vanilla helpers act on the player, and the accepted spelling
    // (unqualified, own-script-defined) is exactly how they call it.
    Some(if adding {
        Effect::AddRaceSpells {
            actor: ActorRef::Player,
        }
    } else {
        Effect::RemoveRaceSpells {
            actor: ActorRef::Player,
        }
    })
}

/// Only the UNQUALIFIED, ARGLESS call to a function the calling script
/// itself defines (`ScriptName`-level function table via
/// [`Scope::own_argless_functions`]). Member-call spellings are declined
/// outright — the receiver is NOT distinguished, because no receiver is
/// accepted at all (#5407).
fn prim_race_spells_arity_ok(e: &Expr, method: &str, scope: &Scope) -> bool {
    let Expr::Call { callee, args } = e else {
        return false;
    };
    if !args.is_empty() {
        return false;
    }
    let Expr::Ident(name) = &callee.node else {
        return false;
    };
    name.0.eq_ignore_ascii_case(method)
        && scope
            .own_argless_functions
            .contains(&name.0.to_ascii_lowercase())
}

pub(super) fn prim_add_race_spells(e: &Expr, scope: &Scope) -> Option<Effect> {
    // Arity, spelling, and own-script definition are bounded by
    // `prim_race_spells_arity_ok` inside.
    lower_race_spells(e, "AddRaceSpells", true, scope)
}

pub(super) fn prim_remove_race_spells(e: &Expr, scope: &Scope) -> Option<Effect> {
    // Arity, spelling, and own-script definition are bounded by
    // `prim_race_spells_arity_ok` inside.
    lower_race_spells(e, "RemoveRaceSpells", false, scope)
}
