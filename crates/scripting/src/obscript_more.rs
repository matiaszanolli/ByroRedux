//! The rest of the ObScript command surface (M47.3): the commands that read engine state through a
//! query, keep a ledger entry ([`crate::obscript_ledger`]), or hand a request to the app.
//!
//! The commands are rows of the command table ([`crate::obscript_cmd`], #6139), each keyed by the game
//! it runs in. Ids that are one number in both families but different commands (`0x116c` is
//! `DamageObject` in Fallout and `SetPlayerInSEWorld` in Oblivion) are told apart by the loaded game.
//!
//! Every answer comes from a source this module names: the GECK / CS page, the engine state it
//! reads, or the ledger. Where the engine has no state for what a command asks (nobody can crouch; a
//! weapon cannot be dismembered; no mount exists) it answers what the engine's state is, and the
//! command's comment says so — never a number the engine cannot back.

use byroredux_core::ecs::components::GlobalTransform;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;

use crate::obscript_actions::{Action, Context};
use crate::obscript_cmd::{Command, Games};
use crate::obscript_effects::{form_arg, live, num};
use crate::obscript_ledger::ObScriptLedger;
use crate::obscript_vm::ObScriptValue;

mod actions;
mod extras;
mod state;
mod tail;
pub(crate) use state::STATE_COMMANDS;
pub(crate) use tail::TAIL_COMMANDS;
pub use tail::{ChildRaces, CellZones, ExitGameRequest, PendingVideos, PlayableRaces};
pub(crate) use actions::ACTIONS_COMMANDS;
pub(crate) use extras::EXTRAS_COMMANDS;
pub use extras::{AshPiles, MusicTrack, MusicTracks, PendingMusic, PendingTextureSwaps, ReloadRequest, TextArguments, TextureSwap, UseWeaponRequest};

/// The point the crosshair is aimed at, within a reach, in Z-up world units.
pub type ReticlePointFn = fn(&World, f32) -> Option<[f32; 3]>;
/// The placement of a point in the player's cell.
pub type PlacementAtFn = fn(&World, [f32; 3]) -> Option<crate::created_references::CreatedPlacement>;

/// Functions of engine state that need systems this crate does not link (physics, the camera, the
/// UI), installed by the app at boot. A missing one declines the command that asks.
#[derive(Default, Clone, Copy)]
pub struct EngineQueries {
    /// Whether the first reference has line of sight to the second (`GetLOS`).
    pub line_of_sight: Option<fn(&World, EntityId, EntityId) -> bool>,
    /// Whether the first actor would attack the second (`GetShouldAttack`).
    pub should_attack: Option<fn(&World, EntityId, EntityId) -> bool>,
    /// Whether the camera is first person (`IsPC1stPerson`).
    pub first_person: Option<fn(&World) -> bool>,
    /// Whether a menu of the given type or code is open (`MenuMode`); `0` asks about any menu.
    pub menu_open: Option<fn(&World, u32) -> bool>,
    /// The point the crosshair is aimed at, within `distance` (`0` for the default reach), in Z-up world
    /// units (`PlaceAtReticle`).
    pub reticle_point: Option<ReticlePointFn>,
    /// The actors in contact with a trap right now and how fast the trap moves relative to each
    /// (`TrapUpdate`), in world units per second.
    pub trap_contacts: Option<fn(&World, EntityId) -> Vec<TrapContact>>,
    /// Whether an actor stands in water that hurts (lava) (`IsInDangerousWater`).
    pub in_dangerous_water: Option<fn(&World, EntityId) -> bool>,
    /// Whether the player is in the air (`IsPlayerActionActive 6`, jumping).
    pub player_airborne: Option<fn(&World) -> bool>,
    /// The placement of a point in the player's cell, for a reference created there.
    pub placement_at: Option<PlacementAtFn>,
}

impl Resource for EngineQueries {}

/// An actor touching a trap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrapContact {
    pub actor: EntityId,
    /// The speed of the trap relative to the actor (CS *TrapUpdate*: "a combination of actor and traps
    /// respective velocities"), in Bethesda units per second.
    pub relative_speed: f32,
}

/// Who touched each trap at its last `TrapUpdate`, by trap reference: a trap that is not *continuous*
/// "only deal[s] damage on first contact". `NOT_SAVED_BY_DESIGN`: contact is re-derived every frame.
#[derive(Debug, Default)]
pub struct ObTrapContacts(pub std::collections::BTreeMap<u32, std::collections::BTreeSet<EntityId>>);

impl Resource for ObTrapContacts {}

/// Hours the player was told to sleep (`SetPCSleepHours`), for the app to run the clock through.
/// `NOT_SAVED_BY_DESIGN`: consumed within a frame.
#[derive(Debug, Default)]
pub struct PendingSleep(pub f32);

impl Resource for PendingSleep {}

/// A menu a script asked for, for the UI that shows it. `NOT_SAVED_BY_DESIGN`: drained by the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuRequest {
    pub menu: &'static str,
    /// The form the menu is about (a recipe list, a map marker, a casino), if any.
    pub form: u32,
}

#[derive(Debug, Default)]
pub struct PendingMenus(pub Vec<MenuRequest>);

impl Resource for PendingMenus {}

/// Knock `actor` down and push it away from `from` with `force` (`PushActorAway`); consumed by the
/// app's knockdown system. `NOT_SAVED_BY_DESIGN`: a one-frame request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PushActorRequest {
    pub from: EntityId,
    pub force: f32,
}

impl byroredux_core::ecs::storage::Component for PushActorRequest {
    type Storage = byroredux_core::ecs::sparse_set::SparseSetStorage<Self>;
}

/// `ForceFlee`: the actor looks for a threat and runs from it. `NOT_SAVED_BY_DESIGN`: one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FleeRequest;

impl byroredux_core::ecs::storage::Component for FleeRequest {
    type Storage = byroredux_core::ecs::sparse_set::SparseSetStorage<Self>;
}

/// `ResetInventory`: put the reference's inventory back to what its record authors.
/// `NOT_SAVED_BY_DESIGN`: one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResetInventoryRequest;

impl byroredux_core::ecs::storage::Component for ResetInventoryRequest {
    type Storage = byroredux_core::ecs::sparse_set::SparseSetStorage<Self>;
}

/// This subsystem's resources and components.
pub fn install(world: &mut World) {
    world.init_resource::<EngineQueries>();
    world.init_resource::<PendingSleep>();
    world.init_resource::<PendingMenus>();
    world.init_resource::<ObTrapContacts>();
    world.register::<PushActorRequest>();
    world.register::<FleeRequest>();
    world.register::<ResetInventoryRequest>();
    extras::install(world);
    world.init_resource::<PendingVideos>();
    world.init_resource::<ExitGameRequest>();
    world.init_resource::<CellZones>();
    world.init_resource::<ChildRaces>();
    world.init_resource::<PlayableRaces>();
}

/// A getter's answer as the host's `Action`.
fn answer(value: f32) -> Action {
    Action::number(value)
}

fn flag(value: bool) -> Action {
    answer(f32::from(u8::from(value)))
}

fn done(effect: impl Into<String>) -> Action {
    Action::Done { value: None, effect: effect.into() }
}

fn player(world: &World) -> Option<EntityId> {
    crate::papyrus_demo::PapyrusPlayerEntity::get(world)
}

/// The calling reference: its form and its live entity.
fn caller(world: &World, ctx: &Context) -> Option<(u32, EntityId)> {
    let form = ctx.caller.filter(|&form| form != 0)?;
    live(world, form).map(|entity| (form, entity))
}

/// Oblivion's Shivering Isles world flag (CS *GetPlayerInSEWorld*: "1 if the player is currently in the
/// Shivering Isles"), the engine flag *SetPlayerInSEWorld* (`0x116c`) writes into the ledger; 0 until a script
/// sets it, as on a new game in Tamriel. The quest-level dialogue gate (condition 365) reads it too.
pub(crate) fn player_in_se_world(world: &World) -> f32 {
    ledger(world, |l| l.value(0x116c, 0)).flatten().unwrap_or(0.0)
}

fn ledger<R>(world: &World, f: impl FnOnce(&ObScriptLedger) -> R) -> Option<R> {
    world.try_resource::<ObScriptLedger>().map(|ledger| f(&ledger))
}

fn ledger_mut<R>(world: &World, f: impl FnOnce(&mut ObScriptLedger) -> R) -> Option<R> {
    world.try_resource_mut::<ObScriptLedger>().map(|mut ledger| f(&mut ledger))
}

/// The reference FormIDs of the factions `entity` belongs to.
fn faction_ids(world: &World, entity: EntityId) -> Vec<u32> {
    world
        .get::<byroredux_core::ecs::components::FactionRanks>(entity)
        .map(|ranks| ranks.0.iter().filter(|(_, rank)| *rank >= 0).map(|(faction, _)| *faction).collect())
        .unwrap_or_default()
}

/// Store the value a `Set...` command was given, for its `Get...` (and for the system that honours it).
fn store(world: &World, cmd: u16, subject: u32, value: f32) -> Action {
    if ledger_mut(world, |ledger| ledger.set_value(cmd, subject, value)).is_none() {
        return Action::Declined;
    }
    done(format!("{cmd:#06x} {subject:08X} {value}"))
}

/// `GetCurrentAIPackage`'s value for a PACK procedure type (CS page: 0 EXPLORE, 1 FOLLOW, 2 ESCORT, 3 EAT,
/// 4 SLEEP, 5 COMBAT, 6 DIALOGUE, ..., 13 WANDER, 14 TRAVEL, 15 ACCOMPANY, 16 USEITEMAT, 17 AMBUSH,
/// 18 FLEE_NON_COMBAT, 19 CAST_MAGIC, 33 USE_WEAPON, 35 GUARD, 36 SANDBOX, 37 PATROL).
fn package_type_code(procedure_type: u32) -> Option<f32> {
    Some(match procedure_type {
        0 => 0.0,
        1 => 1.0,
        2 => 2.0,
        3 => 3.0,
        4 => 4.0,
        5 => 13.0,
        6 => 14.0,
        7 => 15.0,
        8 => 16.0,
        9 => 17.0,
        10 => 18.0,
        11 => 19.0,
        12 => 36.0,
        13 => 37.0,
        14 => 35.0,
        15 => 6.0,
        16 => 33.0,
        _ => return None,
    })
}

/// `GetCurrentAIProcedure`'s value for a PACK procedure type (CS page: 0 TRAVEL, 3 WAIT, 4 DIALOGUE, 7 WANDER,
/// 8 SLEEP, 10 EAT, 11 FOLLOW, 12 ESCORT, 13 COMBAT, 27 ACCOMPANY, 28 USE_ITEM_AT, 34 FLEE_NON_COMBAT,
/// 41 PATROL, 42 USE_WEAPON, 44 GUARD, 45 SANDBOX). A package that has not reached its destination is
/// still the travel procedure; one with none running is DONE (17).
fn procedure_code(procedure_type: u32) -> Option<f32> {
    Some(match procedure_type {
        0 => 2.0,
        1 => 11.0,
        2 => 12.0,
        3 => 10.0,
        4 => 8.0,
        5 => 7.0,
        6 => 0.0,
        7 => 27.0,
        8 => 28.0,
        9 => 30.0,
        10 => 34.0,
        11 => 33.0,
        12 => 45.0,
        13 => 41.0,
        14 => 44.0,
        15 => 4.0,
        16 => 42.0,
        _ => return None,
    })
}

/// The procedure type of the package an actor runs now (its reported package, in the registry).
fn running_procedure_type(world: &World, actor: EntityId) -> Option<u32> {
    let package = crate::reported_package(world, actor)??;
    Some(world.try_resource::<crate::package::PackageRegistry>()?.package(package)?.procedure_type)
}

// GECK *GetLOS*: 1 when the calling actor has line of sight to the object, asked through the
// sight test the app installs (physics).
fn get_los(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let queries = world.try_resource::<EngineQueries>().map(|queries| *queries).unwrap_or_default();
    let target = || form_arg(args, 0).and_then(|form| live(world, form));
    match (caller(world, ctx), target(), queries.line_of_sight) {
        (Some((_, from)), Some(to), Some(sees)) => flag(sees(world, from, to)),
        _ => Action::Declined,
    }
}

// `MenuMode [type]`: 1 while a menu of that type or code is open — the UI the app installs answers.
fn menu_mode(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    let queries = world.try_resource::<EngineQueries>().map(|queries| *queries).unwrap_or_default();
    match queries.menu_open {
        Some(open) => flag(open(world, num(args, 0) as u32)),
        None => Action::Declined,
    }
}

// GECK *GetSleeping*: 0 not sleeping, 3 is sleeping. An actor whose active package is Sleep has
// a `SleepBehavior`; the engine has no loading / getting-ready / getting-up states.
fn get_sleeping(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => answer(if world.has::<byroredux_core::ecs::components::SleepBehavior>(actor) { 3.0 } else { 0.0 }),
        None => Action::Declined,
    }
}

// GECK *GetTalkedToPC*: "1 if the NPC has ever spoken to the player".
fn get_talked_to_pc(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((form, _)) => flag(ledger(world, |l| l.talked_to_player(form)).unwrap_or(false)),
        None => Action::Declined,
    }
}

// GECK *IsRaining*: "1 if the current weather is stable and rainy" — the weather's classification
// is Rainy (2).
fn is_raining(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    let rainy = world
        .try_resource::<crate::weather_state::CurrentWeather>()
        .and_then(|current| current.0)
        .and_then(|weather| world.try_resource::<crate::weather_state::WeatherClassifications>().map(|c| c.of(weather)))
        .is_some_and(|class| class == 2);
    flag(rainy)
}

// CS *GetIsCreature*: 1 when the actor is a creature (`CreatureActor`).
fn get_is_creature(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => flag(world.has::<byroredux_core::ecs::components::CreatureActor>(actor)),
        None => Action::Declined,
    }
}

// GECK *GetIsCreatureType*: the `CREA`'s creature type (0 Animal, 1 Mutated Animal, 2 Mutated Insect,
// 3 Abomination, 4 Super Mutant, 5 Feral Ghoul, 6 Robot, 7 Giant).
fn get_is_creature_type(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match (caller(world, ctx), form_arg(args, 0).is_some() || !args.is_empty()) {
        (Some((_, actor)), true) => {
            let wanted = num(args, 0) as i32;
            let kind = crate::relationships::actor_base(world, actor)
                .and_then(|base| world.try_resource::<crate::actor_base_traits::ActorBaseTraits>()?.get(base))
                .and_then(|traits| traits.creature_type);
            flag(kind.is_some_and(|kind| i32::from(kind) == wanted))
        }
        _ => Action::Declined,
    }
}

// GECK / CS *GetIsRace* / *GetPCIsRace*: the actor's (the player's) race is the named one.
fn get_is_race(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let who = if cmd == 0x1082 { player(world) } else { caller(world, ctx).map(|(_, e)| e).or_else(|| player(world)) };
    match (who, form_arg(args, 0)) {
        (Some(who), Some(race)) => flag(world.get::<byroredux_core::character::Background>(who).is_some_and(|b| b.race_form_id == race)),
        _ => Action::Declined,
    }
}

// GECK *GetIsSex* / *GetPCIsSex*: 1 when the actor (the player) is of that sex (0 Male, 1 Female) —
// the base record's `ACBS` female flag.
fn get_is_sex(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let who = if cmd == 0x1083 { player(world) } else { caller(world, ctx).map(|(_, e)| e).or_else(|| player(world)) };
    match who.and_then(|who| crate::actor_base_traits::is_female(world, who)) {
        Some(female) => flag(female == (num(args, 0) as i32 == 1)),
        None => Action::Declined,
    }
}

// GECK *GetShouldAttack*: whether the calling actor would attack the target.
fn get_should_attack(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let queries = world.try_resource::<EngineQueries>().map(|queries| *queries).unwrap_or_default();
    let target = || form_arg(args, 0).and_then(|form| live(world, form));
    match (caller(world, ctx), target(), queries.should_attack) {
        (Some((_, from)), Some(to), Some(should)) => flag(should(world, from, to)),
        _ => Action::Declined,
    }
}

// GECK *ShowMap <marker> [enableFastTravel]*: the marker joins the player's map, travelable when asked.
fn cmd_1055(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    match form_arg(args, 0) {
        Some(marker) => {
            if let Some(mut markers) = world.try_resource_mut::<crate::MapMarkers>() {
                markers.add(marker, num(args, 1) != 0.0);
            }
            if let Some(mut menus) = world.try_resource_mut::<PendingMenus>() {
                menus.0.push(MenuRequest { menu: "map", form: marker });
            }
            done(format!("ShowMap {marker:08X}"))
        }
        None => Action::Declined,
    }
}

// GECK *AddTopic* declines: the topic list is built from the topics' own conditions when a
// conversation opens, and nothing gates a topic on the player's "known topics".
fn add_topic(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    Action::Declined
}

// GECK *GetPlayerControlsDisabled [movement pipboy fighting pov looking rollover sneaking]*: 1 when
// any of the named controls (every control when none is named) is disabled.
fn cmd_1062(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _ctx: &Context) -> Action {
    let Some(state) = world.try_resource::<crate::PlayerControlState>().map(|s| *s) else { return Action::Declined };
    let disabled = [
        !state.movement_enabled,
        !state.menu_enabled,
        !state.fighting_enabled,
        !state.camera_switching_enabled,
        !state.looking_enabled,
        false,
        !state.sneaking_enabled,
    ];
    let asked: Vec<usize> = (0..7).filter(|&i| num(args, i) != 0.0).collect();
    flag(if asked.is_empty() { disabled.iter().any(|&d| d) } else { asked.iter().any(|&i| disabled[i]) })
}

// GECK *GetHeadingAngle <object>*: "the angle between the calling reference and the specified
// object in a range from -180 to 180 degrees ... The direction the calling reference is facing is
// 0 degrees. Angles measured clockwise from 0 are positive."
fn cmd_1063(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let target = || form_arg(args, 0).and_then(|form| live(world, form));
    match (caller(world, ctx), target()) {
        (Some((_, from)), Some(to)) => match (world.get::<GlobalTransform>(from).map(|t| *t), world.get::<GlobalTransform>(to).map(|t| t.translation)) {
            (Some(from_t), Some(to_pos)) => {
                // A placement faces its local -Z (Gamebryo's +Y through the Z-up to Y-up conversion).
                let forward = from_t.rotation * byroredux_core::math::Vec3::NEG_Z;
                let toward = to_pos - from_t.translation;
                let (fh, th) = (forward.x.atan2(forward.z), toward.x.atan2(toward.z));
                let mut delta = th - fh;
                while delta > std::f32::consts::PI {
                    delta -= std::f32::consts::TAU;
                }
                while delta < -std::f32::consts::PI {
                    delta += std::f32::consts::TAU;
                }
                // Clockwise (seen from above) is positive: toward +X from a -Z heading is a right turn.
                answer(-delta.to_degrees())
            }
            _ => Action::Declined,
        },
        _ => Action::Declined,
    }
}

// GECK *CreateDetectionEvent <owner> <sound level> [type]* (Fallout 3): the event the detection
// queue records, at the calling reference.
fn cmd_1068(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((source, _)) => {
            if let Some(mut pending) = world.try_resource_mut::<crate::PendingDetectionEvents>() {
                pending.push(crate::DetectionEvent { source, owner: form_arg(args, 0).unwrap_or(0), sound_level: num(args, 1) as i32 });
            }
            done(format!("CreateDetectionEvent {source:08X}"))
        }
        None => Action::Declined,
    }
}

// GECK *GetWeaponAnimType* (Fallout): 0 AGW_NONE, 1 HandToHand, 2 OneHandMelee, 3 TwoHandMelee,
// 4 OneHandPistol (ballistic and energy), 5 TwoHandRifle (ballistic and energy), 6 TwoHandAutomatic,
// 7 TwoHandHandle, 8 TwoHandLauncher, 9 OneHandGrenade (and thrown), 10 OneHandMine,
// 11 OneHandLunchboxMine — from the equipped weapon's `DNAM` animation type.
fn get_weapon_anim_type(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => {
            let base = world.get::<byroredux_core::ecs::components::EquippedWeapon>(actor).map(|w| w.base_form_id);
            let raw = base.and_then(|base| world.try_resource::<crate::obscript_state::WeaponAnimTypes>()?.get(base));
            answer(match raw {
                None => 0.0,
                Some(0) => 1.0,
                Some(1) => 2.0,
                Some(2) => 3.0,
                Some(3 | 4) => 4.0,
                Some(5 | 7) => 5.0,
                Some(6) => 6.0,
                Some(8) => 7.0,
                Some(9) => 8.0,
                Some(10 | 13) => 9.0,
                Some(11) => 10.0,
                Some(12) => 11.0,
                Some(_) => 0.0,
            })
        }
        None => Action::Declined,
    }
}

// `GetCurrentAIPackage`: the package type the actor runs now (see [`package_type_code`]); an actor
// in combat is COMBAT (5).
fn get_current_ai_package(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => {
            if crate::is_in_combat(world, actor) {
                return answer(5.0);
            }
            match running_procedure_type(world, actor).and_then(package_type_code) {
                Some(code) => answer(code),
                None => Action::Declined,
            }
        }
        None => Action::Declined,
    }
}

// `GetCurrentAIProcedure`: the procedure the actor runs (see [`procedure_code`]); combat is 13, no
// package DONE (17).
fn get_current_ai_procedure(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => {
            if crate::is_in_combat(world, actor) {
                return answer(13.0);
            }
            match running_procedure_type(world, actor) {
                Some(kind) => procedure_code(kind).map_or(Action::Declined, answer),
                None => answer(17.0),
            }
        }
        None => Action::Declined,
    }
}

// GECK *IsIdlePlaying*: 1 when the actor is playing a special (scripted) idle.
fn is_idle_playing(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => flag(world.has::<crate::obscript_state::PlayingScriptedIdle>(actor)),
        None => Action::Declined,
    }
}

// GECK *HasBeenEaten*: 1 when the player (a cannibal) ate the actor. No actor can be eaten.
fn has_been_eaten(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    flag(false)
}

// GECK *IsTalking*: 1 when the actor is speaking a line.
fn is_talking(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], ctx: &Context) -> Action {
    match caller(world, ctx) {
        Some((_, actor)) => flag(world.try_resource::<crate::obscript_speech::PendingScriptedLines>().is_some_and(|lines| lines.speaking.iter().any(|line| line.speaker == actor))),
        None => Action::Declined,
    }
}

// GECK *IsCurrentFurnitureRef <furniture>*: 1 when the actor sits on that furniture.
fn cmd_10a2(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    let target = || form_arg(args, 0).and_then(|form| live(world, form));
    match (caller(world, ctx), target()) {
        (Some((_, actor)), Some(furniture)) => flag(world.get::<byroredux_core::ecs::components::Seated>(actor).is_some_and(|seat| seat.furniture == furniture)),
        _ => Action::Declined,
    }
}

// `GetFurnitureMarkerID`: its CS / GECK page is a stub ("Incomplete") and the scripts compare it with
// ids like 26 that are no marker index the engine keeps: declined, not guessed.
fn get_furniture_marker_id(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _ctx: &Context) -> Action {
    Action::Declined
}

/// The commands this file runs (#6139).
pub(crate) const HEAD_COMMANDS: &[Command] = &[
    Command { id: 0x101b, games: Games::Both, needs_caller: false, handler: get_los },
    Command { id: 0x1024, games: Games::Both, needs_caller: false, handler: menu_mode },
    Command { id: 0x1031, games: Games::Both, needs_caller: false, handler: get_sleeping },
    Command { id: 0x1032, games: Games::Both, needs_caller: false, handler: get_talked_to_pc },
    Command { id: 0x103e, games: Games::Both, needs_caller: false, handler: is_raining },
    Command { id: 0x1040, games: Games::Oblivion, needs_caller: false, handler: get_is_creature },
    Command { id: 0x11b6, games: Games::Fallout, needs_caller: false, handler: get_is_creature_type },
    Command { id: 0x1045, games: Games::Oblivion, needs_caller: false, handler: get_is_race },
    Command { id: 0x1082, games: Games::Both, needs_caller: false, handler: get_is_race },
    Command { id: 0x1046, games: Games::Fallout, needs_caller: false, handler: get_is_sex },
    Command { id: 0x1083, games: Games::Both, needs_caller: false, handler: get_is_sex },
    Command { id: 0x1042, games: Games::Fallout, needs_caller: false, handler: get_should_attack },
    Command { id: 0x1055, games: Games::Both, needs_caller: false, handler: cmd_1055 },
    Command { id: 0x1058, games: Games::Both, needs_caller: false, handler: add_topic },
    Command { id: 0x1062, games: Games::Both, needs_caller: false, handler: cmd_1062 },
    Command { id: 0x1063, games: Games::Both, needs_caller: false, handler: cmd_1063 },
    Command { id: 0x1068, games: Games::Fallout, needs_caller: false, handler: cmd_1068 },
    Command { id: 0x106c, games: Games::Fallout, needs_caller: false, handler: get_weapon_anim_type },
    Command { id: 0x106e, games: Games::Both, needs_caller: false, handler: get_current_ai_package },
    Command { id: 0x108f, games: Games::Both, needs_caller: false, handler: get_current_ai_procedure },
    Command { id: 0x1070, games: Games::Both, needs_caller: false, handler: is_idle_playing },
    Command { id: 0x107f, games: Games::Fallout, needs_caller: false, handler: has_been_eaten },
    Command { id: 0x108d, games: Games::Both, needs_caller: false, handler: is_talking },
    Command { id: 0x10a2, games: Games::Fallout, needs_caller: false, handler: cmd_10a2 },
    Command { id: 0x10a0, games: Games::Fallout, needs_caller: false, handler: get_furniture_marker_id },
];


/// The actors a script turned the AI off for (`SetActorsAI 0`, stored under the key `0x118b`).
pub fn ai_disabled_actors(world: &World) -> Vec<EntityId> {
    let forms: Vec<u32> = ledger(world, |l| l.values_of(0x118b)).unwrap_or_default().into_iter().filter(|&(_, value)| value == 0.0).map(|(form, _)| form).collect();
    forms.into_iter().filter_map(|form| live(world, form)).collect()
}

/// Whether a script put the player into the character-generation state (`SetInChargen 1`): "while in this
/// state, the player will not level up (although XP will accumulate as usual)".
pub fn in_chargen(world: &World) -> bool {
    ledger(world, |l| l.value(0x10e7, 0)).flatten().is_some_and(|flag| flag != 0.0)
}

/// The opacity scripts gave actors (`SetActorAlpha`, stored under `0x1129`), by reference.
pub fn actor_alphas(world: &World) -> Vec<(u32, f32)> {
    ledger(world, |l| l.values_of(0x1129)).unwrap_or_default()
}

/// The query functions the app installed (a default one, with none, when it installed none).
fn engine_queries(world: &World) -> EngineQueries {
    world.try_resource::<EngineQueries>().map(|queries| *queries).unwrap_or_default()
}

/// The player's count of misc stat `name` (`GetPCMiscStat "Locks Picked"`).
pub fn misc_stat(world: &World, name: &str) -> i32 {
    world.try_resource::<crate::misc_stats::MiscStats>().map_or(0, |stats| stats.get(name))
}

/// Add `amount` to misc stat `name` (`ModPCMiscStat "Locks Picked" 1`).
pub fn mod_misc_stat(world: &World, name: &str, amount: i32) {
    if let Some(mut stats) = world.try_resource_mut::<crate::misc_stats::MiscStats>() {
        stats.increment(name, amount);
    }
}

#[cfg(test)]
mod tests;
