//! AI / combat primitives (#5801): faction enemy/ally, combat start/stop,
//! crime faction, combat alarm, unconscious / ghost, `EvaluatePackage`.
//! Applied by `fragment/effects/ai_combat.rs`.

use super::*;

/// #4318 — Skyrim's own `faction.pex` declares `SetEnemy(Faction akOther,
/// Bool abSelfIsNeutralToOther = false, Bool abOtherIsNeutralToSelf = false)`.
/// A `true` flag makes that direction *neutral* rather than hostile, so
/// `SetEnemy(x, true, true)` is "mutually neutral", not enmity. #4414 made
/// the relation directed, so both flags are carried; an omitted one takes its
/// declared default, and a non-literal one is an unknown runtime value and
/// declines.
pub(super) fn prim_set_enemy(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetEnemy")?;
    if !(1..=3).contains(&args.len()) {
        return None;
    }
    Some(Effect::SetEnemy {
        faction: receiver_object(object, scope)?,
        other_faction: receiver_object(&args[0].value.node, scope)?,
        self_neutral: bool_arg(args, 1)?.unwrap_or(false),
        other_neutral: bool_arg(args, 2)?.unwrap_or(false),
    })
}

/// `Faction.SetAlly(Faction akOther, bool abSelfIsFriendToOther = false,
/// bool abOtherIsFriendToSelf = false)`; literal flags only, like
/// [`prim_set_enemy`].
pub(super) fn prim_set_ally(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetAlly")?;
    if !(1..=3).contains(&args.len()) {
        return None;
    }
    Some(Effect::SetAlly {
        faction: receiver_object(object, scope)?,
        other_faction: receiver_object(&args[0].value.node, scope)?,
        self_friend: bool_arg(args, 1)?.unwrap_or(false),
        other_friend: bool_arg(args, 2)?.unwrap_or(false),
    })
}

pub(super) fn prim_start_combat(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "StartCombat")?;
    // FO4 / Starfield add `abPreferredTarget`, a literal flag: the combat
    // runtime holds a single target, so preferring it changes nothing.
    match args.len() {
        1 => {}
        2 => {
            bool_arg(args, 1)??;
        }
        _ => return None,
    }
    let actor = receiver_actor(object, scope)?;
    // #4323 — the runtime this arms is an NPC chase-and-strike: it steers the
    // combatant's own Transform and emits hits on cooldown. On the player
    // that would fight the character controller and auto-attack, which is
    // not what `StartCombat` means, so a player combatant is unmodeled.
    if actor == ActorRef::Player {
        return None;
    }
    Some(Effect::StartCombat {
        actor,
        target: receiver_actor(&args[0].value.node, scope)?,
    })
}

/// #4816 — Skyrim's `actor.psc` declares `StopCombat()` with no parameters.
/// A player receiver declines for the reason [`prim_start_combat`] gives:
/// the combat runtime this clears is the NPC one, and the player carries
/// none.
pub(super) fn prim_stop_combat(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "StopCombat")?;
    if !args.is_empty() {
        return None;
    }
    let actor = receiver_actor(object, scope)?;
    if actor == ActorRef::Player {
        return None;
    }
    Some(Effect::StopCombat { actor })
}

/// `<actor>.SetCrimeFaction(akFaction)`, a faction property or `None`.
pub(super) fn prim_set_crime_faction(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetCrimeFaction")?;
    if args.len() != 1 {
        return None;
    }
    let faction = match &args[0].value.node {
        Expr::NoneLit => None,
        other => Some(receiver_object(other, scope)?),
    };
    Some(Effect::SetCrimeFaction {
        actor: receiver_actor(object, scope)?,
        faction,
    })
}

/// `<actor>.StopCombatAlarm()`, the player included (a player target is the
/// usual one: stand everyone down after a persuasion or an arrest).
pub(super) fn prim_stop_combat_alarm(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "StopCombatAlarm")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StopCombatAlarm {
        actor: receiver_actor(object, scope)?,
    })
}

/// #5017 — `actor.psc` declares `SetUnconscious(bool abUnconscious = true)`.
/// A player receiver declines: an unconscious player would need the
/// controls, camera and HUD handling the Creation Kit does not describe, and
/// the vanilla calls this issue targets wake NPCs.
pub(super) fn prim_set_unconscious(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetUnconscious")?;
    if args.len() > 1 {
        return None;
    }
    let actor = receiver_actor(object, scope)?;
    if actor == ActorRef::Player {
        return None;
    }
    Some(Effect::SetUnconscious {
        actor,
        unconscious: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// `Actor.SetGhost(bool abIsGhost = true)`. The player may be a ghost too
/// (the CS page's `player.SetGhost 1`).
pub(super) fn prim_set_ghost(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetGhost")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetGhost {
        actor: receiver_actor(object, scope)?,
        ghost: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// `Actor.EvaluatePackage()` on Skyrim; `Actor.EvaluatePackage(abResetAI
/// = false)` on Fallout 4 and Starfield (shipped `Actor.psc`).
///
/// #3487 — same default-materialization problem as [`prim_move_to`]:
/// requiring zero arguments declined 2,628 of 3,481 real calls, all of
/// them the FO4/Starfield build of the same statement, making the
/// primitive silently game-asymmetric. The compiler-emitted literal
/// `false` is accepted; a literal `true` still declines, because
/// resetting the actor's AI is a second effect that
/// [`Effect::EvaluatePackage`] does not model (2,621 of the 2,628 pass
/// the default).
pub(super) fn prim_evaluate_package(e: &Expr, scope: &Scope) -> Option<Effect> {
    // M47.2 — `ReferenceAlias.TryToEvaluatePackage()` is `EvaluatePackage` on
    // the alias's actor when it is filled (the CK's own definition); an
    // empty alias resolves to nothing and is skipped at run time.
    if let Some((object, args)) = method_call(e, "TryToEvaluatePackage") {
        return args.is_empty().then(|| {
            receiver_object(object, scope).map(|actor| Effect::EvaluatePackage {
                actor: ActorRef::Object(actor),
            })
        })?;
    }
    let (object, args) = method_call(e, "EvaluatePackage")?;
    if args.len() > 1 || bool_arg(args, 0)?.unwrap_or(false) {
        return None;
    }
    Some(Effect::EvaluatePackage {
        actor: receiver_actor(object, scope)?,
    })
}
