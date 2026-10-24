//! The action commands of [`super`] that reach other runtimes: quests, destruction, projectiles,
//! triggers, the knockdown and flee requests, placement, menus and the flags the UI reads.

use byroredux_core::ecs::World;

use super::{answer, caller, done, flag, ledger, ledger_mut, player, store, MenuRequest, PendingMenus, PushActorRequest};
use crate::obscript_actions::{Action, Context};
use crate::obscript_cmd::{Command, Games};
use crate::obscript_effects::{form_arg, live, num};
use crate::obscript_vm::ObScriptValue;

// ── quests ────────────────────────────────────────────────────────────────────────────────
// GECK *ResetQuest <quest>*: "clears the quest stage list, the objective list, all the quest
// variables; resets the quest stage to its default state". (The quest's own locals are the
// `ObScriptQuestLocals` the quest tick keeps.)
fn ob_script_quest_locals(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match form_arg(args, 0) {
        Some(quest) => {
            let quest = crate::quest_stages::QuestFormId(quest);
            if let Some(mut stages) = world.try_resource_mut::<crate::quest_stages::QuestStageState>() {
                stages.reset(quest);
            }
            if let Some(mut objectives) = world.try_resource_mut::<crate::quest_stages::QuestObjectiveState>() {
                objectives.reset(quest);
            }
            done(format!("ResetQuest {:08X}", quest.0))
        }
        None => Action::Declined,
    }
}

// GECK *CompleteAllObjectives <quest>*: "marks all objectives for a quest as completed".
fn cmd_11ad(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match form_arg(args, 0) {
        Some(quest) => {
            let quest = crate::quest_stages::QuestFormId(quest);
            // The definitions first, then the objective state: the order the story manager takes them.
            let authored = world
                .try_resource::<crate::quest_stages::QuestDefinitionRegistry>()
                .map(|definitions| definitions.objectives(quest).to_vec())
                .unwrap_or_default();
            if let Some(mut objectives) = world.try_resource_mut::<crate::quest_stages::QuestObjectiveState>() {
                if authored.is_empty() {
                    objectives.complete_all(quest);
                } else {
                    objectives.complete_all_authored(quest, authored);
                }
            }
            done(format!("CompleteAllObjectives {:08X}", quest.0))
        }
        None => Action::Declined,
    }
}

// ── destruction, projectiles, triggers, pushes ────────────────────────────────────────────
// GECK *DamageObject <amount>* (Fallout; `0x116c` is another command in Oblivion): damage a
// destructible object; the stage the damage reaches may destroy or disable it.
fn cmd_116c(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, entity)) => match crate::destruction::damage_object(world, entity, form, num(args, 0)) {
            Some(outcome) => {
                if outcome.disable {
                    crate::obscript_actions::run(world, 0x1022, &[], &Context { caller: Some(form), ..*ctx });
                }
                done(format!("DamageObject {form:08X} {}", num(args, 0)))
            }
            None => Action::Declined,
        },
        None => Action::Declined,
    }
}

// GECK *FireWeapon <weapon>*: "fires a projectile from any specified reference".
fn cmd_11e2(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller(world, ctx), form_arg(args, 0)) {
        (Some((source, _)), Some(weapon)) => {
            if let Some(mut pending) = world.try_resource_mut::<crate::PendingProjectileLaunches>() {
                pending.0.push(crate::projectile_requests::ProjectileLaunch { source, weapon, ammo: 0, aim_at: 0, harmless: false });
            }
            done(format!("FireWeapon {weapon:08X} from {source:08X}"))
        }
        _ => Action::Declined,
    }
}

// GECK *EnterTrigger <actor>* (Fallout 3): "forces an OnTriggerEnter event on the target ref".
fn cmd_11ba(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let target = |index: usize| form_arg(args, index).and_then(|form| live(world, form));
    match (caller(world, ctx), target(0)) {
        (Some((_, trigger)), Some(actor)) => {
            if let Some(mut events) = world.query_mut::<crate::events::OnTriggerEnterEvent>() {
                match events.get_mut(trigger) {
                    Some(event) => event.triggerers.push(actor),
                    None => {
                        events.insert(trigger, crate::events::OnTriggerEnterEvent { triggerers: vec![actor] });
                    }
                }
            }
            done("EnterTrigger")
        }
        _ => Action::Declined,
    }
}

// `<pusher>.PushActorAway <actor> <force>`: knocks the actor down and pushes it away from the caller
// ("a negative force pulls it toward").
fn cmd_118a(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let target = |index: usize| form_arg(args, index).and_then(|form| live(world, form));
    match (caller(world, ctx), target(0)) {
        (Some((_, pusher)), Some(actor)) => {
            if let Some(mut requests) = world.query_mut::<PushActorRequest>() {
                requests.insert(actor, PushActorRequest { from: pusher, force: num(args, 1) });
            }
            done("PushActorAway")
        }
        _ => Action::Declined,
    }
}

// GECK *ForceFlee*: "the actor analyzes its surroundings for potential threats, and flees if one is found".
fn force_flee(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => {
            if let Some(mut requests) = world.query_mut::<super::FleeRequest>() {
                requests.insert(actor, super::FleeRequest);
            }
            done("ForceFlee")
        }
        None => Action::Declined,
    }
}

// GECK *ResetInventory*: "all previously added, removed, or calculated objects will be lost".
fn reset_inventory(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, entity)) => {
            if let Some(mut requests) = world.query_mut::<super::ResetInventoryRequest>() {
                requests.insert(entity, super::ResetInventoryRequest);
            }
            done("ResetInventory")
        }
        None => Action::Declined,
    }
}

// ── placement ─────────────────────────────────────────────────────────────────────────────
// GECK *PlaceLeveledActorAtMe <leveled actor> [level mod] [encounter zone]*: the leveled list is
// resolved to an actor when the created reference spawns (as a placed leveled actor is). The level
// modifier and the zone are the spawn's to read; the created-reference record keeps neither.
fn cmd_11d1(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller(world, ctx), form_arg(args, 0)) {
        (Some((at, _)), Some(base)) => {
            let Some(ids) = world.try_resource::<crate::CreatedReferences>().map(|created| created.scripted_ids()) else {
                return Action::Declined;
            };
            let Some(form) = ids.next() else { return Action::Declined };
            if let Some(mut created) = world.try_resource_mut::<crate::CreatedReferences>() {
                created.create_scripted(form, base, at, false);
            }
            Action::Done { value: Some(ObScriptValue::Ref(form)), effect: format!("PlaceLeveledActorAtMe {base:08X} at {at:08X}") }
        }
        _ => Action::Declined,
    }
}

// GECK *AddItemHealthPercent <item> <count> <percent> [hidden]*: adds the item at that condition.
fn cmd_11bc(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller(world, ctx), form_arg(args, 0)) {
        (Some((holder, entity)), Some(item)) => {
            crate::fragment::add_items(world, entity, item, num(args, 1).max(1.0) as u32);
            ledger_mut(world, |l| l.set_weapon_condition(holder, item, num(args, 2) * 100.0));
            done(format!("AddItemHealthPercent {item:08X}"))
        }
        _ => Action::Declined,
    }
}

// ── reputation, ownership, region ─────────────────────────────────────────────────────────
// `SetReputation <repu> <axis> <points>`: axis 1 Fame, 0 Infamy (the numbering `GetReputation` and
// `AddReputation` use); the points are what `GetReputation` reads, clamped to the axis maximum.
fn set_reputation(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match (form_arg(args, 0), player(world)) {
        (Some(repu), Some(player)) => {
            let points = num(args, 2).clamp(0.0, f32::from(u16::MAX)) as u16;
            if crate::obscript_effects::adjust_standing(world, player, repu, num(args, 1) as i32 == 1, points, true).is_none() {
                return Action::Declined;
            }
            done(format!("SetReputation {repu:08X} axis {} {points}", num(args, 1)))
        }
        _ => Action::Declined,
    }
}

// GECK *IsPlayerInRegion <region>*: 1 when the player stands in one of the region's polygons; "always
// returns 0 if the actor is in an interior cell".
fn cmd_1260(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match (form_arg(args, 0), player(world)) {
        (Some(region), Some(player)) => {
            use crate::cell_kind::CellKind;
            let Some(position) = world.get::<byroredux_core::ecs::components::GlobalTransform>(player).map(|t| t.translation) else {
                return Action::Declined;
            };
            let zup = byroredux_core::math::coord::yup_to_zup_pos(position.to_array());
            match world.try_resource::<crate::cell_kind::PlayerCellKind>().and_then(|kind| kind.0) {
                Some(CellKind::Exterior { worldspace }) => {
                    flag(world.try_resource::<crate::obscript_state::RegionAreas>().is_some_and(|areas| areas.contains(region, worldspace, zup[0], zup[1])))
                }
                Some(CellKind::Interior) => flag(false),
                None => Action::Declined,
            }
        }
        _ => Action::Declined,
    }
}

// ── the UI and the game's own flags ───────────────────────────────────────────────────────
// GECK *ShowRecipeMenu <recipe list>*, *OpenTeammateContainer*: a menu for the UI that shows it.
fn open_teammate_container(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    if let Some(mut menus) = world.try_resource_mut::<PendingMenus>() {
        menus.0.push(MenuRequest { menu: if cmd == 0x1250 { "recipes" } else { "teammate container" }, form: form_arg(args, 0).or(ctx.caller).unwrap_or(0) });
    }
    done(format!("{cmd:#06x}"))
}

// GECK *EnableFastTravel <flag>*: "1 = allowed, 0 = disabled until reenabled by this function".
fn cmd_1111(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    store(world, cmd, 0, num(args, 0))
}

// GECK *SetLocationSpecificLoadScreensOnly*, *AutoDisplayObjectives*, *EnableLoadingMenu*,
// *PipBoyRadioOff*, *SetPCYoung*: game-wide flags.
fn set_location_specific_load_screens_only(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    store(world, cmd, 0, num(args, 0))
}

fn cmd_1224(world: &World, _fallout: bool, cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    store(world, cmd, 0, 1.0)
}

// GECK *AgeRace <steps>* / *SexChange <male|female> <flag>*: the player's race age and sex.
fn cmd_11e4(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match player(world) {
        Some(_) => store(world, cmd, 0x14, num(args, 0)),
        None => Action::Declined,
    }
}

// GECK *SetCellImageSpace <cell> <imagespace>* (Fallout 3): "the ImageSpace will not be refreshed until
// the cell is reloaded".
fn cmd_1196(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match (form_arg(args, 0), form_arg(args, 1)) {
        (Some(cell), Some(imagespace)) => {
            ledger_mut(world, |l| l.set_form(cmd, cell, imagespace));
            done(format!("SetCellImageSpace {cell:08X} {imagespace:08X}"))
        }
        _ => Action::Declined,
    }
}

// GECK *SetLastExtDoorActivated <door>* (Fallout 3).
fn cmd_1230(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match form_arg(args, 0) {
        Some(door) => {
            ledger_mut(world, |l| l.set_form(cmd, 0, door));
            done(format!("SetLastExtDoorActivated {door:08X}"))
        }
        None => Action::Declined,
    }
}

// GECK *SetNPCRadio <on> <station>*, *SetBroadcastState <flag>*, *StartRadioConversation [topic]*.
fn cmd_11c4(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => {
            if let Some(station) = form_arg(args, 1).or_else(|| form_arg(args, 0)) {
                ledger_mut(world, |l| l.set_form(cmd, form, station));
            }
            store(world, cmd, form, if cmd == 0x11d6 { 1.0 } else { num(args, 0) })
        }
        None => Action::Declined,
    }
}

// GECK *SetCriticalStage <stage>*: None, GooStart, GooEnd, DisintegrateStart, DisintegrateEnd; the End
// stages "cull its 3D, and remove its havok data from the world" — the reference is disabled.
fn cmd_1212(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => {
            let stage = num(args, 0);
            if matches!(stage as i32, 2 | 4) {
                crate::obscript_actions::run(world, 0x1022, &[], &Context { caller: Some(form), ..*ctx });
            }
            store(world, cmd, form, stage)
        }
        None => Action::Declined,
    }
}

// GECK *IsInCriticalStage <stage>*.
fn cmd_1213(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => flag(ledger(world, |l| l.value(0x1212, form)).flatten().is_some_and(|stage| stage == num(args, 0))),
        None => Action::Declined,
    }
}

// `SetSecuritronExpression <ref> <type> <expression>` (New Vegas): the securitron's face.
fn set_securitron_expression(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx).map(|(form, _)| form).or_else(|| form_arg(args, 0)) {
        Some(subject) => store(world, cmd, subject, num(args, 2).max(num(args, 1))),
        None => Action::Declined,
    }
}

// ── GECK *GetPCMiscStat* / *ModPCMiscStat* `"name"`: the name compiles to its stat code, the
// index into xEdit's `wbMiscStatEnum` (`challenges::MISC_STAT_NAMES`; every vanilla FO3 / FNV
// call checked by `obscript_tools avcodes`). The pre-fix arm took the code for the count, so
// CGTutorial's `GetPCMiscStat "Times Addicted" > 0` read 10 and raised the addiction tutorial
// at every boot. ─
fn get_pc_misc_stat(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match fallout_misc_stat(args) {
        Some(name) => answer(super::misc_stat(world, name) as f32),
        None => Action::Declined,
    }
}

fn cmd_1137(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match fallout_misc_stat(args) {
        Some(name) => {
            super::mod_misc_stat(world, name, num(args, 1) as i32);
            done(format!("ModPCMiscStat {name}"))
        }
        None => Action::Declined,
    }
}

/// The commands this file runs (#6139).
pub(crate) const ACTIONS_COMMANDS: &[Command] = &[
    Command { id: 0x11db, games: Games::Fallout, needs_caller: false, handler: ob_script_quest_locals },
    Command { id: 0x11ad, games: Games::Fallout, needs_caller: false, handler: cmd_11ad },
    Command { id: 0x116c, games: Games::Fallout, needs_caller: false, handler: cmd_116c },
    Command { id: 0x11e2, games: Games::Fallout, needs_caller: false, handler: cmd_11e2 },
    Command { id: 0x11ba, games: Games::Fallout, needs_caller: false, handler: cmd_11ba },
    Command { id: 0x118a, games: Games::Fallout, needs_caller: false, handler: cmd_118a },
    Command { id: 0x116e, games: Games::Oblivion, needs_caller: false, handler: cmd_118a },
    Command { id: 0x10f5, games: Games::Fallout, needs_caller: false, handler: force_flee },
    Command { id: 0x11ed, games: Games::Fallout, needs_caller: false, handler: reset_inventory },
    Command { id: 0x11d1, games: Games::Fallout, needs_caller: false, handler: cmd_11d1 },
    Command { id: 0x11bc, games: Games::Fallout, needs_caller: false, handler: cmd_11bc },
    Command { id: 0x1238, games: Games::Fallout, needs_caller: false, handler: set_reputation },
    Command { id: 0x1260, games: Games::Fallout, needs_caller: false, handler: cmd_1260 },
    Command { id: 0x1250, games: Games::Fallout, needs_caller: false, handler: open_teammate_container },
    Command { id: 0x11c8, games: Games::Fallout, needs_caller: false, handler: open_teammate_container },
    Command { id: 0x1111, games: Games::Both, needs_caller: false, handler: cmd_1111 },
    Command { id: 0x1200, games: Games::Fallout, needs_caller: false, handler: set_location_specific_load_screens_only },
    Command { id: 0x1225, games: Games::Fallout, needs_caller: false, handler: set_location_specific_load_screens_only },
    Command { id: 0x1270, games: Games::Fallout, needs_caller: false, handler: set_location_specific_load_screens_only },
    Command { id: 0x11e6, games: Games::Fallout, needs_caller: false, handler: set_location_specific_load_screens_only },
    Command { id: 0x1224, games: Games::Fallout, needs_caller: false, handler: cmd_1224 },
    Command { id: 0x11e4, games: Games::Fallout, needs_caller: false, handler: cmd_11e4 },
    Command { id: 0x11e7, games: Games::Fallout, needs_caller: false, handler: cmd_11e4 },
    Command { id: 0x1196, games: Games::Fallout, needs_caller: false, handler: cmd_1196 },
    Command { id: 0x1230, games: Games::Fallout, needs_caller: false, handler: cmd_1230 },
    Command { id: 0x11c4, games: Games::Fallout, needs_caller: false, handler: cmd_11c4 },
    Command { id: 0x11d5, games: Games::Fallout, needs_caller: false, handler: cmd_11c4 },
    Command { id: 0x11d6, games: Games::Fallout, needs_caller: false, handler: cmd_11c4 },
    Command { id: 0x1212, games: Games::Fallout, needs_caller: false, handler: cmd_1212 },
    Command { id: 0x1213, games: Games::Fallout, needs_caller: false, handler: cmd_1213 },
    Command { id: 0x1248, games: Games::Fallout, needs_caller: false, handler: set_securitron_expression },
    Command { id: 0x1138, games: Games::Fallout, needs_caller: false, handler: get_pc_misc_stat },
    Command { id: 0x1137, games: Games::Fallout, needs_caller: false, handler: cmd_1137 },
];


/// The Fallout misc stat the first argument's code names (see the `0x1138` arm).
fn fallout_misc_stat(args: &[ObScriptValue]) -> Option<&'static str> {
    let code = num(args, 0);
    (code >= 0.0 && code.fract() == 0.0)
        .then(|| crate::challenges::misc_stat_name(code as u16))
        .flatten()
}
