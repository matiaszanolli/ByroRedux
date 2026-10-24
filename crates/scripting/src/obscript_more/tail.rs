//! The long tail of the command surface of [`super`]: the commands a vanilla script uses once or twice —
//! the character-creation screens, the math functions, the platform questions, the faction and cell
//! setters. Each answers from the page that documents it (GECK / CS) or from the engine state it names.

use byroredux_core::ecs::components::{ActorValues, FactionRanks, Inventory};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;

use super::state::caller_or_player;
use super::{answer, caller, done, flag, ledger, ledger_mut, player, store, MenuRequest, PendingMenus};
use crate::obscript_actions::{Action, Context};
use crate::obscript_cmd::{Command, Games};
use crate::obscript_effects::{form_arg, live, num};
use crate::obscript_vm::ObScriptValue;

/// A video a script asked for (`PlayBink`), for a player the engine does not have. `NOT_SAVED_BY_DESIGN`:
/// drained by the app the frame it is raised.
#[derive(Debug, Default)]
pub struct PendingVideos(pub Vec<String>);

impl byroredux_core::ecs::Resource for PendingVideos {}

/// `ExitGame`: the game is to end (credits, back to the main menu). `NOT_SAVED_BY_DESIGN`: consumed the
/// frame it is raised.
#[derive(Debug, Default)]
pub struct ExitGameRequest(pub bool);

impl byroredux_core::ecs::Resource for ExitGameRequest {}

/// The encounter zone of each cell, by cell FormID (`GetInZone`). `NOT_SAVED_BY_DESIGN`: rebuilt from the
/// records at every content load.
#[derive(Debug, Default, Clone)]
pub struct CellZones(std::sync::Arc<std::collections::HashMap<u32, u32>>);

impl byroredux_core::ecs::Resource for CellZones {}

impl CellZones {
    pub fn from_index(index: &byroredux_plugin::esm::records::EsmIndex) -> Self {
        let mut zones = std::collections::HashMap::new();
        let cells = index.cells.cells.values().chain(index.cells.exterior_cells.values().flat_map(|grid| grid.values()));
        for cell in cells {
            if let Some(zone) = cell.encounter_zone_form {
                zones.insert(cell.form_id, zone);
            }
        }
        Self(std::sync::Arc::new(zones))
    }
}

/// The races flagged *Child* (`IsChild`; RACE `DATA` flag `0x04`), by race FormID. `NOT_SAVED_BY_DESIGN`:
/// rebuilt from the records at every content load.
#[derive(Debug, Default, Clone)]
pub struct ChildRaces(std::sync::Arc<std::collections::HashSet<u32>>);

impl byroredux_core::ecs::Resource for ChildRaces {}

impl ChildRaces {
    pub fn from_index(index: &byroredux_plugin::esm::records::EsmIndex) -> Self {
        Self(races_flagged(index, 0x04))
    }
}

/// The races flagged *Playable* (`GetIsPlayableRace`; RACE `DATA` flag `0x01` — xEdit's TES4 `Playable`, bit 0
/// of the flags word on every later game), by race FormID. `NOT_SAVED_BY_DESIGN`: rebuilt from the records at
/// every content load.
#[derive(Debug, Default, Clone)]
pub struct PlayableRaces(std::sync::Arc<std::collections::HashSet<u32>>);

impl byroredux_core::ecs::Resource for PlayableRaces {}

impl PlayableRaces {
    pub fn from_index(index: &byroredux_plugin::esm::records::EsmIndex) -> Self {
        Self(races_flagged(index, 0x01))
    }

    /// Whether `race` is a playable race of the loaded records.
    pub fn contains(&self, race: u32) -> bool {
        self.0.contains(&race)
    }
}

/// The FormIDs of the races whose RACE `DATA` flags carry any bit of `flag`.
fn races_flagged(
    index: &byroredux_plugin::esm::records::EsmIndex,
    flag: u32,
) -> std::sync::Arc<std::collections::HashSet<u32>> {
    std::sync::Arc::new(index.races.iter().filter(|(_, race)| race.race_flags & flag != 0).map(|(&form, _)| form).collect())
}

/// The DLC master files, in the order a New Vegas script counts them (`IsDLCInstalled`): Dead Money,
/// Honest Hearts, Old World Blues, Lonesome Road, Gun Runners' Arsenal, Courier's Stash.
const DLC_MASTERS: [&str; 6] = ["DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm", "GunRunnersArsenal.esm", "ClassicPack.esm"];

fn menu(world: &World, name: &'static str, form: u32) -> Action {
    if let Some(mut menus) = world.try_resource_mut::<PendingMenus>() {
        menus.0.push(MenuRequest { menu: name, form });
    }
    done(format!("{name} menu requested"))
}

/// The player's actor-value form for skill code `skill`.
fn skill_form(world: &World, skill: f32) -> Option<u32> {
    crate::obscript_actor_values::actor_value_form(world, skill)
}

/// Apply or take back the starting bonus a tagged skill carries (`fAVDTagSkillBonus`).
fn tag_bonus(world: &World, player: EntityId, skill: u8, sign: f32) {
    let bonus = crate::obscript_state::game_setting(world, "fAVDTagSkillBonus").unwrap_or(0.0);
    let (Some(form), true) = (skill_form(world, f32::from(skill)), bonus != 0.0) else { return };
    if let Some(mut values) = world.query_mut::<ActorValues>() {
        if let Some(values) = values.get_mut(player) {
            values.mod_permanent(form, sign * bonus);
        }
    }
}

// ── the platform ───────────────────────────────────────────────────────────────────────────
// GECK *IsXbox*: 1 on a console; *IsWin32*: "1 if the game is running on a PC" — this engine is a PC.
fn is_xbox(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    flag(false)
}

fn cmd_120c(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    flag(true)
}

// GECK *IsDLCInstalled <index>*: whether the DLC's master is in the load order.
fn cmd_126f(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    let wanted = DLC_MASTERS.get(num(args, 0) as usize).copied();
    match (wanted, world.try_resource::<crate::LoadOrderIdentity>()) {
        (Some(master), Some(identity)) => {
            let plugin = byroredux_core::form_id::PluginId::from_filename(master);
            flag(identity.owners().iter().any(|(_, owner)| *owner == plugin))
        }
        _ => Action::Declined,
    }
}

// GECK *SetRumble <left> <right> <seconds>*: the motors of an Xbox 360 / PS3 pad. No pad is read, so
// the request is kept for one that is.
fn cmd_11fb(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    store(world, cmd, 0, num(args, 0).max(num(args, 1)))
}

// GECK *ExitGame*: "plays credits and returns the game to the main menu". The engine has no main
// menu to return to: the game ends.
fn exit_game(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    if let Some(mut exit) = world.try_resource_mut::<ExitGameRequest>() {
        exit.0 = true;
    }
    menu(world, "credits", 0)
}

// GECK *PlayBink "<file>" [interruptable mute pause letterbox]*: a Bink video from `data\video`; there
// is no Bink decoder, so the request is recorded.
fn cmd_1114(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    let texts = || world.try_resource_mut::<super::TextArguments>().map(|mut texts| std::mem::take(&mut texts.0)).unwrap_or_default();
    let file = texts().into_iter().next().unwrap_or_default();
    if let Some(mut videos) = world.try_resource_mut::<PendingVideos>() {
        videos.0.push(file.clone());
    }
    done(format!("PlayBink {file}"))
}

// GECK *PipBoyRadio <0|1|Tune> [station]*: the radio's state, kept: `Tune` names the station.
fn tune(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    store(world, cmd, 0, num(args, 0));
    if let Some(station) = form_arg(args, 1).or_else(|| form_arg(args, 0)) {
        ledger_mut(world, |l| l.set_form(cmd, 0, station));
    }
    done("PipBoyRadio")
}

// GECK *ResetPipboyManager*: re-initialises the Pip-Boy's menus and appearance; the engine's holds
// nothing a script could have left stale.
fn reset_pipboy_manager(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    done("ResetPipboyManager")
}

// GECK *SetHardcore <flag>* (Hardcore mode).
fn cmd_124b(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    store(world, cmd, 0, num(args, 0))
}

// GECK *DetonatePlacedExplosives*: the player's placed mines go off. None is ever placed.
fn detonate_placed_explosives(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    done("DetonatePlacedExplosives")
}

// GECK *PreloadMagicEffect*: queues an effect's particles; nothing waits to be loaded.
fn preload_magic_effect(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    done("PreloadMagicEffect")
}

// GECK *SetGlobalTimeMultiplier* (`SGTM`): "1.0 is standard time, 0.5 is slow mo".
fn set_global_time_multiplier(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    store(world, cmd, 0, num(args, 0))
}

// ── math (GECK, `Sqrt` / `Abs` / `Tan`) ────────────────────────────────────────────────────
fn sqrt(_world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    answer(num(args, 0).max(0.0).sqrt())
}

fn cmd_1221(_world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    answer(num(args, 0).abs())
}

fn cmd_121e(_world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    answer(num(args, 0).tan())
}

// ── the character-creation screens: requests for the UI that shows them ────────────────────
fn cmd_10d9(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    menu(world, "class", 0)
}

fn cmd_10ea(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    menu(world, "spell making", 0)
}

fn cmd_10eb(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    menu(world, "enchanting", 0)
}

fn cmd_1268(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    menu(world, "traits", 0)
}

fn cmd_11e1(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    menu(world, "name entry", 0)
}

// `SetSPECIALPoints <n>` / `SetTagSkills <n>` / `ShowLoveTesterMenuParams <n>` / `SSBMP <n>`: the
// SPECIAL, tagged-skill and Vigor Tester screens with that many points to give.
fn set_special_points(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    menu(world, "special points", num(args, 0) as u32)
}

fn cmd_121a(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    menu(world, "tag skills", num(args, 0) as u32)
}

fn cmd_1245(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    menu(world, "vigor tester", num(args, 0) as u32)
}

fn cmd_11ee(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    menu(world, "special book", num(args, 0) as u32)
}

// CS *ShowRaceMenu*: with no race screen the call accepts the player's current appearance, as
// `Game.ShowRaceMenu` does.
fn show_race_menu(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    if let Some(mut presentation) = world.try_resource_mut::<crate::cinematic::CinematicPresentationState>() {
        presentation.show_race_menu();
    }
    menu(world, "race", 0)
}

// GECK *SetInChargen <flag>*: "while in this state the player will not level up (XP accumulates)".
fn cmd_10e7(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    store(world, cmd, 0, num(args, 0))
}

// `ResetXP`: the experience toward the next level goes back to zero.
fn reset_xp(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    match player(world) {
        Some(player) => {
            if let Some(mut levels) = world.query_mut::<byroredux_core::character::CharacterLevel>() {
                if let Some(level) = levels.get_mut(player) {
                    level.xp = 0;
                }
            }
            done("ResetXP")
        }
        None => Action::Declined,
    }
}

// GECK *SetPlayerTagSkill <skill> <slot 0-2>*: "gives the player a starting bonus in that skill"
// (`fAVDTagSkillBonus`); the skill it replaces loses its bonus. *IsPlayerTagSkill <skill>*.
fn cmd_11cd(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match (player(world), num(args, 1) as i32) {
        (Some(player), slot @ 0..=2) => {
            let skill = num(args, 0) as u8;
            if let Some(Some(previous)) = ledger_mut(world, |l| l.set_tag_skill(slot as u8, skill)) {
                tag_bonus(world, player, previous, -1.0);
            }
            tag_bonus(world, player, skill, 1.0);
            done(format!("SetPlayerTagSkill {skill} {slot}"))
        }
        _ => Action::Declined,
    }
}

fn cmd_11ce(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    flag(ledger(world, |l| l.is_tag_skill(num(args, 0) as u8)).unwrap_or(false))
}

// ── actors ─────────────────────────────────────────────────────────────────────────────────
// GECK *GetKnockedState*: 0 normal, 1 knocked out ("an unconscious essential actor, a paralyzed actor,
// negative-fatigue knockout"), 2 knocked down — and "0 if called on actors knocked down by
// PushActorAway", which is the only knockdown the engine has.
fn get_knocked_state(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => {
            let paralysed = crate::obscript_actor_values::actor_value_form(world, 47.0)
                .and_then(|form| world.get::<ActorValues>(actor).map(|values| values.current(form) > 0.0))
                .unwrap_or(false);
            answer(if crate::is_unconscious(world, actor) || paralysed { 1.0 } else { 0.0 })
        }
        None => Action::Declined,
    }
}

// GECK *GetHitLocation*: "None -1": the combat model records no hit location.
fn get_hit_location(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some(_) => answer(-1.0),
        None => Action::Declined,
    }
}

// GECK *IsChild*: the actor's race has the Child flag.
fn is_child(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => {
            let race = world.get::<byroredux_core::character::Background>(actor).map(|background| background.race_form_id);
            flag(race.is_some_and(|race| world.try_resource::<ChildRaces>().is_some_and(|races| races.0.contains(&race))))
        }
        None => Action::Declined,
    }
}

// GECK *GetFriendHit <actor>*: "the number of friendly hits made by the target actor on the calling
// actor".
fn cmd_1120(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller(world, ctx), form_arg(args, 0)) {
        (Some((victim, _)), Some(attacker)) => answer(ledger(world, |l| l.friend_hits(victim, attacker)).unwrap_or(0) as f32),
        _ => Action::Declined,
    }
}

// GECK *SetLevel <level> <relative to player> [min] [max]* on an actor (the player's level changes,
// "but not any of his stats").
fn cmd_1133(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller_or_player(world, ctx) {
        Some((_, actor)) => {
            let player_level = player(world).and_then(|player| world.get::<byroredux_core::character::CharacterLevel>(player).map(|level| i32::from(level.level)));
            let asked = num(args, 0) as i32;
            let mut level = if num(args, 1) != 0.0 { player_level.unwrap_or(1) + asked } else { asked };
            if args.len() > 2 {
                level = level.max(num(args, 2) as i32);
            }
            if args.len() > 3 && num(args, 3) > 0.0 {
                level = level.min(num(args, 3) as i32);
            }
            let level = level.clamp(1, i32::from(u16::MAX)) as u16;
            if let Some(mut levels) = world.query_mut::<byroredux_core::character::CharacterLevel>() {
                match levels.get_mut(actor) {
                    Some(current) => current.level = level,
                    None => {
                        levels.insert(actor, byroredux_core::character::CharacterLevel { level, xp: 0 });
                    }
                }
            }
            done(format!("SetLevel {level}"))
        }
        None => Action::Declined,
    }
}

// GECK *DispelAllSpells*: "removes all current spells/magic that are effecting an actor".
fn dispel_all_spells(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => {
            let ended = crate::magic_effects::dispel(world, actor, None);
            done(format!("DispelAllSpells ended {ended}"))
        }
        None => Action::Declined,
    }
}

// GECK *SetDisposition <actor> <value>*: kept for the reference.
fn cmd_1184(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller(world, ctx), form_arg(args, 0)) {
        (Some((subject, _)), Some(other)) => {
            ledger_mut(world, |l| l.set_form(cmd, subject, other));
            store(world, cmd, subject, num(args, 1))
        }
        _ => Action::Declined,
    }
}

// GECK *RemoveFromAllFactions*.
fn remove_from_all_factions(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => {
            if let Some(mut ranks) = world.query_mut::<FactionRanks>() {
                if let Some(factions) = ranks.get_mut(actor) {
                    factions.0.clear();
                }
            }
            done("RemoveFromAllFactions")
        }
        None => Action::Declined,
    }
}

// GECK *SetWeaponHealthPerc <value>*: "sets the base condition of an actor's currently equipped
// weapon" — in New Vegas "hardcoded not to increase condition beyond 99.83%".
fn cmd_11f5(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller_or_player(world, ctx) {
        Some((form, actor)) => match world.get::<byroredux_core::ecs::components::EquippedWeapon>(actor).map(|w| w.base_form_id) {
            Some(weapon) => {
                ledger_mut(world, |l| l.set_weapon_condition(form, weapon, num(args, 0).min(99.83)));
                done(format!("SetWeaponHealthPerc {}", num(args, 0)))
            }
            None => Action::Declined,
        },
        None => Action::Declined,
    }
}

// GECK *MatchRace <actor>*: the calling actor takes the other's race.
fn cmd_11e5(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller(world, ctx), form_arg(args, 0).and_then(|form| live(world, form))) {
        (Some((_, actor)), Some(other)) => {
            let race = world.get::<byroredux_core::character::Background>(other).map(|background| background.race_form_id);
            match race {
                Some(race) => {
                    if let Some(mut backgrounds) = world.query_mut::<byroredux_core::character::Background>() {
                        if let Some(background) = backgrounds.get_mut(actor) {
                            background.race_form_id = race;
                        }
                    }
                    done(format!("MatchRace {race:08X}"))
                }
                None => Action::Declined,
            }
        }
        _ => Action::Declined,
    }
}

// GECK *MatchFaceGeometry <actor> <percent>*: the face morph, kept; no runtime morph reads it.
fn cmd_11df(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller(world, ctx), form_arg(args, 0)) {
        (Some((subject, _)), Some(other)) => {
            ledger_mut(world, |l| l.set_form(cmd, subject, other));
            store(world, cmd, subject, num(args, 1))
        }
        _ => Action::Declined,
    }
}

// CS *SetClass <class>*.
fn cmd_10f0(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller_or_player(world, ctx), form_arg(args, 0)) {
        (Some((_, actor)), Some(class)) => {
            if let Some(mut backgrounds) = world.query_mut::<byroredux_core::character::Background>() {
                if let Some(background) = backgrounds.get_mut(actor) {
                    background.class_form_id = class;
                }
            }
            done(format!("SetClass {class:08X}"))
        }
        _ => Action::Declined,
    }
}

// GECK *GetIsLockBroken*: the engine has no way to break a lock, so none is.
fn get_is_lock_broken(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some(_) => flag(false),
        None => Action::Declined,
    }
}

// GECK *SetMinimalUse <flag>* on a door; *SetNoActivationSound <sound>*; *SetNoRumors*: kept.
fn set_no_rumors(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((subject, _)) => {
            if let Some(form) = form_arg(args, 0) {
                ledger_mut(world, |l| l.set_form(cmd, subject, form));
            }
            store(world, cmd, subject, num(args, 0))
        }
        None => Action::Declined,
    }
}

// GECK *ClearDestruction*: "resets everything related to destruction on a specified reference".
fn clear_destruction(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => {
            crate::destruction::clear_destruction(world, form);
            done(format!("ClearDestruction {form:08X}"))
        }
        None => Action::Declined,
    }
}

// GECK *AddCardToPlayer <card>*: the Caravan card joins the player's inventory.
fn cmd_125b(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match (player(world), form_arg(args, 0)) {
        (Some(player), Some(card)) => {
            crate::fragment::add_items(world, player, card, 1);
            done(format!("AddCardToPlayer {card:08X}"))
        }
        _ => Action::Declined,
    }
}

// ── the player's standing ──────────────────────────────────────────────────────────────────
// GECK *AddReputationExact <repu> <fame 1 | infamy 0> <amount>*: "an exact amount of reputation".
fn cmd_123b(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match (form_arg(args, 0), player(world)) {
        (Some(repu), Some(player)) => {
            let points = num(args, 2).clamp(0.0, f32::from(u16::MAX)) as u16;
            if crate::obscript_effects::adjust_standing(world, player, repu, num(args, 1) as i32 == 1, points, false).is_none() {
                return Action::Declined;
            }
            done(format!("AddReputationExact {repu:08X} +{points}"))
        }
        _ => Action::Declined,
    }
}

// CS *SetPCFame <value>* / *SetPCInfamy <value>*.
fn cmd_10fc(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    ledger_mut(world, |l| if cmd == 0x10fc { l.fame = num(args, 0) } else { l.infamy = num(args, 0) });
    done(format!("{cmd:#06x} {}", num(args, 0)))
}

// CS *IsPCAMurderer*: "1 if the player has ever murdered an NPC" — no crime system flags a murder,
// so only a script's `SetPCFactionMurder` does.
fn is_pca_murderer(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    flag(ledger(world, |l| l.any_flag(crate::obscript_ledger::FactionFlag::Murder)).unwrap_or(false))
}

// CS *SetPCFactionAttack <faction> <flag>*.
fn cmd_10c8(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match form_arg(args, 0) {
        Some(faction) => {
            let on = num(args, 1) != 0.0;
            ledger_mut(world, |l| l.set_flag(crate::obscript_ledger::FactionFlag::Attack, faction, on));
            done(format!("SetPCFactionAttack {faction:08X} {on}"))
        }
        None => Action::Declined,
    }
}

// CS *ModFactionReaction <faction> <target faction> <value>*: "modify the specified faction's reaction
// towards the target faction by the specified amount" — on top of what scripts set (the authored
// number is not read back).
fn cmd_10a9(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match (form_arg(args, 0), form_arg(args, 1)) {
        (Some(faction), Some(other)) => {
            let now = ledger(world, |l| l.reaction(faction, other)).unwrap_or(0.0) + num(args, 2);
            ledger_mut(world, |l| l.set_reaction(faction, other, now));
            if crate::combat::set_faction_reaction(world, faction, other, now) {
                done(format!("ModFactionReaction {faction:08X} {other:08X} -> {now}"))
            } else {
                Action::Declined
            }
        }
        _ => Action::Declined,
    }
}

// CS *GetGold*: the player's gold — the count of `Gold001`.
fn get_gold(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    match player(world) {
        Some(player) => {
            answer(world.get::<Inventory>(player).map_or(0.0, |inventory| inventory.items.iter().filter(|stack| stack.base_form_id == crate::guards::CURRENCY_FORM).map(|stack| stack.count as f32).sum()))
        }
        None => Action::Declined,
    }
}

// CS *GetIsPlayerBirthsign <sign>*: the engine keeps no birthsign, so the player has none.
fn cmd_10e0(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    flag(false)
}

// CS *IsGuard*: "1 if the NPC's class is marked as a Guard class" — the `CLAS` Guard flag (Oblivion's).
fn is_guard(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => flag(crate::guards::is_guard(world, actor)),
        None => Action::Declined,
    }
}

// CS *IsInDangerousWater*: whether the actor is in water that hurts (lava).
fn is_in_dangerous_water(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => match super::engine_queries(world).in_dangerous_water {
            Some(dangerous) => flag(dangerous(world, actor)),
            None => Action::Declined,
        },
        None => Action::Declined,
    }
}

// CS *SetActorRefraction <0..10>* (`SAR`): "how transparent they are" — drawn through the opacity
// override (0 opaque, 10 the most transparent).
fn sar(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => store(world, 0x1129, form, (1.0 - num(args, 0).clamp(0.0, 10.0) / 10.0).clamp(0.0, 1.0)),
        None => Action::Declined,
    }
}

// CS *Wait <package>*: the follower waits ("stopped waiting" by *StopWaiting*): kept.
fn stop_waiting(world: &World, _fallout: bool, cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((subject, _)) => store(world, cmd, subject, 1.0),
        None => Action::Declined,
    }
}

// CS *Drop <item> <count>*: "calling actor drops the specified item(s) in the world at his/her feet".
fn cmd_1057(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller(world, ctx), form_arg(args, 0)) {
        (Some((at, _)), Some(item)) => {
            let count = num(args, 1).max(1.0);
            let removed = crate::obscript_effects::run(world, 0x1052, &[ObScriptValue::Ref(item), ObScriptValue::Num(count)], ctx);
            if !matches!(removed, Action::Done { .. }) {
                return Action::Declined;
            }
            let Some(mut created) = world.try_resource_mut::<crate::CreatedReferences>() else { return Action::Declined };
            let Some(form) = created.scripted_ids().next() else { return Action::Declined };
            created.create_scripted(form, item, at, false);
            done(format!("Drop {item:08X} x{count} at {at:08X}"))
        }
        _ => Action::Declined,
    }
}

// CS *DeleteFullActorCopy*: "deletes a created reference and its base object" — and "acts as a return".
fn delete_full_actor_copy(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => {
            if let Some(mut created) = world.try_resource_mut::<crate::CreatedReferences>() {
                created.mark_deleted(form);
            }
            crate::obscript_actions::run(world, 0x1022, &[], ctx);
            done(format!("DeleteFullActorCopy {form:08X}"))
        }
        None => Action::Declined,
    }
}

// ── cells and quests ───────────────────────────────────────────────────────────────────────
// GECK *SetCellOwnership <cell> [owner]*: the owner kept for the cell (none: the player).
fn cmd_1119(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match form_arg(args, 0) {
        Some(cell) => {
            let owner = form_arg(args, 1).unwrap_or(crate::PLAYER_REFERENCE);
            ledger_mut(world, |l| l.set_form(cmd, cell, owner));
            done(format!("SetCellOwnership {cell:08X} {owner:08X}"))
        }
        None => Action::Declined,
    }
}

// GECK *SetCellFullName <cell> <message>*: the cell's display name, kept.
fn cmd_111b(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match (form_arg(args, 0), form_arg(args, 1)) {
        (Some(cell), Some(message)) => {
            ledger_mut(world, |l| l.set_form(cmd, cell, message));
            done(format!("SetCellFullName {cell:08X}"))
        }
        _ => Action::Declined,
    }
}

// GECK *ResetInterior <cell>*: "marks the cell as expired so that the next time it is loaded it will be
// cleaned up as if several days have passed".
fn cmd_1127(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match form_arg(args, 0) {
        Some(cell) => {
            if let Some(mut visits) = world.try_resource_mut::<crate::obscript_state::CellVisits>() {
                visits.expire(cell);
            }
            done(format!("ResetInterior {cell:08X}"))
        }
        None => Action::Declined,
    }
}

// GECK *SetZoneRespawns <zone> <flag>*: kept for the encounter zone.
fn cmd_11ea(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match form_arg(args, 0) {
        Some(zone) => store(world, cmd, zone, num(args, 1)),
        None => Action::Declined,
    }
}

// GECK *GetInZone <zone>*: "1 if the designated target is in the encounter zone" — the zone of the cell
// the actor stands in.
fn cmd_11be(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller_or_player(world, ctx), form_arg(args, 0)) {
        (Some((_, entity)), Some(zone)) => {
            let cell = world
                .get::<byroredux_core::ecs::components::CellRoot>(entity)
                .and_then(|root| world.get::<byroredux_core::ecs::components::CellFormId>(root.0).map(|id| id.0));
            let in_zone = cell.and_then(|cell| world.try_resource::<CellZones>().and_then(|zones| zones.0.get(&cell).copied()));
            flag(in_zone == Some(zone))
        }
        _ => Action::Declined,
    }
}

// GECK *DisableNavMesh <navmesh ref>*: kept; the pathing does not read it yet.
fn cmd_11bf(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match form_arg(args, 0) {
        Some(mesh) => store(world, cmd, mesh, 1.0),
        None => Action::Declined,
    }
}

// GECK *ForceActiveQuest <quest>*: the quest the journal shows first, kept.
fn cmd_11dd(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match form_arg(args, 0) {
        Some(quest) => {
            ledger_mut(world, |l| l.set_form(cmd, 0, quest));
            done(format!("ForceActiveQuest {quest:08X}"))
        }
        None => Action::Declined,
    }
}

// GECK *FadeSFX <seconds>*: the sound effects fade; kept.
fn cmd_1227(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    store(world, cmd, 0, num(args, 0))
}

// GECK *GetIsLockBroken* above; *IsPlayerActionActive <index>* (FO3): the player's current action —
// only jumping is tracked (index 6).
fn get_is_lock_broken_command(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    let action = num(args, 0) as i32;
    match (action, super::engine_queries(world).player_airborne) {
        (6, Some(airborne)) => flag(airborne(world)),
        (1..=10, _) => flag(false),
        _ => Action::Declined,
    }
}

// CS *GetPCMiscStat <id>* (Oblivion numbers its stats; the page lists 0-33).
fn cmd_1138(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match oblivion_misc_stat(num(args, 0) as usize) {
        Some(name) => answer(super::misc_stat(world, name) as f32),
        None => Action::Declined,
    }
}

// CS *ModPCMiscStat <id> <amount>* (Oblivion).
fn cmd_1137(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match oblivion_misc_stat(num(args, 0) as usize) {
        Some(name) => {
            super::mod_misc_stat(world, name, num(args, 1) as i32);
            done(format!("ModPCMiscStat {name}"))
        }
        None => Action::Declined,
    }
}

// GECK *GetPCSleepHours*: the hours the player has set to sleep.
fn get_pc_sleep_hours(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    answer(world.try_resource::<PendingSleep>().map_or(0.0, |sleep| sleep.0.floor()))
}

/// The commands this file runs (#6139).
pub(crate) const TAIL_COMMANDS: &[Command] = &[
    Command { id: 0x1135, games: Games::Both, needs_caller: false, handler: is_xbox },
    Command { id: 0x120c, games: Games::Fallout, needs_caller: false, handler: cmd_120c },
    Command { id: 0x126f, games: Games::Fallout, needs_caller: false, handler: cmd_126f },
    Command { id: 0x11fb, games: Games::Fallout, needs_caller: false, handler: cmd_11fb },
    Command { id: 0x11b5, games: Games::Fallout, needs_caller: false, handler: exit_game },
    Command { id: 0x1114, games: Games::Fallout, needs_caller: false, handler: cmd_1114 },
    Command { id: 0x11a7, games: Games::Fallout, needs_caller: false, handler: tune },
    Command { id: 0x1201, games: Games::Fallout, needs_caller: false, handler: reset_pipboy_manager },
    Command { id: 0x124b, games: Games::Fallout, needs_caller: false, handler: cmd_124b },
    Command { id: 0x124d, games: Games::Fallout, needs_caller: false, handler: detonate_placed_explosives },
    Command { id: 0x1163, games: Games::Fallout, needs_caller: false, handler: preload_magic_effect },
    Command { id: 0x1186, games: Games::Fallout, needs_caller: false, handler: set_global_time_multiplier },
    Command { id: 0x121f, games: Games::Fallout, needs_caller: false, handler: sqrt },
    Command { id: 0x1221, games: Games::Fallout, needs_caller: false, handler: cmd_1221 },
    Command { id: 0x121e, games: Games::Fallout, needs_caller: false, handler: cmd_121e },
    Command { id: 0x10d9, games: Games::Oblivion, needs_caller: false, handler: cmd_10d9 },
    Command { id: 0x10ea, games: Games::Oblivion, needs_caller: false, handler: cmd_10ea },
    Command { id: 0x10eb, games: Games::Oblivion, needs_caller: false, handler: cmd_10eb },
    Command { id: 0x1268, games: Games::Fallout, needs_caller: false, handler: cmd_1268 },
    Command { id: 0x11e1, games: Games::Fallout, needs_caller: false, handler: cmd_11e1 },
    Command { id: 0x1218, games: Games::Fallout, needs_caller: false, handler: set_special_points },
    Command { id: 0x121a, games: Games::Fallout, needs_caller: false, handler: cmd_121a },
    Command { id: 0x1245, games: Games::Fallout, needs_caller: false, handler: cmd_1245 },
    Command { id: 0x11ee, games: Games::Fallout, needs_caller: false, handler: cmd_11ee },
    Command { id: 0x10da, games: Games::Both, needs_caller: false, handler: show_race_menu },
    Command { id: 0x10e7, games: Games::Fallout, needs_caller: false, handler: cmd_10e7 },
    Command { id: 0x126d, games: Games::Fallout, needs_caller: false, handler: reset_xp },
    Command { id: 0x11cd, games: Games::Fallout, needs_caller: false, handler: cmd_11cd },
    Command { id: 0x11ce, games: Games::Fallout, needs_caller: false, handler: cmd_11ce },
    Command { id: 0x106b, games: Games::Both, needs_caller: false, handler: get_knocked_state },
    Command { id: 0x1187, games: Games::Fallout, needs_caller: false, handler: get_hit_location },
    Command { id: 0x116d, games: Games::Fallout, needs_caller: false, handler: is_child },
    Command { id: 0x1120, games: Games::Fallout, needs_caller: false, handler: cmd_1120 },
    Command { id: 0x1133, games: Games::Fallout, needs_caller: false, handler: cmd_1133 },
    Command { id: 0x1148, games: Games::Fallout, needs_caller: false, handler: dispel_all_spells },
    Command { id: 0x1184, games: Games::Fallout, needs_caller: false, handler: cmd_1184 },
    Command { id: 0x1214, games: Games::Fallout, needs_caller: false, handler: remove_from_all_factions },
    Command { id: 0x11f5, games: Games::Fallout, needs_caller: false, handler: cmd_11f5 },
    Command { id: 0x11e5, games: Games::Fallout, needs_caller: false, handler: cmd_11e5 },
    Command { id: 0x11df, games: Games::Fallout, needs_caller: false, handler: cmd_11df },
    Command { id: 0x10f0, games: Games::Oblivion, needs_caller: false, handler: cmd_10f0 },
    Command { id: 0x120a, games: Games::Fallout, needs_caller: false, handler: get_is_lock_broken },
    Command { id: 0x1228, games: Games::Fallout, needs_caller: false, handler: set_no_rumors },
    Command { id: 0x11fc, games: Games::Fallout, needs_caller: false, handler: set_no_rumors },
    Command { id: 0x1141, games: Games::Oblivion, needs_caller: false, handler: set_no_rumors },
    Command { id: 0x11d8, games: Games::Fallout, needs_caller: false, handler: clear_destruction },
    Command { id: 0x125b, games: Games::Fallout, needs_caller: false, handler: cmd_125b },
    Command { id: 0x123b, games: Games::Fallout, needs_caller: false, handler: cmd_123b },
    Command { id: 0x10fc, games: Games::Oblivion, needs_caller: false, handler: cmd_10fc },
    Command { id: 0x10fd, games: Games::Oblivion, needs_caller: false, handler: cmd_10fc },
    Command { id: 0x10b0, games: Games::Oblivion, needs_caller: false, handler: is_pca_murderer },
    Command { id: 0x10c8, games: Games::Oblivion, needs_caller: false, handler: cmd_10c8 },
    Command { id: 0x10a9, games: Games::Oblivion, needs_caller: false, handler: cmd_10a9 },
    Command { id: 0x1030, games: Games::Oblivion, needs_caller: false, handler: get_gold },
    Command { id: 0x10e0, games: Games::Oblivion, needs_caller: false, handler: cmd_10e0 },
    Command { id: 0x107d, games: Games::Oblivion, needs_caller: false, handler: is_guard },
    Command { id: 0x114c, games: Games::Oblivion, needs_caller: false, handler: is_in_dangerous_water },
    Command { id: 0x1154, games: Games::Oblivion, needs_caller: false, handler: sar },
    Command { id: 0x10b7, games: Games::Oblivion, needs_caller: false, handler: stop_waiting },
    Command { id: 0x1057, games: Games::Oblivion, needs_caller: false, handler: cmd_1057 },
    Command { id: 0x1168, games: Games::Oblivion, needs_caller: false, handler: delete_full_actor_copy },
    Command { id: 0x1119, games: Games::Fallout, needs_caller: false, handler: cmd_1119 },
    Command { id: 0x111b, games: Games::Fallout, needs_caller: false, handler: cmd_111b },
    Command { id: 0x1127, games: Games::Fallout, needs_caller: false, handler: cmd_1127 },
    Command { id: 0x11ea, games: Games::Fallout, needs_caller: false, handler: cmd_11ea },
    Command { id: 0x11be, games: Games::Fallout, needs_caller: false, handler: cmd_11be },
    Command { id: 0x11bf, games: Games::Fallout, needs_caller: false, handler: cmd_11bf },
    Command { id: 0x11dd, games: Games::Fallout, needs_caller: false, handler: cmd_11dd },
    Command { id: 0x1227, games: Games::Fallout, needs_caller: false, handler: cmd_1227 },
    Command { id: 0x1170, games: Games::Fallout, needs_caller: false, handler: get_is_lock_broken_command },
    Command { id: 0x1138, games: Games::Oblivion, needs_caller: false, handler: cmd_1138 },
    Command { id: 0x1137, games: Games::Oblivion, needs_caller: false, handler: cmd_1137 },
    Command { id: 0x10bc, games: Games::Fallout, needs_caller: false, handler: get_pc_sleep_hours },
];


use super::PendingSleep;

/// Oblivion's misc stats by the number `GetPCMiscStat` takes (the CS page's list).
pub(crate) const OBLIVION_MISC_STATS: [&str; 34] = [
    "Days In Prison",
    "Days Passed",
    "Skill Increases",
    "Training Sessions",
    "Largest Bounty",
    "Creatures Killed",
    "People Killed",
    "Places Discovered",
    "Locks Picked",
    "Picks Broken",
    "Souls Trapped",
    "Ingredients Eaten",
    "Potions Made",
    "Oblivion Gates Shut",
    "Horses Owned",
    "Houses Owned",
    "Stores Invested In",
    "Books Read",
    "Skill Books Read",
    "Artifacts Found",
    "Hours Slept",
    "Hours Waited",
    "Days As A Vampire",
    "Last Day As Vampire",
    "People Fed On",
    "Jokes Told",
    "Diseases Contracted",
    "Nirnroots Found",
    "Items Stolen",
    "Items Pickpocketed",
    "Trespasses",
    "Assaults",
    "Murders",
    "Horses Stolen",
];

fn oblivion_misc_stat(index: usize) -> Option<&'static str> {
    OBLIVION_MISC_STATS.get(index).copied()
}
