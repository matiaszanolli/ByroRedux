//! Actor-state handlers: actor values, perks, factions, reputation, rewards, spells, teammate.

use super::*;

pub(super) fn add_to_faction(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, mut bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = {
        let Some(faction) = form_arg(args, 0) else { return Action::Declined };
        let change = match cmd {
            CMD_REMOVE_FROM_FACTION => FactionChange::Remove,
            // `AddToFaction <faction> <rank>`.
            _ => FactionChange::SetRank(num(args, 1).clamp(f32::from(i8::MIN), f32::from(i8::MAX)) as i8),
        };
        vec![Effect::ChangeFaction { actor: me, faction: bind.object(faction), change }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn add_spell(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, mut bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = {
        let Some(spell) = form_arg(args, 0) else { return Action::Declined };
        let spell = bind.object(spell);
        vec![if cmd == CMD_ADD_SPELL { Effect::AddSpell { actor: me, spell } } else { Effect::RemoveSpell { actor: me, spell } }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn add_achievement(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let bind = Binder::default();
    let effects: Vec<Effect> = vec![Effect::AddAchievement { id: num(args, 0) as i32 }];
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn set_player_teammate(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = {
        vec![Effect::SetPlayerTeammate { actor: me, teammate: num(args, 0) != 0.0, can_do_favor: false }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

// `SetEssential <ActorBase> <flag>`: the base is an argument, not the caller.
pub(super) fn set_essential(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let mut bind = Binder::default();
    let effects: Vec<Effect> = {
        let Some(base) = form_arg(args, 0) else { return Action::Declined };
        vec![Effect::SetBaseProtection {
            base: ActorBaseRef::Form(bind.object(base)),
            protected: false,
            value: num(args, 1) != 0.0,
        }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn set_av(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, mut bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = {
        let Some(avif) = crate::obscript_actor_values::actor_value_form(world, num(args, 0)) else {
            return Action::Declined;
        };
        let actor_value = bind.object(avif);
        let value = ValueExpr::Literal(num(args, 1));
        vec![match cmd {
            CMD_SET_AV => Effect::SetActorValue { actor: me, actor_value, value },
            CMD_DAMAGE_AV => Effect::DamageActorValue {
                actor: me,
                actor_value: ActorValueArg::Form(actor_value),
                amount: value,
                restore: false,
            },
            _ => Effect::DamageActorValue {
                actor: me,
                actor_value: ActorValueArg::Form(actor_value),
                amount: value,
                restore: true,
            },
        }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn mod_av(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let Some(avif) = crate::obscript_actor_values::actor_value_form(world, num(args, 0)) else {
        return Action::Declined;
    };
    let actor = caller_entity;
    // `ForceAV` is "the equivalent of ModAV by (value - GetAV)".
    let delta = if cmd == CMD_FORCE_AV {
        num(args, 1) - actor_value_view(world, actor, avif, ActorValueView::Current)
    } else {
        num(args, 1)
    };
    crate::fragment::mod_actor_value_permanent(world, actor, avif, delta);
    Action::Done { value: None, effect: format!("{} {caller:08X} {avif:08X} {delta}", if cmd == CMD_MOD_AV { "ModAV" } else { "ForceAV" }) }
}

// GECK: the flags are Movement, PipBoy, Fighting, POV, Looking, RolloverText,
// Sneaking; with none written, `EnablePlayerControls` is `1 1 1 1 1 1 1` and
// `DisablePlayerControls` is `1 1 1 1 0 0 0`. A flag names a domain and the
// call assigns it. The page's definitions give the mapping: Movement also
// takes world activation, PipBoy the pip-boy/wait menus (`menu`), POV camera
// switching. RolloverText has no domain here and is not applied.
pub(super) fn enable_player_controls(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let bind = Binder::default();
    let effects: Vec<Effect> = {
        let enable = cmd == CMD_ENABLE_PLAYER_CONTROLS;
        let flag = |i: usize, default: bool| if args.len() > i { num(args, i) != 0.0 } else { default };
        let defaults = if enable { [true; 7] } else { [true, true, true, true, false, false, false] };
        let [movement, pipboy, fighting, pov, looking, _rollover, sneaking] = std::array::from_fn(|i| flag(i, defaults[i]));
        vec![Effect::SetPlayerControls {
            enabled: enable,
            selection: crate::player_control::PlayerControlSelection {
                movement,
                fighting,
                camera_switching: pov,
                looking,
                sneaking,
                menu: pipboy,
                activation: movement,
                journal_tabs: false,
                pov_type: 0,
            },
        }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn add_perk(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, mut bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = {
        let Some(perk) = form_arg(args, 0) else { return Action::Declined };
        vec![Effect::ChangePerk { actor: me, perk: bind.object(perk), add: cmd == CMD_ADD_PERK }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

// `SetFactionRank <faction> <rank>`: rank `-1` removes the actor ("This will
// remove the player from the faction"). The GECK also sets it on the base, so
// every actor spawned from it later has it; here only this reference changes.
pub(super) fn set_faction_rank(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, mut bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = {
        let Some(faction) = form_arg(args, 0) else { return Action::Declined };
        let rank = num(args, 1);
        let change = if rank < 0.0 { FactionChange::Remove } else { FactionChange::SetRank(rank.min(f32::from(i8::MAX)) as i8) };
        vec![Effect::ChangeFaction { actor: me, faction: bind.object(faction), change }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn reward_xp(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let bind = Binder::default();
    let effects: Vec<Effect> = vec![Effect::RewardPlayerXp { amount: ItemCount::Literal(num(args, 0).max(0.0) as u32), direct: false }];
    finish_effects(world, cmd, context, caller, &bind, effects)
}

// `RewardKarma <points>`: karma is an actor value on the player.
pub(super) fn reward_karma(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _context: &Context) -> Action {
    let Some(avif) = crate::obscript_actor_values::actor_value_form(world, KARMA_CODE) else {
        return Action::Declined;
    };
    let Some(player) = crate::papyrus_demo::PapyrusPlayerEntity::get(world) else {
        return Action::Declined;
    };
    crate::fragment::mod_actor_value_permanent(world, player, avif, num(args, 0));
    Action::Done { value: None, effect: format!("RewardKarma {}", num(args, 0)) }
}

// `AddReputation <REPU> <fame 1 | infamy 0> <amount 1-5>`: the player's reputation with the
// faction rises by the points the amount names (1, 2, 4, 7, 12 by default).
pub(super) fn add_reputation(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _context: &Context) -> Action {
    let (Some(repu), Some(player)) = (form_arg(args, 0), crate::papyrus_demo::PapyrusPlayerEntity::get(world)) else {
        return Action::Declined;
    };
    let amount = num(args, 2);
    if !(1.0..=5.0).contains(&amount) || !matches!(num(args, 1) as i32, 0 | 1) {
        return Action::Declined;
    }
    let points = u16::from(byroredux_core::character::reputation_bump_points(amount as u8));
    if adjust_standing(world, player, repu, num(args, 1) as i32 == 1, points, false).is_none() {
        return Action::Declined;
    }
    Action::Done { value: None, effect: format!("AddReputation {repu:08X} +{points}") }
}

pub(super) fn reset_health(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let name = |what: &str| format!("{what} {caller:08X}");
    let actor = caller_entity;
    if !crate::obscript_actor_values::reset_health(world, actor) {
        return Action::Declined;
    }
    Action::Done { value: None, effect: name("ResetHealth") }
}
