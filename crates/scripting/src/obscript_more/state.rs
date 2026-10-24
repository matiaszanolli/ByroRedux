//! The ledger-, request- and engine-state commands of [`super`]: everything past the plain reads.

use byroredux_core::ecs::components::{Dead, GlobalTransform};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;

use super::{answer, caller, done, faction_ids, flag, ledger, ledger_mut, player, store, MenuRequest, PendingMenus, PendingSleep};
use crate::obscript_actions::{Action, Context};
use crate::obscript_cmd::{Command, Games};
use crate::obscript_effects::{form_arg, live, num};
use crate::obscript_ledger::FactionFlag;
use crate::obscript_vm::ObScriptValue;

/// The player's entity or, when a command names a calling reference, that reference's.
pub(super) fn caller_or_player(world: &World, ctx: &Context) -> Option<(u32, EntityId)> {
    caller(world, ctx).or_else(|| player(world).map(|entity| (0x14, entity)))
}

// ── the player's fame, infamy and bounty (Oblivion) ────────────────────────────────────────
// CS *GetPCFame* / *GetPCInfamy* / *ModPCFame*: "only the player character has fame or infamy values".
fn get_pc_fame(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    answer(ledger(world, |l| l.fame).unwrap_or(0.0))
}

fn cmd_10fb(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    answer(ledger(world, |l| l.infamy).unwrap_or(0.0))
}

fn cmd_10f8(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    ledger_mut(world, |l| l.fame += num(args, 0));
    done(format!("ModPCFame {}", num(args, 0)))
}

// CS *GetCrimeGold* / *SetCrimeGold* / *ModCrimeGold*: the player's bounty.
fn get_crime_gold(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    answer(ledger(world, |l| l.crime_gold).unwrap_or(0) as f32)
}

fn cmd_1075(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    ledger_mut(world, |l| l.crime_gold = num(args, 0) as i32);
    done(format!("SetCrimeGold {}", num(args, 0)))
}

fn cmd_1076(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    ledger_mut(world, |l| l.mod_crime_gold(num(args, 0) as i32));
    done(format!("ModCrimeGold {}", num(args, 0)))
}

// CS *PayFine* / *PayFineThief* / *GoToJail* (Oblivion): the fine, and the jail. A refusal (no
// crime model, cannot pay, no jail in reach, already serving) is a decline with nothing changed.
fn pay_fine(world: &World, _fallout: bool, cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    let (name, ok) = match cmd {
        0x107b => ("PayFine", crate::jail::pay_fine(world)),
        0x107c => ("PayFineThief", crate::jail::pay_fine_thief(world)),
        _ => ("GoToJail", crate::jail::go_to_jail(world)),
    };
    if ok {
        done(name)
    } else {
        Action::Declined
    }
}

// CS *GetAmountSoldStolen*: "the total value of stolen goods that the player has sold" — no sale
// of stolen goods is tracked, so it is what scripts have put there (0).
fn get_amount_sold_stolen(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    answer(ledger(world, |l| l.amount_sold_stolen).unwrap_or(0) as f32)
}

// CS *GetCrimeKnown <type> <criminal> <victim>*: "1 if the calling actor knows about the specified
// crime" — the crime record witnesses and the guards they call write. A crime type the engine
// does not carry is declined.
fn cmd_1077(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let (Some((knower, _)), Some(kind), Some(criminal), Some(victim)) =
        (caller(world, ctx), byroredux_core::character::CrimeKind::from_wire(num(args, 0) as u32), form_arg(args, 1), form_arg(args, 2))
    else {
        return Action::Declined;
    };
    flag(
        world
            .try_resource::<crate::crime_knowledge::CrimeKnowledge>()
            .is_some_and(|knowledge| knowledge.knows(knower, criminal, Some(kind), Some(victim))),
    )
}

// CS *Get/SetPCExpelled*, *Get/SetPCFactionMurder / Steal / Attack*.
fn cmd_10c1(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    let kind = match cmd {
        0x10c1 | 0x10c2 => FactionFlag::Expelled,
        0x10c3 | 0x10c4 => FactionFlag::Murder,
        0x10c5 | 0x10c6 => FactionFlag::Steal,
        _ => FactionFlag::Attack,
    };
    let Some(faction) = form_arg(args, 0) else { return Action::Declined };
    if matches!(cmd, 0x10c1 | 0x10c3 | 0x10c5 | 0x10c7) {
        flag(ledger(world, |l| l.flag(kind, faction)).unwrap_or(false))
    } else {
        let on = num(args, 1) != 0.0;
        ledger_mut(world, |l| l.set_flag(kind, faction, on));
        done(format!("{cmd:#06x} {faction:08X} {on}"))
    }
}

// GECK *GetPCEnemyOfFaction* (Fallout, in Oblivion's steal slot) / *SetPCEnemyOfFaction <faction> <flag>*.
fn get_pc_enemy_of_faction(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    let Some(faction) = form_arg(args, 0) else { return Action::Declined };
    if cmd == 0x10c5 {
        flag(ledger(world, |l| l.flag(FactionFlag::Enemy, faction)).unwrap_or(false))
    } else {
        let on = num(args, 1) != 0.0;
        ledger_mut(world, |l| l.set_flag(FactionFlag::Enemy, faction, on));
        done(format!("SetPCEnemyOfFaction {faction:08X} {on}"))
    }
}

// GECK *ClearFactionPlayerEnemyFlag <faction>*; *GetActorFactionPlayerEnemy*: "1 if the player has been
// flagged as an enemy of any of the targeted actor's factions due to crime".
fn get_actor_faction_player_enemy(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match form_arg(args, 0) {
        Some(faction) => {
            ledger_mut(world, |l| l.set_flag(FactionFlag::Enemy, faction, false));
            done(format!("ClearFactionPlayerEnemyFlag {faction:08X}"))
        }
        None => Action::Declined,
    }
}

fn cmd_11cc(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => {
            let factions = faction_ids(world, actor);
            flag(ledger(world, |l| l.flagged_with_any(FactionFlag::Enemy, &factions)).unwrap_or(false))
        }
        None => Action::Declined,
    }
}

// CS / GECK *SetFactionReaction <faction> <target faction> <value>*.
fn cmd_10a8(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match (form_arg(args, 0), form_arg(args, 1)) {
        (Some(faction), Some(other)) if crate::combat::set_faction_reaction(world, faction, other, num(args, 2)) => {
            done(format!("SetFactionReaction {faction:08X} {other:08X}"))
        }
        _ => Action::Declined,
    }
}

// GECK *GetFactionRelation <actor>*: 0 Neutral, 1 Enemy, 2 Ally, 3 Friend between the two actors.
fn cmd_11c2(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let target = |index: usize| form_arg(args, index).and_then(|form| live(world, form));
    match (caller_or_player(world, ctx), target(0)) {
        (Some((_, actor)), Some(other)) => match crate::combat::faction_reaction(world, actor, other) {
            Some(reaction) => answer(reaction as f32),
            None => Action::Declined,
        },
        _ => Action::Declined,
    }
}

// GECK *SendTrespassAlarm <actor>*: a trespass crime nobody records: the faction-enemy flag is what
// a crime-tracking faction sets on a major crime, and a trespass is not one. Accepted, no effect.
fn cmd_115c(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    done("SendTrespassAlarm")
}

// ── disposition (Oblivion): *ModDisposition* declines — nothing in the engine reads an NPC's
// disposition towards the player, so a stored mod would be written and never consulted ────────
fn mod_disposition(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    Action::Declined
}

// ── topics, talking, quest objects ─────────────────────────────────────────────────────────
// CS *RefreshTopicList*: the topic list is built when a conversation opens; nothing is cached.
fn refresh_topic_list(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    done("RefreshTopicList")
}

// *SetQuestObject* declines: nothing models the quest-item flag (no drop / sell / container-take
// guard reads it), so storing it would only be saved.
fn set_quest_object(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    Action::Declined
}

// CS *SetShowQuestItems <flag>* (a global).
fn cmd_114e(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    store(world, cmd, 0, num(args, 0))
}

// ── sleeping, time ─────────────────────────────────────────────────────────────────────────
// GECK *SetPCSleepHours <hours>*: "manually forces the player to sleep the specified number of hours".
fn cmd_10bd(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    if let Some(mut sleep) = world.try_resource_mut::<PendingSleep>() {
        sleep.0 += num(args, 0).max(0.0);
    }
    done(format!("SetPCSleepHours {}", num(args, 0)))
}

// GECK *IsPCSleeping* / *IsTimePassing* ("sleeping, resting, or traveling"): sleeping hours queued.
fn is_pc_sleeping(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    flag(world.try_resource::<PendingSleep>().is_some_and(|sleep| sleep.0 > 0.0))
}

// GECK *WakeUpPC*: "manually closes the wait/sleep menu (as if the player had pressed Cancel)".
fn wake_up_pc(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    if let Some(mut sleep) = world.try_resource_mut::<PendingSleep>() {
        sleep.0 = 0.0;
    }
    done("WakeUpPC")
}

// ── flags and values a Set command keeps for its Get ───────────────────────────────────────
// GECK *SetForceSneak* / *GetForceSneak*; *SetForceRun* (Oblivion): the actor's forced stance. The
// engine has no sneak or run stance to apply it to, so it is kept and read back.
fn set_force_sneak(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller_or_player(world, ctx) {
        Some((form, _)) => store(world, cmd, form, num(args, 0)),
        None => Action::Declined,
    }
}

fn cmd_10d2(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller_or_player(world, ctx) {
        Some((form, _)) => answer(ledger(world, |l| l.value(0x10d3, form)).flatten().unwrap_or(0.0)),
        None => Action::Declined,
    }
}

// GECK *SetIgnoreFriendlyHits <flag>*: "so actor will ignore friendly hits".
fn cmd_1151(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => store(world, cmd, form, num(args, 0)),
        None => Action::Declined,
    }
}

// GECK *GetIgnoreFriendlyHits* (`GIFH`): the flag `SetIgnoreFriendlyHits` stored for the actor.
fn get_ignore_friendly_hits(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => answer(ledger(world, |l| l.value(0x1151, form)).flatten().unwrap_or(0.0)),
        None => Action::Declined,
    }
}

// GECK *SetCellPublicFlag <cell> <flag>*: "cells which are public cannot have trespassing".
fn cmd_10bb(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match form_arg(args, 0) {
        Some(cell) => store(world, cmd, cell, num(args, 1)),
        None => Action::Declined,
    }
}

// CS *SetDoorDefaultOpen <flag>*: the door's default open state.
fn cmd_10d8(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => store(world, cmd, form, num(args, 0)),
        None => Action::Declined,
    }
}

// GECK *SetActorFullName <message>*, *SetTalkingActivatorActor [actor]*, *SetCombatStyle <style>*,
// *SetMerchantContainer <container>*: a form kept for the reference.
fn cmd_111c(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((subject, _)) => {
            let form = form_arg(args, 0).unwrap_or(0);
            ledger_mut(world, |l| l.set_form(cmd, subject, form));
            done(format!("{cmd:#06x} {subject:08X} {form:08X}"))
        }
        None => Action::Declined,
    }
}

// GECK *SetItemValue <caps>* declines: the base value is read straight from the record by barter,
// repair and the inventory weight/value totals, and nothing overlays a script's override.
fn cmd_1155(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    Action::Declined
}

// GECK *SetActorAlpha <0..1>* (SAA): the app's alpha system reads it back from the ledger.
fn cmd_1129(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => store(world, cmd, form, num(args, 0).clamp(0.0, 1.0)),
        None => Action::Declined,
    }
}

// GECK *SetActorsAI <0|nonzero>*: "turns off and on the processing of an actor" — the app's AI systems
// skip an actor whose flag reads 0.
fn cmd_116f(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => store(world, 0x118b, form, if num(args, 0) == 0.0 { 0.0 } else { 1.0 }),
        None => Action::Declined,
    }
}

// GECK *SetAllReachable / SetAllVisible / SetNoAvoidance / SetSceneIsComplex*: "increase the game's
// performance ... the flag is cleared every frame". They change cost, not results: kept for the frame.
fn cmd_1159(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    store(world, cmd, 0, num(args, 0))
}

// GECK *SetVATSTarget <flag>*: VATS targeting of a destructible object.
fn cmd_11eb(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => store(world, cmd, form, num(args, 0)),
        None => Action::Declined,
    }
}

// CS *SetPackDuration <minutes>*: the current package's duration — kept; the ambient AI re-evaluates
// the actor when it elapses.
fn cmd_1122(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, actor)) => {
            crate::package::request_package_evaluation(world, actor);
            store(world, cmd, form, num(args, 0))
        }
        None => Action::Declined,
    }
}

// Menus and screens with no UI here: the request is queued for the UI that shows them.
fn cmd_1189(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    done("PurgeCellBuffers")
}

fn cmd_1164(world: &World, _fallout: bool, cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    if let Some(mut menus) = world.try_resource_mut::<PendingMenus>() {
        menus.0.push(MenuRequest { menu: if cmd == 0x1164 { "dialog subtitles" } else { "birthsign" }, form: 0 });
    }
    done(format!("{cmd:#06x}"))
}

// CS *ForceCloseOblivionGate*: the gate quests' state is the scripts' own; recorded for them.
fn force_close_oblivion_gate(world: &World, _fallout: bool, cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => store(world, cmd, form, 1.0),
        None => Action::Declined,
    }
}

// CS *SetPlayerInSEWorld <flag>* / *GetPlayerInSEWorld* (Oblivion; `0x116c` is DamageObject in
// Fallout): "a game engine flag indicating whether the player is in the Shivering Isles".
fn get_player_in_se_world(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    store(world, cmd, 0, num(args, 0))
}

fn cmd_116d(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    answer(super::player_in_se_world(world))
}

// CS *IsActorUsingATorch*: "1 if the calling actor is currently using a torch (i.e. has it out)". The
// equipment model carries armor and a weapon, not a light, so no actor can have a torch out.
fn is_actor_using_a_torch(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some(_) => flag(false),
        None => Action::Declined,
    }
}

// CS *ClearOwnership*: "clears the ownership from the reference it is called on" — Fallout's id for
// the same command is lowered by the effects table.
fn clear_ownership(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    crate::obscript_cmd::run(world, 0x118c, args, ctx)
}

// CS *ResetFallDamageTimer*: the controller's fall is measured from where it left the ground; the
// timer is the airborne time, which the character system owns.
fn reset_fall_damage_timer(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    done("ResetFallDamageTimer")
}

// CS *SetRigidBodyMass <0..100>*: "how much it moves when it is bumped ... 100 being no movement".
fn cmd_1156(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => store(world, cmd, form, num(args, 0).clamp(0.0, 100.0)),
        None => Action::Declined,
    }
}

// Oblivion path grids: *EnableLinkedPathPoints* / *DisableLinkedPathPoints*: the engine paths on
// navmeshes, so the flag is kept for the grid that would read it.
fn enable_linked_path_points(world: &World, _fallout: bool, cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => store(world, 0x112a, form, if cmd == 0x112a { 1.0 } else { 0.0 }),
        None => Action::Declined,
    }
}

// CS *CreateFullActorCopy*: "creates a copy of the specified actor and its base ... and returns the
// ID of the new copy". A base record cannot be created at run time, so the copy is a fresh
// reference of the same base, placed at the original — its inventory and values are the base's, not
// what the original carried.
fn create_full_actor_copy(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller_or_player(world, ctx) {
        Some((at, _)) => {
            let Some(base) = live(world, at).and_then(|entity| world.get::<crate::scene::SceneAliasCandidate>(entity).map(|c| c.base_form_id)).filter(|&b| b != 0).or_else(|| {
                // The player has no placed-reference candidate: its base is the player NPC record.
                (at == crate::PLAYER_REFERENCE).then_some(crate::relationships::PLAYER_BASE)
            }) else {
                return Action::Declined;
            };
            let Some(mut created) = world.try_resource_mut::<crate::CreatedReferences>() else { return Action::Declined };
            let Some(form) = created.scripted_ids().next() else { return Action::Declined };
            created.create_scripted(form, base, at, false);
            Action::Done { value: Some(ObScriptValue::Ref(form)), effect: format!("CreateFullActorCopy {at:08X} as {form:08X}") }
        }
        None => Action::Declined,
    }
}

// CS *GetIsAlerted*: "1 if the actor is in an alerted state" — a drawn weapon (`SetAlert`) or combat.
fn get_is_alerted(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => flag(crate::obscript_state::is_weapon_out(world, actor)),
        None => Action::Declined,
    }
}

// ── engine state reads ─────────────────────────────────────────────────────────────────────
// GECK *IsSpellTarget <magic item>*: 1 when an effect of that spell, enchantment or ingestible is on
// the reference (a carried ability counts: "this will return 1 if Buddy has the specified Actor Ability").
fn cmd_10df(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller_or_player(world, ctx), form_arg(args, 0)) {
        (Some((_, actor)), Some(magic)) => flag(
            world.get::<crate::magic_effects::ActiveMagicEffects>(actor).is_some_and(|active| active.0.iter().any(|e| e.source == magic))
                || world.get::<crate::magic::SpellList>(actor).is_some_and(|list| list.0.contains(&magic)),
        ),
        _ => Action::Declined,
    }
}

// GECK *IsOwner [faction|actor]*: 1 when the object is owned by that faction or NPC — or by the player
// with no argument; "returns 0 if the owner is set to NONE".
fn cmd_1116(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, object)) => {
            let owner = crate::ownership::owner_override(world, object)
                .or_else(|| world.get::<crate::ownership::ExplicitOwner>(object).map(|owner| owner.0));
            match (owner, form_arg(args, 0)) {
                (Some(owner), Some(asked)) => flag(owner == asked),
                (Some(owner), None) => flag(owner == 0x14 || owner == crate::relationships::PLAYER_BASE),
                (None, _) => flag(false),
            }
        }
        None => Action::Declined,
    }
}

// GECK *IsInList <form list>*: 1 when the reference's base object is in the list.
fn cmd_1174(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller(world, ctx), form_arg(args, 0)) {
        (Some((_, entity)), Some(list)) => {
            let base = world.get::<crate::scene::SceneAliasCandidate>(entity).map(|c| c.base_form_id).unwrap_or(0);
            match crate::form_lists::forms_of(world, list, &[]) {
                Some(forms) => flag(base != 0 && forms.contains(&base)),
                None => Action::Declined,
            }
        }
        _ => Action::Declined,
    }
}

// GECK *IsWeaponInList <form list>*: 1 when the actor's equipped weapon is in the list.
fn cmd_118f(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller_or_player(world, ctx), form_arg(args, 0)) {
        (Some((_, actor)), Some(list)) => {
            let weapon = world.get::<byroredux_core::ecs::components::EquippedWeapon>(actor).map(|w| w.base_form_id);
            match (weapon, crate::form_lists::forms_of(world, list, &[])) {
                (Some(weapon), Some(forms)) => flag(forms.contains(&weapon)),
                (None, Some(_)) => flag(false),
                _ => Action::Declined,
            }
        }
        _ => Action::Declined,
    }
}

// CS / GECK *IsActor*, *IsEssential*.
fn is_actor(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, entity)) => flag(world.has::<byroredux_core::ecs::components::ActorVitals>(entity)),
        None => Action::Declined,
    }
}

fn cmd_1162(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller_or_player(world, ctx) {
        Some((_, actor)) => flag(world.get::<crate::ActorProtection>(actor).is_some_and(|p| p.essential)),
        None => Action::Declined,
    }
}

// GECK *GetAnimAction*: -1 none, 2 attack ... — only what the engine's state shows: attacking while in
// combat is ATTACK (2), else none.
fn get_anim_action(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => answer(if crate::is_in_combat(world, actor) && world.has::<byroredux_core::ecs::components::EquippedWeapon>(actor) { 2.0 } else { -1.0 }),
        None => Action::Declined,
    }
}

// GECK *IsPlayerMovingIntoNewSpace*: 1 while the player loads into a new cell — no cell is resident
// for the frames the transition takes.
fn is_player_moving_into_new_space(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    flag(world.try_resource::<crate::cell_kind::PlayerCellKind>().is_some_and(|kind| kind.0.is_none()))
}

// GECK *IsPC1stPerson*: the camera mode, through the query the app installs.
fn is_pc1st_person(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    match super::engine_queries(world).first_person {
        Some(first) => flag(first(world)),
        None => Action::Declined,
    }
}

// GECK *IsPS3* / *IsGoreDisabled*: the engine is neither a PS3 build nor has a gore-off setting.
fn is_ps3(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    flag(false)
}

// GECK *HasLoaded3D*: the reference has a placement in the loaded world.
fn has_loaded3_d(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, entity)) => flag(world.has::<GlobalTransform>(entity)),
        None => Action::Declined,
    }
}

// GECK *IsPlayerGrabbedRef <ref>*: the engine has no grab; nothing is grabbed.
fn cmd_11d0(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    flag(false)
}

// GECK *IsRidingHorse* (Oblivion): the engine has no mounts.
fn is_riding_horse(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    flag(false)
}

// GECK *IsLimbGone <limb>*: "tests if an actor has had a particular limb dismembered" — the engine
// keeps no dismemberment, so none is gone. *GetKillingBlowLimb*: -1 for a living actor; a corpse's
// killing blow location is not kept.
fn get_killing_blow_limb(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    flag(false)
}

fn cmd_11f0(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) if !world.has::<Dead>(actor) => answer(-1.0),
        _ => Action::Declined,
    }
}

// GECK *IsCombatTarget <actor>*: 1 when the calling actor is fighting that actor.
fn cmd_1203(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let target = |index: usize| form_arg(args, index).and_then(|form| live(world, form));
    match (caller(world, ctx), target(0)) {
        (Some((_, actor)), Some(other)) => flag(world.get::<crate::AiCombatState>(actor).is_some_and(|state| state.target == other)),
        _ => Action::Declined,
    }
}

// GECK *GetPermanentActorValue <av>*: base plus the permanent layers, no situational bonus.
fn cmd_11ef(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller_or_player(world, ctx), crate::obscript_actor_values::actor_value_form(world, num(args, 0))) {
        (Some((_, actor)), Some(avif)) => match world.get::<byroredux_core::ecs::components::ActorValues>(actor).and_then(|v| v.get(avif).copied()) {
            Some(value) => answer(value.base + value.set_override + value.permanent_mod),
            None => Action::Declined,
        },
        _ => Action::Declined,
    }
}

// GECK *GetWeaponHealthPerc* / *ModWeaponHealthPerc <delta>*: the condition of the equipped weapon,
// a percent that scripts can change (nothing else wears a weapon).
fn get_weapon_health_perc(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller_or_player(world, ctx) {
        Some((form, actor)) => match world.get::<byroredux_core::ecs::components::EquippedWeapon>(actor).map(|w| w.base_form_id) {
            Some(weapon) => {
                if cmd == 0x11f4 {
                    answer(ledger(world, |l| l.weapon_condition(form, weapon)).unwrap_or(100.0))
                } else {
                    let now = ledger(world, |l| l.weapon_condition(form, weapon)).unwrap_or(100.0);
                    ledger_mut(world, |l| l.set_weapon_condition(form, weapon, now + num(args, 0)));
                    done(format!("ModWeaponHealthPerc {form:08X} {}", num(args, 0)))
                }
            }
            None => Action::Declined,
        },
        None => Action::Declined,
    }
}

// GECK *GetContainerInventoryCount*: the number of items in the container.
fn get_container_inventory_count(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, entity)) => match world.get::<byroredux_core::ecs::components::Inventory>(entity) {
            Some(inventory) => answer(inventory.items.iter().map(|stack| stack.count as f32).sum()),
            None => answer(0.0),
        },
        None => Action::Declined,
    }
}

// GECK *GetMapMarkerVisible*: 0 undiscovered, 1 visible, 2 visible and travelable.
fn get_map_marker_visible(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => answer(if crate::map_marker_fast_travel(world, form) {
            2.0
        } else if crate::map_marker_visible(world, form) {
            1.0
        } else {
            0.0
        }),
        None => Action::Declined,
    }
}

// GECK *IsActorDetected* (Oblivion): 1 when any other actor detects the calling actor; 0 for the player.
fn is_actor_detected(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller(world, ctx), world.try_resource::<crate::SightQuery>().map(|q| q.0)) {
        (Some((_, actor)), Some(sight)) => {
            if Some(actor) == player(world) {
                return flag(false);
            }
            let observers: Vec<EntityId> = world
                .query::<byroredux_core::ecs::components::ActorVitals>()
                .map(|q| q.iter().map(|(entity, _)| entity).collect())
                .unwrap_or_default();
            flag(observers.into_iter().any(|observer| observer != actor && !world.has::<Dead>(observer) && sight(world, observer, actor)))
        }
        _ => Action::Declined,
    }
}

/// The commands this file runs (#6139).
pub(crate) const STATE_COMMANDS: &[Command] = &[
    Command { id: 0x10f9, games: Games::Oblivion, needs_caller: false, handler: get_pc_fame },
    Command { id: 0x10fb, games: Games::Oblivion, needs_caller: false, handler: cmd_10fb },
    Command { id: 0x10f8, games: Games::Oblivion, needs_caller: false, handler: cmd_10f8 },
    Command { id: 0x1074, games: Games::Oblivion, needs_caller: false, handler: get_crime_gold },
    Command { id: 0x1075, games: Games::Oblivion, needs_caller: false, handler: cmd_1075 },
    Command { id: 0x1076, games: Games::Oblivion, needs_caller: false, handler: cmd_1076 },
    Command { id: 0x107b, games: Games::Oblivion, needs_caller: false, handler: pay_fine },
    Command { id: 0x107c, games: Games::Oblivion, needs_caller: false, handler: pay_fine },
    Command { id: 0x107e, games: Games::Oblivion, needs_caller: false, handler: pay_fine },
    Command { id: 0x10be, games: Games::Oblivion, needs_caller: false, handler: get_amount_sold_stolen },
    Command { id: 0x1077, games: Games::Oblivion, needs_caller: false, handler: cmd_1077 },
    Command { id: 0x10c1, games: Games::Oblivion, needs_caller: false, handler: cmd_10c1 },
    Command { id: 0x10c2, games: Games::Oblivion, needs_caller: false, handler: cmd_10c1 },
    Command { id: 0x10c3, games: Games::Oblivion, needs_caller: false, handler: cmd_10c1 },
    Command { id: 0x10c4, games: Games::Oblivion, needs_caller: false, handler: cmd_10c1 },
    Command { id: 0x10c5, games: Games::Oblivion, needs_caller: false, handler: cmd_10c1 },
    Command { id: 0x10c6, games: Games::Oblivion, needs_caller: false, handler: cmd_10c1 },
    Command { id: 0x10c7, games: Games::Oblivion, needs_caller: false, handler: cmd_10c1 },
    Command { id: 0x10c5, games: Games::Fallout, needs_caller: false, handler: get_pc_enemy_of_faction },
    Command { id: 0x10c6, games: Games::Fallout, needs_caller: false, handler: get_pc_enemy_of_faction },
    Command { id: 0x11c9, games: Games::Fallout, needs_caller: false, handler: get_actor_faction_player_enemy },
    Command { id: 0x11cc, games: Games::Fallout, needs_caller: false, handler: cmd_11cc },
    Command { id: 0x10a8, games: Games::Both, needs_caller: false, handler: cmd_10a8 },
    Command { id: 0x11c2, games: Games::Fallout, needs_caller: false, handler: cmd_11c2 },
    Command { id: 0x115c, games: Games::Both, needs_caller: false, handler: cmd_115c },
    Command { id: 0x1053, games: Games::Oblivion, needs_caller: false, handler: mod_disposition },
    Command { id: 0x1145, games: Games::Oblivion, needs_caller: false, handler: refresh_topic_list },
    Command { id: 0x110d, games: Games::Both, needs_caller: false, handler: set_quest_object },
    Command { id: 0x114e, games: Games::Oblivion, needs_caller: false, handler: cmd_114e },
    Command { id: 0x10bd, games: Games::Fallout, needs_caller: false, handler: cmd_10bd },
    Command { id: 0x10af, games: Games::Both, needs_caller: false, handler: is_pc_sleeping },
    Command { id: 0x1109, games: Games::Oblivion, needs_caller: false, handler: is_pc_sleeping },
    Command { id: 0x10ae, games: Games::Both, needs_caller: false, handler: wake_up_pc },
    Command { id: 0x10d3, games: Games::Both, needs_caller: false, handler: set_force_sneak },
    Command { id: 0x10d1, games: Games::Oblivion, needs_caller: false, handler: set_force_sneak },
    Command { id: 0x10d2, games: Games::Both, needs_caller: false, handler: cmd_10d2 },
    Command { id: 0x1151, games: Games::Both, needs_caller: false, handler: cmd_1151 },
    Command { id: 0x1152, games: Games::Fallout, needs_caller: false, handler: get_ignore_friendly_hits },
    Command { id: 0x10bb, games: Games::Both, needs_caller: false, handler: cmd_10bb },
    Command { id: 0x10d8, games: Games::Oblivion, needs_caller: false, handler: cmd_10d8 },
    Command { id: 0x111c, games: Games::Both, needs_caller: false, handler: cmd_111c },
    Command { id: 0x1171, games: Games::Fallout, needs_caller: false, handler: cmd_111c },
    Command { id: 0x10b1, games: Games::Both, needs_caller: false, handler: cmd_111c },
    Command { id: 0x11b7, games: Games::Fallout, needs_caller: false, handler: cmd_111c },
    Command { id: 0x1155, games: Games::Both, needs_caller: false, handler: cmd_1155 },
    Command { id: 0x1129, games: Games::Both, needs_caller: false, handler: cmd_1129 },
    Command { id: 0x116f, games: Games::Oblivion, needs_caller: false, handler: cmd_116f },
    Command { id: 0x118b, games: Games::Fallout, needs_caller: false, handler: cmd_116f },
    Command { id: 0x1159, games: Games::Both, needs_caller: false, handler: cmd_1159 },
    Command { id: 0x115a, games: Games::Both, needs_caller: false, handler: cmd_1159 },
    Command { id: 0x115b, games: Games::Both, needs_caller: false, handler: cmd_1159 },
    Command { id: 0x115d, games: Games::Both, needs_caller: false, handler: cmd_1159 },
    Command { id: 0x11eb, games: Games::Fallout, needs_caller: false, handler: cmd_11eb },
    Command { id: 0x1122, games: Games::Both, needs_caller: false, handler: cmd_1122 },
    Command { id: 0x1189, games: Games::Fallout, needs_caller: false, handler: cmd_1189 },
    Command { id: 0x116b, games: Games::Oblivion, needs_caller: false, handler: cmd_1189 },
    Command { id: 0x1164, games: Games::Oblivion, needs_caller: false, handler: cmd_1164 },
    Command { id: 0x10db, games: Games::Oblivion, needs_caller: false, handler: cmd_1164 },
    Command { id: 0x1165, games: Games::Oblivion, needs_caller: false, handler: force_close_oblivion_gate },
    Command { id: 0x116c, games: Games::Oblivion, needs_caller: false, handler: get_player_in_se_world },
    Command { id: 0x116d, games: Games::Oblivion, needs_caller: false, handler: cmd_116d },
    Command { id: 0x1132, games: Games::Oblivion, needs_caller: false, handler: is_actor_using_a_torch },
    Command { id: 0x1170, games: Games::Oblivion, needs_caller: false, handler: clear_ownership },
    Command { id: 0x1134, games: Games::Oblivion, needs_caller: false, handler: reset_fall_damage_timer },
    Command { id: 0x1156, games: Games::Oblivion, needs_caller: false, handler: cmd_1156 },
    Command { id: 0x112a, games: Games::Oblivion, needs_caller: false, handler: enable_linked_path_points },
    Command { id: 0x112b, games: Games::Oblivion, needs_caller: false, handler: enable_linked_path_points },
    Command { id: 0x1167, games: Games::Oblivion, needs_caller: false, handler: create_full_actor_copy },
    Command { id: 0x105b, games: Games::Oblivion, needs_caller: false, handler: get_is_alerted },
    Command { id: 0x10df, games: Games::Both, needs_caller: false, handler: cmd_10df },
    Command { id: 0x1116, games: Games::Both, needs_caller: false, handler: cmd_1116 },
    Command { id: 0x1174, games: Games::Fallout, needs_caller: false, handler: cmd_1174 },
    Command { id: 0x118f, games: Games::Fallout, needs_caller: false, handler: cmd_118f },
    Command { id: 0x1161, games: Games::Oblivion, needs_caller: false, handler: is_actor },
    Command { id: 0x1162, games: Games::Both, needs_caller: false, handler: cmd_1162 },
    Command { id: 0x10db, games: Games::Fallout, needs_caller: false, handler: get_anim_action },
    Command { id: 0x1166, games: Games::Fallout, needs_caller: false, handler: is_player_moving_into_new_space },
    Command { id: 0x1188, games: Games::Fallout, needs_caller: false, handler: is_pc1st_person },
    Command { id: 0x120b, games: Games::Fallout, needs_caller: false, handler: is_ps3 },
    Command { id: 0x1226, games: Games::Fallout, needs_caller: false, handler: is_ps3 },
    Command { id: 0x122e, games: Games::Fallout, needs_caller: false, handler: has_loaded3_d },
    Command { id: 0x11d0, games: Games::Fallout, needs_caller: false, handler: cmd_11d0 },
    Command { id: 0x1147, games: Games::Oblivion, needs_caller: false, handler: is_riding_horse },
    Command { id: 0x118e, games: Games::Fallout, needs_caller: false, handler: get_killing_blow_limb },
    Command { id: 0x11f0, games: Games::Fallout, needs_caller: false, handler: cmd_11f0 },
    Command { id: 0x1203, games: Games::Fallout, needs_caller: false, handler: cmd_1203 },
    Command { id: 0x11ef, games: Games::Fallout, needs_caller: false, handler: cmd_11ef },
    Command { id: 0x11f4, games: Games::Fallout, needs_caller: false, handler: get_weapon_health_perc },
    Command { id: 0x11f6, games: Games::Fallout, needs_caller: false, handler: get_weapon_health_perc },
    Command { id: 0x127a, games: Games::Fallout, needs_caller: false, handler: get_container_inventory_count },
    Command { id: 0x11ec, games: Games::Fallout, needs_caller: false, handler: get_map_marker_visible },
    Command { id: 0x10b5, games: Games::Oblivion, needs_caller: false, handler: is_actor_detected },
];

