//! Combat and alarm handlers: kills, hostility, combat start/stop, crime alarms, AI reset.

use super::*;

/// The actor an assault alarm is about: `actor` when it is a live non-player actor, else the loaded,
/// living member of `faction` standing nearest the player.
pub(super) fn assault_victim(world: &World, actor: Option<u32>, faction: Option<u32>) -> Option<EntityId> {
    use byroredux_core::ecs::components::{Dead, FactionRanks, GlobalTransform};
    let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world);
    let usable = |entity: EntityId| Some(entity) != player && !world.has::<Dead>(entity);
    if let Some(named) = actor.and_then(|form| live(world, form)).filter(|&entity| usable(entity)) {
        return Some(named);
    }
    let faction = faction?;
    let origin = player.and_then(|player| world.get::<GlobalTransform>(player)).map(|t| t.translation);
    let members = world.query::<FactionRanks>()?;
    members
        .iter()
        .filter(|(entity, ranks)| usable(*entity) && ranks.0.iter().any(|(id, rank)| *id == faction && *rank >= 0))
        .filter_map(|(entity, _)| {
            let at = world.get::<GlobalTransform>(entity)?.translation;
            Some((entity, origin.map_or(0.0, |origin| origin.distance(at))))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(entity, _)| entity)
}

pub(super) fn kill_actor(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, mut bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = {
        let killer = form_arg(args, 0).map(|killer| bind.actor(killer));
        vec![Effect::Kill { actor: me, killer, silent: false }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn set_enemy(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let mut bind = Binder::default();
    let effects: Vec<Effect> = {
        let (Some(first), Some(second)) = (form_arg(args, 0), form_arg(args, 1)) else {
            return Action::Declined;
        };
        let (faction, other_faction) = (bind.object(first), bind.object(second));
        let (self_flag, other_flag) = (num(args, 2) != 0.0, num(args, 3) != 0.0);
        vec![if cmd == CMD_SET_ENEMY {
            Effect::SetEnemy { faction, other_faction, self_neutral: self_flag, other_neutral: other_flag }
        } else {
            Effect::SetAlly { faction, other_faction, self_friend: self_flag, other_friend: other_flag }
        }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn start_combat(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, mut bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = {
        let Some(target) = form_arg(args, 0) else { return Action::Declined };
        if live(world, target).is_none() {
            return Action::Declined;
        }
        vec![Effect::StartCombat { actor: me, target: bind.actor(target) }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn stop_combat(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity, bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = {
        // An actor has one combat target: naming another one stops nothing.
        if let Some(target) = form_arg(args, 0) {
            let fighting = world.get::<crate::AiCombatState>(caller_entity).map(|state| state.target);
            if fighting.is_none() || fighting != live(world, target) {
                return Action::Done { value: None, effect: String::new() };
            }
        }
        vec![Effect::StopCombat { actor: me }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn stop_combat_alarm(world: &World, _fallout: bool, cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = vec![Effect::StopCombatAlarm { actor: me }];
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn set_unconscious(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = vec![Effect::SetUnconscious { actor: me, unconscious: num(args, 0) != 0.0 }];
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn set_ghost(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = vec![Effect::SetGhost { actor: me, ghost: num(args, 0) != 0.0 }];
    finish_effects(world, cmd, context, caller, &bind, effects)
}

// `SetAlert <flag>`: the actor draws (or lowers) its weapon; `IgnoreCrime <flag>`: it neither
// attacks the player for crime nor reports it.
pub(super) fn set_alert(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let entity = caller_entity;
    let on = num(args, 0) != 0.0;
    if cmd == CMD_SET_ALERT {
        set_flag::<crate::obscript_state::WeaponDrawn>(world, entity, on, crate::obscript_state::WeaponDrawn)
    } else {
        set_flag::<crate::obscript_state::IgnoreCrime>(world, entity, on, crate::obscript_state::IgnoreCrime)
    }
    Action::Done { value: None, effect: format!("{} {caller:08X} {on}", if cmd == CMD_SET_ALERT { "SetAlert" } else { "IgnoreCrime" }) }
}

// `SendAssaultAlarm [actor] [faction]`: "manually creates an assault crime ... Flash will react
// as if he is a victim of an assault crime. If Flash is not there, an NPC in the faction will be
// used instead; `SendAssaultAlarm Player BigTown` ignores the ActorRef parameter and searches
// for an NPC in the faction". The victim (the named actor, else the faction's loaded member
// nearest the player) turns on the player. There is no witness / bounty runtime, so the crime
// itself is not recorded.
pub(super) fn send_assault_alarm(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let mut bind = Binder::default();
    let effects: Vec<Effect> = {
        let Some(victim) = assault_victim(world, form_arg(args, 0), form_arg(args, 1)) else {
            return Action::Declined;
        };
        let Some(victim_form) = crate::obscript_state::reference_form(world, victim) else { return Action::Declined };
        let victim_ref = bind.actor(victim_form);
        let player = bind.actor(0x14);
        vec![Effect::StartCombat { actor: victim_ref, target: player }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

// `Resurrect <animate>`: "resurrects the calling actor" (CS) from the dead or unconscious state
// (GECK). The revival itself — the ragdoll torn down, health back, the idle and the AI resumed —
// belongs to the app (`ReviveRequest`); an actor that is neither dead nor unconscious has nothing
// to resurrect.
pub(super) fn resurrect(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let actor = caller_entity;
    let down = world.has::<byroredux_core::ecs::components::Dead>(actor)
        || world.get::<crate::ActorControlState>(actor).is_some_and(|state| state.unconscious);
    if !down {
        return Action::Done { value: None, effect: format!("Resurrect {caller:08X} (not down)") };
    }
    let Some(mut requests) = world.query_mut::<crate::obscript_state::ReviveRequest>() else { return Action::Declined };
    requests.insert(actor, crate::obscript_state::ReviveRequest);
    Action::Done { value: None, effect: format!("Resurrect {caller:08X}") }
}

pub(super) fn set_restrained(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let actor = caller_entity;
    let restrained = num(args, 0) != 0.0;
    if !crate::update_actor_control(world, actor, |state| state.set_restrained(restrained)) {
        return Action::Declined;
    }
    Action::Done { value: None, effect: format!("SetRestrained {caller:08X} {restrained}") }
}

// GECK: "clears all current AI behaviors from an actor. Combat, pathing, and
// packages are re-evaluated."
pub(super) fn reset_ai(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let actor = caller_entity;
    if let Some(mut states) = world.query_mut::<crate::AiCombatState>() {
        states.remove(actor);
    }
    crate::script_packages::remove_script_package(world, actor);
    crate::package::request_package_evaluation(world, actor);
    Action::Done { value: None, effect: format!("ResetAI {caller:08X}") }
}
