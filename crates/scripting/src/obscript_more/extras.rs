//! The last slice of the command surface of [`super`]: the challenge, casino, calendar, music, texture,
//! weapon-use, placement and reload commands, with the requests and derived tables they need.

use std::collections::HashMap;

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_plugin::esm::records::EsmIndex;

use super::{answer, caller, done, flag, ledger, MenuRequest, PendingMenus};
use crate::obscript_actions::{Action, Context};
use crate::obscript_effects::{form_arg, live, num};
use crate::obscript_vm::ObScriptValue;

/// The strings of the command being run, handed over by the VM just before the call (`SwapTexture`'s
/// slot and texture, `EssentialDeathReload`'s message). `NOT_SAVED_BY_DESIGN`: transient.
#[derive(Debug, Default)]
pub struct TextArguments(pub Vec<String>);

impl byroredux_core::ecs::Resource for TextArguments {}

/// A track `PlayMusic` can name, by music type.
#[derive(Debug, Clone, PartialEq)]
pub struct MusicTrack {
    pub form_id: u32,
    /// Relative to the `music\` archive folder.
    pub file: String,
    pub looping: bool,
    /// The authored level; only a negative value attenuates.
    pub decibels: f32,
}

/// The load order's music types (`MUSC`) by lowercased editor ID. `NOT_SAVED_BY_DESIGN`: rebuilt from the
/// records at every content load.
#[derive(Debug, Default, Clone)]
pub struct MusicTracks {
    by_editor_id: crate::name_table::EditorIdTable<MusicTrack>,
    /// FormID → its editor ID, for `by_form`.
    by_form: HashMap<u32, String>,
}

impl byroredux_core::ecs::Resource for MusicTracks {}

impl MusicTracks {
    pub fn from_index(index: &EsmIndex) -> Self {
        let mut out = Self::default();
        for musc in index.music_types.values().filter(|musc| !musc.file.is_empty() && !musc.editor_id.is_empty()) {
            out.by_form.insert(musc.form_id, musc.editor_id.clone());
            out.by_editor_id.insert(&musc.editor_id, MusicTrack { form_id: musc.form_id, file: musc.file.clone(), looping: musc.looping(), decibels: musc.decibels });
        }
        out
    }

    pub fn by_editor_id(&self, name: &str) -> Option<&MusicTrack> {
        self.by_editor_id.get(name)
    }

    pub fn by_form(&self, form: u32) -> Option<&MusicTrack> {
        self.by_editor_id.get(self.by_form.get(&form)?)
    }
}

/// A track a script asked to play (`PlayMusic`), for the app's audio. `NOT_SAVED_BY_DESIGN`: drained by
/// the audio system the frame it is raised.
#[derive(Debug, Default)]
pub struct PendingMusic(pub Vec<MusicTrack>);

impl byroredux_core::ecs::Resource for PendingMusic {}

/// A texture swap a script asked for (`SwapTexture <ref> "<slot>" "<texture>"`): the meshes under `target`
/// whose texture is named `slot` take `texture`. `NOT_SAVED_BY_DESIGN`: drained by the render-side swap
/// system the frame it is raised.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureSwap {
    pub target: EntityId,
    pub slot: String,
    pub texture: String,
}

#[derive(Debug, Default)]
pub struct PendingTextureSwaps(pub Vec<TextureSwap>);

impl byroredux_core::ecs::Resource for PendingTextureSwaps {}

/// `UseWeapon`: the actor draws `weapon` and uses it on `target` `shots` times. `NOT_SAVED_BY_DESIGN`:
/// the app's weapon-use system consumes it over the next seconds; a save mid-volley loses the rest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UseWeaponRequest {
    pub weapon: u32,
    /// The target's reference.
    pub target: u32,
    pub shots: u32,
}

impl byroredux_core::ecs::storage::Component for UseWeaponRequest {
    type Storage = byroredux_core::ecs::sparse_set::SparseSetStorage<Self>;
}

/// `EssentialDeathReload "message"`: the game is to show the message, then load the last save.
/// `NOT_SAVED_BY_DESIGN`: consumed the frame it is raised.
#[derive(Debug, Default)]
pub struct ReloadRequest(pub Option<String>);

impl byroredux_core::ecs::Resource for ReloadRequest {}

/// The two ash-pile activators (`DefaultAshPile1`, `DefaultAshPile2`: *AttachAshPile* "iType: 1 = normal
/// ash pile, 2 = plasma goo pile"). `NOT_SAVED_BY_DESIGN`: found in the records at every content load.
#[derive(Debug, Default, Clone, Copy)]
pub struct AshPiles {
    pub normal: u32,
    pub goo: u32,
}

impl byroredux_core::ecs::Resource for AshPiles {}

impl AshPiles {
    pub fn from_index(index: &EsmIndex) -> Self {
        let find = |name: &str| index.activators.iter().find(|(_, acti)| acti.editor_id.eq_ignore_ascii_case(name)).map(|(&form, _)| form).unwrap_or(0);
        Self { normal: find("DefaultAshPile1"), goo: find("DefaultAshPile2") }
    }
}

/// This slice's resources and components.
pub fn install(world: &mut World) {
    world.init_resource::<TextArguments>();
    world.init_resource::<MusicTracks>();
    world.init_resource::<PendingMusic>();
    world.init_resource::<PendingTextureSwaps>();
    world.init_resource::<ReloadRequest>();
    world.init_resource::<AshPiles>();
    world.register::<UseWeaponRequest>();
}

fn texts(world: &World) -> Vec<String> {
    world.try_resource_mut::<TextArguments>().map(|mut texts| std::mem::take(&mut texts.0)).unwrap_or_default()
}

fn elapsed_days(world: &World) -> u64 {
    world.try_resource::<crate::globals::GameCalendar>().map_or(0, |calendar| calendar.days_elapsed)
}

pub(super) fn run(world: &World, fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
    match cmd {
        // ── challenges (GECK *Challenge*) ──────────────────────────────────────────────────────────
        0x125e | 0x126c => match form_arg(args, 0) {
            Some(challenge) => {
                let known = if cmd == 0x125e { crate::challenges::unlock(world, challenge) } else { crate::challenges::remove_recurring(world, challenge) };
                if known {
                    done(format!("{} {challenge:08X}", if cmd == 0x125e { "UnlockChallenge" } else { "RemoveRecurringFromChallenge" }))
                } else {
                    Action::Declined
                }
            }
            None => Action::Declined,
        },
        0x1266 => match form_arg(args, 0).and_then(|challenge| crate::challenges::completed(world, challenge)) {
            Some(completed) => flag(completed),
            None => Action::Declined,
        },

        // ── casinos ────────────────────────────────────────────────────────────────────────────────
        // `GetCasinoWinningsLevel <casino>`: the winnings level (1 small, 2 big, 3 whale, 4 banned — the
        // casino-comps scripts' `SmallFry` / `BigWinner` / `Whale` / `Banned`) of what the player has won
        // there. The engine runs no casino game, so nothing has been won: level 0.
        0x125c => match form_arg(args, 0) {
            Some(casino) => answer(ledger(world, |l| l.value(0x125c, casino)).flatten().unwrap_or(0.0)),
            None => Action::Declined,
        },
        // The blackjack, roulette and slot-machine screens: asked of the UI that shows them.
        0x1254 | 0x1256 | 0x1252 => {
            let menu = match cmd {
                0x1254 => "blackjack",
                0x1256 => "roulette",
                _ => "slot machine",
            };
            let casino = form_arg(args, 0).unwrap_or(0);
            if let Some(mut menus) = world.try_resource_mut::<PendingMenus>() {
                menus.0.push(MenuRequest { menu, form: casino });
            }
            done(format!("Show{menu}Menu {casino:08X}"))
        }

        // ── calendar ───────────────────────────────────────────────────────────────────────────────
        // GECK / CS *GetDayOfWeek*: 0 Sunday .. 6 Saturday.
        0x10aa => answer(crate::globals::day_of_week(world, fallout, elapsed_days(world)) as f32),

        // ── actors ─────────────────────────────────────────────────────────────────────────────────
        // `GetOwnerLastTarget`: the last actor the calling actor (the owner of the effect or script)
        // struck. No page documents it; the one vanilla use is `Set UnluckyDude to GetOwnerLastTarget`
        // in the Miss Fortune ability's `OnHit`, which then casts at him.
        0x124e => match caller(world, ctx) {
            Some((form, _)) => Action::Done {
                value: Some(ObScriptValue::Ref(ledger(world, |l| l.last_target(form)).flatten().unwrap_or(0))),
                effect: String::new(),
            },
            None => Action::Declined,
        },
        // GECK *UseWeapon <weapon> <where to stand> <target> <shots> [crouch] [hold fire] [always hit]
        // [no damage] [target place]*: "has an actor use its weapon".
        0x119c => match (caller(world, ctx), form_arg(args, 0), form_arg(args, 2)) {
            (Some((_, actor)), Some(weapon), Some(target)) => {
                if let Some(mut requests) = world.query_mut::<UseWeaponRequest>() {
                    requests.insert(actor, UseWeaponRequest { weapon, target, shots: num(args, 3).max(1.0) as u32 });
                }
                done(format!("UseWeapon {weapon:08X} on {target:08X}"))
            }
            _ => Action::Declined,
        },

        // ── presentation ───────────────────────────────────────────────────────────────────────────
        // GECK *PlayMusic <music type>*: plays the track "and will not stop the music of the cell".
        0x11ff => match args.first() {
            Some(ObScriptValue::Ref(form)) if *form != 0 => {
                let Some(track) = world.try_resource::<MusicTracks>().and_then(|tracks| tracks.by_form(*form).cloned()) else { return Action::Declined };
                if let Some(mut pending) = world.try_resource_mut::<PendingMusic>() {
                    pending.0.push(track);
                }
                done(format!("PlayMusic {form:08X}"))
            }
            _ => Action::Declined,
        },
        // `SwapTexture <ref> "<slot>" "<texture>"`: the end-slide screens' picture.
        0x1269 => {
            let strings = texts(world);
            match (form_arg(args, 0).and_then(|form| live(world, form)), strings.as_slice()) {
                (Some(target), [slot, texture]) => {
                    if let Some(mut pending) = world.try_resource_mut::<PendingTextureSwaps>() {
                        pending.0.push(TextureSwap { target, slot: slot.clone(), texture: texture.clone() });
                    }
                    done(format!("SwapTexture {slot} {texture}"))
                }
                _ => Action::Declined,
            }
        }
        // CS *EssentialDeathReload "message"*: "force reload of the game (same as when the player dies). The
        // message will appear first, then the reload menu."
        0x114d => {
            let message = texts(world).into_iter().next().unwrap_or_default();
            crate::obscript_state::show_warning(world, &message);
            if let Some(mut reload) = world.try_resource_mut::<ReloadRequest>() {
                reload.0 = Some(message);
            }
            done("EssentialDeathReload")
        }

        // ── placement ──────────────────────────────────────────────────────────────────────────────
        // GECK *AttachAshPile [type]*: "attaches an ash pile to the calling reference" — 1 the normal one,
        // 2 the plasma goo pile.
        0x1211 => match caller(world, ctx) {
            Some((at, _)) => {
                let piles = world.try_resource::<AshPiles>().map(|piles| *piles).unwrap_or_default();
                let base = if num(args, 0) as i32 == 2 { piles.goo } else { piles.normal };
                if base == 0 {
                    return Action::Declined;
                }
                match place_at_reference(world, base, at) {
                    Some(form) => Action::Done { value: Some(ObScriptValue::Ref(form)), effect: format!("AttachAshPile {base:08X} at {at:08X}") },
                    None => Action::Declined,
                }
            }
            None => Action::Declined,
        },
        // GECK *PlaceAtReticle <object> [i1 i2 distance]*: "places the object at the spot the crosshair is
        // pointing at" — the reticle's hit point, asked through the query the app installs.
        0x1263 => match (form_arg(args, 0), super::engine_queries(world)) {
            (Some(base), queries) => {
                let distance = if args.len() > 3 { num(args, 3) } else { 0.0 };
                let (Some(point), Some(place)) = (queries.reticle_point.and_then(|reticle| reticle(world, distance)), queries.placement_at) else {
                    return Action::Declined;
                };
                let Some(placement) = place(world, point) else { return Action::Declined };
                let Some(mut created) = world.try_resource_mut::<crate::CreatedReferences>() else { return Action::Declined };
                let Some(form) = created.scripted_ids().next() else { return Action::Declined };
                created.create_scripted(form, base, crate::PLAYER_REFERENCE, false);
                created.place(form, placement);
                Action::Done { value: Some(ObScriptValue::Ref(form)), effect: format!("PlaceAtReticle {base:08X}") }
            }
            _ => Action::Declined,
        },
        // GECK *DisableAllMines*: "disables all the mines located in the cell the player is in" — every
        // placed mine (a weapon of the mine animation types) the player's cell holds.
        0x122f => {
            let mines = mines_in_player_cell(world);
            for &mine in &mines {
                crate::obscript_actions::run(world, 0x1022, &[], &Context { caller: Some(mine), ..*ctx });
            }
            done(format!("DisableAllMines {}", mines.len()))
        }
        // CS *TrapUpdate*: "special function which allows traps to deal damage ... everytime it's called it
        // checks whether any actor is in contact with the trap and takes care of the damage", by the
        // variables the trap's script declares (the VM hands them over): `fTrapDamage` each time the trap
        // affects an actor, `fTrapPushBack` the push, `fTrapMinVelocity` the speed the trap must have
        // relative to the actor, `bTrapContinuous` (0 only on first contact, 1 every call while in contact),
        // `fLevelledDamage` extra per level of the actor.
        0x110c => trap_update(world, args, ctx),
        _ => super::tail::run(world, fallout, cmd, args, ctx),
    }
}

fn trap_update(world: &World, args: &[ObScriptValue], ctx: &Context) -> Action {
    let [damage, push, _death_push, min_speed, continuous, levelled] = [0, 1, 2, 3, 4, 5].map(|i| num(args, i));
    let Some((trap_form, trap)) = caller(world, ctx) else { return Action::Declined };
    let Some(contacts) = super::engine_queries(world).trap_contacts else { return Action::Declined };
    let now = contacts(world, trap);
    let before = world.try_resource::<super::ObTrapContacts>().and_then(|seen| seen.0.get(&trap_form).cloned()).unwrap_or_default();
    let mut touching = std::collections::BTreeSet::new();
    for contact in &now {
        touching.insert(contact.actor);
        // "Minimum velocity a trap must be moving at relative to the actor to do damage."
        if contact.relative_speed < min_speed {
            continue;
        }
        if continuous == 0.0 && before.contains(&contact.actor) {
            continue;
        }
        let level = world.get::<byroredux_core::character::CharacterLevel>(contact.actor).map_or(0.0, |level| f32::from(level.level));
        let total = damage + levelled * level;
        if total > 0.0 {
            if let Some(mut pending) = world.try_resource_mut::<crate::PendingTrapHits>() {
                pending.0.push(crate::TrapHit { target: contact.actor, trap, damage: total });
            }
        }
        if push > 0.0 {
            if let Some(mut requests) = world.query_mut::<super::PushActorRequest>() {
                requests.insert(contact.actor, super::PushActorRequest { from: trap, force: push });
            }
        }
    }
    if let Some(mut seen) = world.try_resource_mut::<super::ObTrapContacts>() {
        seen.0.insert(trap_form, touching);
    }
    Action::Done { value: None, effect: String::new() }
}

/// Create a reference of `base` at the reference `at`.
fn place_at_reference(world: &World, base: u32, at: u32) -> Option<u32> {
    let mut created = world.try_resource_mut::<crate::CreatedReferences>()?;
    let form = created.scripted_ids().next()?;
    created.create_scripted(form, base, at, false);
    Some(form)
}

/// The references of placed mines in the player's cell.
fn mines_in_player_cell(world: &World) -> Vec<u32> {
    use byroredux_core::ecs::components::CellRoot;
    let Some(player) = crate::papyrus_demo::PapyrusPlayerEntity::get(world) else { return Vec::new() };
    let Some(cell) = world.get::<CellRoot>(player).map(|root| root.0) else { return Vec::new() };
    let Some(types) = world.try_resource::<crate::obscript_state::WeaponAnimTypes>() else { return Vec::new() };
    let Some(candidates) = world.query::<crate::scene::SceneAliasCandidate>() else { return Vec::new() };
    let Some(roots) = world.query::<CellRoot>() else { return Vec::new() };
    candidates
        .iter()
        // 10 OneHandMine, 11 OneHandLunchboxMine (the weapons' animation types, GECK *GetWeaponAnimType*).
        .filter(|(entity, candidate)| {
            roots.get(*entity).is_some_and(|root| root.0 == cell) && types.get(candidate.base_form_id).is_some_and(|kind| kind == 11 || kind == 12)
        })
        .filter_map(|(entity, _)| crate::obscript_state::reference_form(world, entity))
        .collect()
}
