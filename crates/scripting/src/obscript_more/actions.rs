//! The action commands of [`super`] that reach other runtimes: quests, destruction, projectiles,
//! triggers, the knockdown and flee requests, placement, menus and the flags the UI reads.

use byroredux_core::ecs::World;

use super::{answer, caller, done, flag, ledger, ledger_mut, player, store, MenuRequest, PendingMenus, PushActorRequest};
use crate::obscript_actions::{Action, Context};
use crate::obscript_effects::{form_arg, live, num};
use crate::obscript_vm::ObScriptValue;

pub(super) fn run(world: &World, fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let target = |index: usize| form_arg(args, index).and_then(|form| live(world, form));
    match cmd {
        // ── quests ────────────────────────────────────────────────────────────────────────────────
        // GECK *ResetQuest <quest>*: "clears the quest stage list, the objective list, all the quest
        // variables; resets the quest stage to its default state". (The quest's own locals are the
        // `ObScriptQuestLocals` the quest tick keeps.)
        0x11db => match form_arg(args, 0) {
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
        },
        // GECK *CompleteAllObjectives <quest>*: "marks all objectives for a quest as completed".
        0x11ad => match form_arg(args, 0) {
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
        },

        // ── destruction, projectiles, triggers, pushes ────────────────────────────────────────────
        // GECK *DamageObject <amount>* (Fallout; `0x116c` is another command in Oblivion): damage a
        // destructible object; the stage the damage reaches may destroy or disable it.
        0x116c if fallout => match caller(world, ctx) {
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
        },
        // GECK *FireWeapon <weapon>*: "fires a projectile from any specified reference".
        0x11e2 => match (caller(world, ctx), form_arg(args, 0)) {
            (Some((source, _)), Some(weapon)) => {
                if let Some(mut pending) = world.try_resource_mut::<crate::PendingProjectileLaunches>() {
                    pending.0.push(crate::projectile_requests::ProjectileLaunch { source, weapon, ammo: 0, aim_at: 0 });
                }
                done(format!("FireWeapon {weapon:08X} from {source:08X}"))
            }
            _ => Action::Declined,
        },
        // GECK *EnterTrigger <actor>* (Fallout 3): "forces an OnTriggerEnter event on the target ref".
        0x11ba => match (caller(world, ctx), target(0)) {
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
        },
        // `<pusher>.PushActorAway <actor> <force>`: knocks the actor down and pushes it away from the caller
        // ("a negative force pulls it toward").
        0x118a | 0x116e => match (caller(world, ctx), target(0)) {
            (Some((_, pusher)), Some(actor)) => {
                if let Some(mut requests) = world.query_mut::<PushActorRequest>() {
                    requests.insert(actor, PushActorRequest { from: pusher, force: num(args, 1) });
                }
                done("PushActorAway")
            }
            _ => Action::Declined,
        },
        // GECK *ForceFlee*: "the actor analyzes its surroundings for potential threats, and flees if one is found".
        0x10f5 => match caller(world, ctx) {
            Some((_, actor)) => {
                if let Some(mut requests) = world.query_mut::<super::FleeRequest>() {
                    requests.insert(actor, super::FleeRequest);
                }
                done("ForceFlee")
            }
            None => Action::Declined,
        },
        // GECK *ResetInventory*: "all previously added, removed, or calculated objects will be lost".
        0x11ed => match caller(world, ctx) {
            Some((_, entity)) => {
                if let Some(mut requests) = world.query_mut::<super::ResetInventoryRequest>() {
                    requests.insert(entity, super::ResetInventoryRequest);
                }
                done("ResetInventory")
            }
            None => Action::Declined,
        },

        // ── placement ─────────────────────────────────────────────────────────────────────────────
        // GECK *PlaceLeveledActorAtMe <leveled actor> [level mod] [encounter zone]*: the leveled list is
        // resolved to an actor when the created reference spawns (as a placed leveled actor is). The level
        // modifier and the zone are the spawn's to read; the created-reference record keeps neither.
        0x11d1 => match (caller(world, ctx), form_arg(args, 0)) {
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
        },
        // GECK *AddItemHealthPercent <item> <count> <percent> [hidden]*: adds the item at that condition.
        0x11bc => match (caller(world, ctx), form_arg(args, 0)) {
            (Some((holder, entity)), Some(item)) => {
                crate::fragment::add_items(world, entity, item, num(args, 1).max(1.0) as u32);
                ledger_mut(world, |l| l.set_weapon_condition(holder, item, num(args, 2) * 100.0));
                done(format!("AddItemHealthPercent {item:08X}"))
            }
            _ => Action::Declined,
        },

        // ── reputation, ownership, region ─────────────────────────────────────────────────────────
        // `SetReputation <repu> <axis> <points>`: axis 1 Fame, 0 Infamy (the numbering `GetReputation` and
        // `AddReputation` use); the points are what `GetReputation` reads, clamped to the axis maximum.
        0x1238 => match (form_arg(args, 0), player(world)) {
            (Some(repu), Some(player)) => {
                let points = num(args, 2).clamp(0.0, f32::from(u16::MAX)) as u16;
                if crate::obscript_effects::adjust_standing(world, player, repu, num(args, 1) as i32 == 1, points, true).is_none() {
                    return Action::Declined;
                }
                done(format!("SetReputation {repu:08X} axis {} {points}", num(args, 1)))
            }
            _ => Action::Declined,
        },
        // GECK *IsPlayerInRegion <region>*: 1 when the player stands in one of the region's polygons; "always
        // returns 0 if the actor is in an interior cell".
        0x1260 => match (form_arg(args, 0), player(world)) {
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
        },

        // ── the UI and the game's own flags ───────────────────────────────────────────────────────
        // GECK *ShowRecipeMenu <recipe list>*, *OpenTeammateContainer*: a menu for the UI that shows it.
        0x1250 | 0x11c8 => {
            if let Some(mut menus) = world.try_resource_mut::<PendingMenus>() {
                menus.0.push(MenuRequest { menu: if cmd == 0x1250 { "recipes" } else { "teammate container" }, form: form_arg(args, 0).or(ctx.caller).unwrap_or(0) });
            }
            done(format!("{cmd:#06x}"))
        }
        // GECK *EnableFastTravel <flag>*: "1 = allowed, 0 = disabled until reenabled by this function".
        0x1111 => store(world, cmd, 0, num(args, 0)),
        // GECK *SetLocationSpecificLoadScreensOnly*, *AutoDisplayObjectives*, *EnableLoadingMenu*,
        // *PipBoyRadioOff*, *SetPCYoung*: game-wide flags.
        0x1200 | 0x1225 | 0x1270 | 0x11e6 => store(world, cmd, 0, num(args, 0)),
        0x1224 => store(world, cmd, 0, 1.0),
        // GECK *AgeRace <steps>* / *SexChange <male|female> <flag>*: the player's race age and sex.
        0x11e4 | 0x11e7 => match player(world) {
            Some(_) => store(world, cmd, 0x14, num(args, 0)),
            None => Action::Declined,
        },
        // GECK *SetCellImageSpace <cell> <imagespace>* (Fallout 3): "the ImageSpace will not be refreshed until
        // the cell is reloaded".
        0x1196 => match (form_arg(args, 0), form_arg(args, 1)) {
            (Some(cell), Some(imagespace)) => {
                ledger_mut(world, |l| l.set_form(cmd, cell, imagespace));
                done(format!("SetCellImageSpace {cell:08X} {imagespace:08X}"))
            }
            _ => Action::Declined,
        },
        // GECK *SetLastExtDoorActivated <door>* (Fallout 3).
        0x1230 => match form_arg(args, 0) {
            Some(door) => {
                ledger_mut(world, |l| l.set_form(cmd, 0, door));
                done(format!("SetLastExtDoorActivated {door:08X}"))
            }
            None => Action::Declined,
        },
        // GECK *SetNPCRadio <on> <station>*, *SetBroadcastState <flag>*, *StartRadioConversation [topic]*.
        0x11c4 | 0x11d5 | 0x11d6 => match caller(world, ctx) {
            Some((form, _)) => {
                if let Some(station) = form_arg(args, 1).or_else(|| form_arg(args, 0)) {
                    ledger_mut(world, |l| l.set_form(cmd, form, station));
                }
                store(world, cmd, form, if cmd == 0x11d6 { 1.0 } else { num(args, 0) })
            }
            None => Action::Declined,
        },
        // GECK *SetCriticalStage <stage>*: None, GooStart, GooEnd, DisintegrateStart, DisintegrateEnd; the End
        // stages "cull its 3D, and remove its havok data from the world" — the reference is disabled.
        0x1212 => match caller(world, ctx) {
            Some((form, _)) => {
                let stage = num(args, 0);
                if matches!(stage as i32, 2 | 4) {
                    crate::obscript_actions::run(world, 0x1022, &[], &Context { caller: Some(form), ..*ctx });
                }
                store(world, cmd, form, stage)
            }
            None => Action::Declined,
        },
        // GECK *IsInCriticalStage <stage>*.
        0x1213 => match caller(world, ctx) {
            Some((form, _)) => flag(ledger(world, |l| l.value(0x1212, form)).flatten().is_some_and(|stage| stage == num(args, 0))),
            None => Action::Declined,
        },
        // `SetSecuritronExpression <ref> <type> <expression>` (New Vegas): the securitron's face.
        0x1248 => match caller(world, ctx).map(|(form, _)| form).or_else(|| form_arg(args, 0)) {
            Some(subject) => store(world, cmd, subject, num(args, 2).max(num(args, 1))),
            None => Action::Declined,
        },

        // ── `Get/ModPCMiscStat`: the VM resolved the stat name and handed the value (see `text_argument`) ─
        0x1138 if fallout => match args.first() {
            Some(ObScriptValue::Num(count)) => answer(*count),
            _ => Action::Declined,
        },
        0x1137 if fallout => done("ModPCMiscStat"),
        _ => super::extras::run(world, fallout, cmd, args, ctx),
    }
}
