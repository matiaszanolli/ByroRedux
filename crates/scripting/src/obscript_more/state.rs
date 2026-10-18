//! The ledger-, request- and engine-state commands of [`super`]: everything past the plain reads.

use byroredux_core::ecs::components::{Dead, GlobalTransform};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;

use super::{answer, caller, done, faction_ids, flag, ledger, ledger_mut, player, store, MenuRequest, PendingMenus, PendingSleep};
use crate::obscript_actions::{Action, Context};
use crate::obscript_effects::{form_arg, live, num};
use crate::obscript_ledger::FactionFlag;
use crate::obscript_vm::ObScriptValue;

/// The player's entity or, when a command names a calling reference, that reference's.
fn caller_or_player(world: &World, ctx: &Context) -> Option<(u32, EntityId)> {
    caller(world, ctx).or_else(|| player(world).map(|entity| (0x14, entity)))
}

/// Run a command of the ledger / request group; `Unhandled` for an id that is not one.
pub(super) fn run(world: &World, fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let target = |index: usize| form_arg(args, index).and_then(|form| live(world, form));
    match cmd {
        // ── the player's fame, infamy and bounty (Oblivion) ────────────────────────────────────────
        // CS *GetPCFame* / *GetPCInfamy* / *ModPCFame*: "only the player character has fame or infamy values".
        0x10f9 => answer(ledger(world, |l| l.fame).unwrap_or(0.0)),
        0x10fb => answer(ledger(world, |l| l.infamy).unwrap_or(0.0)),
        0x10f8 => {
            ledger_mut(world, |l| l.fame += num(args, 0));
            done(format!("ModPCFame {}", num(args, 0)))
        }
        // CS *GetCrimeGold* / *SetCrimeGold* / *ModCrimeGold*: the player's bounty.
        0x1074 => answer(ledger(world, |l| l.crime_gold).unwrap_or(0) as f32),
        0x1075 => {
            ledger_mut(world, |l| l.crime_gold = num(args, 0) as i32);
            done(format!("SetCrimeGold {}", num(args, 0)))
        }
        0x1076 => {
            ledger_mut(world, |l| l.crime_gold += num(args, 0) as i32);
            done(format!("ModCrimeGold {}", num(args, 0)))
        }
        // CS *GetAmountSoldStolen*: "the total value of stolen goods that the player has sold" — no sale
        // of stolen goods is tracked, so it is what scripts have put there (0).
        0x10be => answer(ledger(world, |l| l.amount_sold_stolen).unwrap_or(0) as f32),
        // CS *GetCrimeKnown <type> <criminal> <victim>*: "1 if the calling actor knows about the specified
        // crime". The engine records no crime, so no actor knows of one.
        0x1077 => flag(false),
        // CS *Get/SetPCExpelled*, *Get/SetPCFactionMurder / Steal / Attack*.
        0x10c1 | 0x10c2 | 0x10c3 | 0x10c4 | 0x10c5 | 0x10c6 | 0x10c7 if !fallout => {
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
        0x10c5 | 0x10c6 if fallout => {
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
        0x11c9 => match form_arg(args, 0) {
            Some(faction) => {
                ledger_mut(world, |l| l.set_flag(FactionFlag::Enemy, faction, false));
                done(format!("ClearFactionPlayerEnemyFlag {faction:08X}"))
            }
            None => Action::Declined,
        },
        0x11cc => match caller(world, ctx) {
            Some((_, actor)) => {
                let factions = faction_ids(world, actor);
                flag(ledger(world, |l| l.flagged_with_any(FactionFlag::Enemy, &factions)).unwrap_or(false))
            }
            None => Action::Declined,
        },
        // CS / GECK *SetFactionReaction <faction> <target faction> <value>*.
        0x10a8 => match (form_arg(args, 0), form_arg(args, 1)) {
            (Some(faction), Some(other)) if crate::combat::set_faction_reaction(world, faction, other, num(args, 2)) => {
                done(format!("SetFactionReaction {faction:08X} {other:08X}"))
            }
            _ => Action::Declined,
        },
        // GECK *GetFactionRelation <actor>*: 0 Neutral, 1 Enemy, 2 Ally, 3 Friend between the two actors.
        0x11c2 => match (caller_or_player(world, ctx), target(0)) {
            (Some((_, actor)), Some(other)) => match crate::combat::faction_reaction(world, actor, other) {
                Some(reaction) => answer(reaction as f32),
                None => Action::Declined,
            },
            _ => Action::Declined,
        },
        // GECK *SendTrespassAlarm <actor>*: a trespass crime nobody records: the faction-enemy flag is what
        // a crime-tracking faction sets on a major crime, and a trespass is not one. Accepted, no effect.
        0x115c => done("SendTrespassAlarm"),

        // ── disposition (Oblivion): ModDisposition keeps its mod; GetDisposition needs the personality
        // and race terms Oblivion's actor values do not give the engine ────────────────────────────
        0x1053 => match caller(world, ctx) {
            Some((form, _)) => {
                ledger_mut(world, |l| l.mod_disposition(form, num(args, 1).max(num(args, 0)) as i32));
                done(format!("ModDisposition {form:08X}"))
            }
            None => Action::Declined,
        },

        // ── topics, talking, quest objects ─────────────────────────────────────────────────────────
        // CS *RefreshTopicList*: the topic list is built when a conversation opens; nothing is cached.
        0x1145 => done("RefreshTopicList"),
        0x110d => match form_arg(args, 0) {
            Some(object) => {
                let on = num(args, 1) != 0.0;
                ledger_mut(world, |l| l.set_quest_object(object, on));
                done(format!("SetQuestObject {object:08X} {on}"))
            }
            None => Action::Declined,
        },
        // CS *SetShowQuestItems <flag>* (a global).
        0x114e => store(world, cmd, 0, num(args, 0)),

        // ── sleeping, time ─────────────────────────────────────────────────────────────────────────
        // GECK *SetPCSleepHours <hours>*: "manually forces the player to sleep the specified number of hours".
        0x10bd => {
            if let Some(mut sleep) = world.try_resource_mut::<PendingSleep>() {
                sleep.0 += num(args, 0).max(0.0);
            }
            done(format!("SetPCSleepHours {}", num(args, 0)))
        }
        // GECK *IsPCSleeping* / *IsTimePassing* ("sleeping, resting, or traveling"): sleeping hours queued.
        0x10af | 0x1109 => flag(world.try_resource::<PendingSleep>().is_some_and(|sleep| sleep.0 > 0.0)),
        // GECK *WakeUpPC*: "manually closes the wait/sleep menu (as if the player had pressed Cancel)".
        0x10ae => {
            if let Some(mut sleep) = world.try_resource_mut::<PendingSleep>() {
                sleep.0 = 0.0;
            }
            done("WakeUpPC")
        }

        // ── flags and values a Set command keeps for its Get ───────────────────────────────────────
        // GECK *SetForceSneak* / *GetForceSneak*; *SetForceRun* (Oblivion): the actor's forced stance. The
        // engine has no sneak or run stance to apply it to, so it is kept and read back.
        0x10d3 | 0x10d1 => match caller_or_player(world, ctx) {
            Some((form, _)) => store(world, cmd, form, num(args, 0)),
            None => Action::Declined,
        },
        0x10d2 => match caller_or_player(world, ctx) {
            Some((form, _)) => answer(ledger(world, |l| l.value(0x10d3, form)).flatten().unwrap_or(0.0)),
            None => Action::Declined,
        },
        // GECK *SetIgnoreFriendlyHits <flag>*: "so actor will ignore friendly hits".
        0x1151 => match caller(world, ctx) {
            Some((form, _)) => store(world, cmd, form, num(args, 0)),
            None => Action::Declined,
        },
        // GECK *SetCellPublicFlag <cell> <flag>*: "cells which are public cannot have trespassing".
        0x10bb => match form_arg(args, 0) {
            Some(cell) => store(world, cmd, cell, num(args, 1)),
            None => Action::Declined,
        },
        // CS *SetDoorDefaultOpen <flag>*: the door's default open state.
        0x10d8 => match caller(world, ctx) {
            Some((form, _)) => store(world, cmd, form, num(args, 0)),
            None => Action::Declined,
        },
        // GECK *SetActorFullName <message>*, *SetTalkingActivatorActor [actor]*, *SetCombatStyle <style>*,
        // *SetMerchantContainer <container>*: a form kept for the reference.
        0x111c | 0x1171 | 0x10b1 | 0x11b7 => match caller(world, ctx) {
            Some((subject, _)) => {
                let form = form_arg(args, 0).unwrap_or(0);
                ledger_mut(world, |l| l.set_form(cmd, subject, form));
                done(format!("{cmd:#06x} {subject:08X} {form:08X}"))
            }
            None => Action::Declined,
        },
        // GECK *SetItemValue <caps>*: "sets the item's base value"; "the changes made by this function will be
        // saved to save games".
        0x1155 => match caller(world, ctx) {
            Some((_, entity)) => match crate::relationships::actor_base(world, entity).or_else(|| {
                world.get::<crate::scene::SceneAliasCandidate>(entity).map(|c| c.base_form_id).filter(|&b| b != 0)
            }) {
                Some(base) => {
                    ledger_mut(world, |l| l.set_item_value(base, num(args, 0) as i32));
                    done(format!("SetItemValue {base:08X} {}", num(args, 0)))
                }
                None => Action::Declined,
            },
            None => Action::Declined,
        },
        // GECK *SetActorAlpha <0..1>* (SAA): the app's alpha system reads it back from the ledger.
        0x1129 => match caller(world, ctx) {
            Some((form, _)) => store(world, cmd, form, num(args, 0).clamp(0.0, 1.0)),
            None => Action::Declined,
        },
        // GECK *SetActorsAI <0|nonzero>*: "turns off and on the processing of an actor" — the app's AI systems
        // skip an actor whose flag reads 0.
        0x116f | 0x118b => match caller(world, ctx) {
            Some((form, _)) => store(world, 0x118b, form, if num(args, 0) == 0.0 { 0.0 } else { 1.0 }),
            None => Action::Declined,
        },
        // GECK *SetAllReachable / SetAllVisible / SetNoAvoidance / SetSceneIsComplex*: "increase the game's
        // performance ... the flag is cleared every frame". They change cost, not results: kept for the frame.
        0x1159 | 0x115a | 0x115b | 0x115d => store(world, cmd, 0, num(args, 0)),
        // GECK *SetVATSTarget <flag>*: VATS targeting of a destructible object.
        0x11eb => match caller(world, ctx) {
            Some((form, _)) => store(world, cmd, form, num(args, 0)),
            None => Action::Declined,
        },
        // CS *SetPackDuration <minutes>*: the current package's duration — kept; the ambient AI re-evaluates
        // the actor when it elapses.
        0x1122 => match caller(world, ctx) {
            Some((form, actor)) => {
                crate::package::request_package_evaluation(world, actor);
                store(world, cmd, form, num(args, 0))
            }
            None => Action::Declined,
        },
        // Menus and screens with no UI here: the request is queued for the UI that shows them.
        0x1189 | 0x116b => done("PurgeCellBuffers"),
        0x1164 | 0x10db if !fallout || cmd == 0x1164 => {
            if let Some(mut menus) = world.try_resource_mut::<PendingMenus>() {
                menus.0.push(MenuRequest { menu: if cmd == 0x1164 { "dialog subtitles" } else { "birthsign" }, form: 0 });
            }
            done(format!("{cmd:#06x}"))
        }
        // CS *ForceCloseOblivionGate*: the gate quests' state is the scripts' own; recorded for them.
        0x1165 => match caller(world, ctx) {
            Some((form, _)) => store(world, cmd, form, 1.0),
            None => Action::Declined,
        },
        // CS *SetPlayerInSEWorld <flag>* / *GetPlayerInSEWorld* (Oblivion; `0x116c` is DamageObject in
        // Fallout): "a game engine flag indicating whether the player is in the Shivering Isles".
        0x116c if !fallout => store(world, cmd, 0, num(args, 0)),
        0x116d => answer(ledger(world, |l| l.value(0x116c, 0)).flatten().unwrap_or(0.0)),
        // CS *IsActorUsingATorch*: "1 if the calling actor is currently using a torch (i.e. has it out)". The
        // equipment model carries armor and a weapon, not a light, so no actor can have a torch out.
        0x1132 => match caller(world, ctx) {
            Some(_) => flag(false),
            None => Action::Declined,
        },
        // CS *ClearOwnership*: "clears the ownership from the reference it is called on" — Fallout's id for
        // the same command is lowered by the effects table.
        0x1170 => crate::obscript_effects::run(world, 0x118c, args, ctx),
        // CS *ResetFallDamageTimer*: the controller's fall is measured from where it left the ground; the
        // timer is the airborne time, which the character system owns.
        0x1134 => done("ResetFallDamageTimer"),
        // CS *SetRigidBodyMass <0..100>*: "how much it moves when it is bumped ... 100 being no movement".
        0x1156 => match caller(world, ctx) {
            Some((form, _)) => store(world, cmd, form, num(args, 0).clamp(0.0, 100.0)),
            None => Action::Declined,
        },
        // Oblivion path grids: *EnableLinkedPathPoints* / *DisableLinkedPathPoints*: the engine paths on
        // navmeshes, so the flag is kept for the grid that would read it.
        0x112a | 0x112b => match caller(world, ctx) {
            Some((form, _)) => store(world, 0x112a, form, if cmd == 0x112a { 1.0 } else { 0.0 }),
            None => Action::Declined,
        },
        // CS *CreateFullActorCopy*: "creates a copy of the specified actor and its base ... and returns the
        // ID of the new copy". A base record cannot be created at run time, so the copy is a fresh
        // reference of the same base, placed at the original — its inventory and values are the base's, not
        // what the original carried.
        0x1167 => match caller_or_player(world, ctx) {
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
        },
        // CS *GetIsAlerted*: "1 if the actor is in an alerted state" — a drawn weapon (`SetAlert`) or combat.
        0x105b => match caller(world, ctx) {
            Some((_, actor)) => flag(crate::obscript_state::is_weapon_out(world, actor)),
            None => Action::Declined,
        },

        // ── engine state reads ─────────────────────────────────────────────────────────────────────
        // GECK *IsSpellTarget <magic item>*: 1 when an effect of that spell, enchantment or ingestible is on
        // the reference (a carried ability counts: "this will return 1 if Buddy has the specified Actor Ability").
        0x10df => match (caller_or_player(world, ctx), form_arg(args, 0)) {
            (Some((_, actor)), Some(magic)) => flag(
                world.get::<crate::magic_effects::ActiveMagicEffects>(actor).is_some_and(|active| active.0.iter().any(|e| e.source == magic))
                    || world.get::<crate::magic::SpellList>(actor).is_some_and(|list| list.0.contains(&magic)),
            ),
            _ => Action::Declined,
        },
        // GECK *IsOwner [faction|actor]*: 1 when the object is owned by that faction or NPC — or by the player
        // with no argument; "returns 0 if the owner is set to NONE".
        0x1116 => match caller(world, ctx) {
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
        },
        // GECK *IsInList <form list>*: 1 when the reference's base object is in the list.
        0x1174 => match (caller(world, ctx), form_arg(args, 0)) {
            (Some((_, entity)), Some(list)) => {
                let base = world.get::<crate::scene::SceneAliasCandidate>(entity).map(|c| c.base_form_id).unwrap_or(0);
                match crate::form_lists::forms_of(world, list, &[]) {
                    Some(forms) => flag(base != 0 && forms.contains(&base)),
                    None => Action::Declined,
                }
            }
            _ => Action::Declined,
        },
        // GECK *IsWeaponInList <form list>*: 1 when the actor's equipped weapon is in the list.
        0x118f => match (caller_or_player(world, ctx), form_arg(args, 0)) {
            (Some((_, actor)), Some(list)) => {
                let weapon = world.get::<byroredux_core::ecs::components::EquippedWeapon>(actor).map(|w| w.base_form_id);
                match (weapon, crate::form_lists::forms_of(world, list, &[])) {
                    (Some(weapon), Some(forms)) => flag(forms.contains(&weapon)),
                    (None, Some(_)) => flag(false),
                    _ => Action::Declined,
                }
            }
            _ => Action::Declined,
        },
        // CS / GECK *IsActor*, *IsEssential*.
        0x1161 => match caller(world, ctx) {
            Some((_, entity)) => flag(world.has::<byroredux_core::ecs::components::ActorVitals>(entity)),
            None => Action::Declined,
        },
        0x1162 => match caller_or_player(world, ctx) {
            Some((_, actor)) => flag(world.get::<crate::ActorProtection>(actor).is_some_and(|p| p.essential)),
            None => Action::Declined,
        },
        // GECK *GetAnimAction*: -1 none, 2 attack ... — only what the engine's state shows: attacking while in
        // combat is ATTACK (2), else none.
        0x10db if fallout => match caller(world, ctx) {
            Some((_, actor)) => answer(if crate::is_in_combat(world, actor) && world.has::<byroredux_core::ecs::components::EquippedWeapon>(actor) { 2.0 } else { -1.0 }),
            None => Action::Declined,
        },
        // GECK *IsPlayerMovingIntoNewSpace*: 1 while the player loads into a new cell — no cell is resident
        // for the frames the transition takes.
        0x1166 => flag(world.try_resource::<crate::cell_kind::PlayerCellKind>().is_some_and(|kind| kind.0.is_none())),
        // GECK *IsPC1stPerson*: the camera mode, through the query the app installs.
        0x1188 => match super::engine_queries(world).first_person {
            Some(first) => flag(first(world)),
            None => Action::Declined,
        },
        // GECK *IsPS3* / *IsGoreDisabled*: the engine is neither a PS3 build nor has a gore-off setting.
        0x120b | 0x1226 => flag(false),
        // GECK *HasLoaded3D*: the reference has a placement in the loaded world.
        0x122e => match caller(world, ctx) {
            Some((_, entity)) => flag(world.has::<GlobalTransform>(entity)),
            None => Action::Declined,
        },
        // GECK *IsPlayerGrabbedRef <ref>*: the engine has no grab; nothing is grabbed.
        0x11d0 => flag(false),
        // GECK *IsRidingHorse* (Oblivion): the engine has no mounts.
        0x1147 => flag(false),
        // GECK *IsLimbGone <limb>*: "tests if an actor has had a particular limb dismembered" — the engine
        // keeps no dismemberment, so none is gone. *GetKillingBlowLimb*: -1 for a living actor; a corpse's
        // killing blow location is not kept.
        0x118e => flag(false),
        0x11f0 => match caller(world, ctx) {
            Some((_, actor)) if !world.has::<Dead>(actor) => answer(-1.0),
            _ => Action::Declined,
        },
        // GECK *IsCombatTarget <actor>*: 1 when the calling actor is fighting that actor.
        0x1203 => match (caller(world, ctx), target(0)) {
            (Some((_, actor)), Some(other)) => flag(world.get::<crate::AiCombatState>(actor).is_some_and(|state| state.target == other)),
            _ => Action::Declined,
        },
        // GECK *GetPermanentActorValue <av>*: base plus the permanent layers, no situational bonus.
        0x11ef => match (caller_or_player(world, ctx), crate::obscript_actor_values::actor_value_form(world, num(args, 0))) {
            (Some((_, actor)), Some(avif)) => match world.get::<byroredux_core::ecs::components::ActorValues>(actor).and_then(|v| v.get(avif).copied()) {
                Some(value) => answer(value.base + value.set_override + value.permanent_mod),
                None => Action::Declined,
            },
            _ => Action::Declined,
        },
        // GECK *GetWeaponHealthPerc* / *ModWeaponHealthPerc <delta>*: the condition of the equipped weapon,
        // a percent that scripts can change (nothing else wears a weapon).
        0x11f4 | 0x11f6 => match caller_or_player(world, ctx) {
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
        },
        // GECK *GetContainerInventoryCount*: the number of items in the container.
        0x127a => match caller(world, ctx) {
            Some((_, entity)) => match world.get::<byroredux_core::ecs::components::Inventory>(entity) {
                Some(inventory) => answer(inventory.items.iter().map(|stack| stack.count as f32).sum()),
                None => answer(0.0),
            },
            None => Action::Declined,
        },
        // GECK *GetMapMarkerVisible*: 0 undiscovered, 1 visible, 2 visible and travelable.
        0x11ec => match caller(world, ctx) {
            Some((form, _)) => answer(if crate::map_marker_fast_travel(world, form) {
                2.0
            } else if crate::map_marker_visible(world, form) {
                1.0
            } else {
                0.0
            }),
            None => Action::Declined,
        },
        // GECK *IsActorDetected* (Oblivion): 1 when any other actor detects the calling actor; 0 for the player.
        0x10b5 => match (caller(world, ctx), world.try_resource::<crate::SightQuery>().map(|q| q.0)) {
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
        },
        _ => super::actions::run(world, fallout, cmd, args, ctx),
    }
}
