//! M47.3 — the engine state ObScript commands ask about that the scripting crate cannot read itself
//! (physics, the camera, the UI): line of sight, would-attack, camera mode, open menus, the point under
//! the crosshair, and the actors touching a trap. Installed at boot as
//! [`byroredux_scripting::EngineQueries`]; a command whose query is absent declines.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::{EntityId, TotalTime, World};
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::EsmIndex;
use byroredux_scripting::obscript_more::{EngineQueries, TrapContact};

/// What the pause/UI layer has open this frame, published by the app each frame. `NOT_SAVED_BY_DESIGN`:
/// presentation state, republished every frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiMenuState {
    /// The pause menu (or a settings page of it).
    pub pause: bool,
    pub inventory: bool,
    pub dialogue: bool,
    pub message_box: bool,
    pub barter: bool,
    /// The crafting page (a workbench, grindstone, …). The engine has no menu kind for it, so it counts
    /// only toward "some menu is open" — never as the Pause menu (#6576).
    pub crafting: bool,
    /// The developer console / overlay.
    pub console: bool,
    /// The loading cover.
    pub loading: bool,
    /// The menu code of the character-creation screen on the native page (`chargen.rs`): RaceSex
    /// 1036, TextEdit 1051, Love Tester 1074, CharGen 1048 or the traits menu 1084.
    pub chargen: Option<u32>,
}

impl Resource for UiMenuState {}

/// The page flags for a game menu: `pages` is `[inventory, dialogue, message box, barter, crafting]`, at most
/// one of which is the open page. The menu being up on none of them is the Pause menu (or a settings page of
/// it). `console` and `loading` are not pages and are left clear for the caller.
fn page_state(game_menu_visible: bool, pages: [bool; 5]) -> UiMenuState {
    let [inventory, dialogue, message_box, barter, crafting] = pages;
    UiMenuState {
        pause: game_menu_visible && !pages.contains(&true),
        inventory,
        dialogue,
        message_box,
        barter,
        crafting,
        ..Default::default()
    }
}

/// Read the open pages off the UI and publish them.
pub(crate) fn publish_ui_menus(world: &World, ui: Option<&byroredux_debug_ui::DebugUiState>, loading: bool) {
    let Some(ui) = ui else { return };
    let chargen = ui.chargen_menu_visible().then(|| crate::chargen::open_menu_code(world)).flatten();
    let mut state = UiMenuState {
        console: ui.visible,
        loading,
        chargen,
        ..page_state(
            ui.game_menu_visible(),
            [
                ui.inventory_menu_visible(),
                ui.dialogue_menu_visible(),
                ui.message_box_menu_visible(),
                ui.barter_menu_visible(),
                ui.crafting_menu_visible(),
            ],
        )
    };
    // The character-creation page is its own menu, not the Pause menu.
    state.pause &= chargen.is_none();
    if let Some(mut published) = world.try_resource_mut::<UiMenuState>() {
        *published = state;
    }
}

/// The specific-menu codes that differ by game, selected once when the load order is installed (#6620). Only
/// the barter screen today; the full per-game table belongs to `docs/prd/game-ui-integration.md`, GUI-REG-002.
/// `NOT_SAVED_BY_DESIGN`: re-derived from the load order every boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MenuCodes {
    barter: u32,
}

impl Default for MenuCodes {
    fn default() -> Self {
        Self::FALLOUT
    }
}

impl Resource for MenuCodes {}

impl MenuCodes {
    /// xEdit `wbMenuModeEnum` (`wbDefinitionsCommon.pas`) 1008 `IsTES4('Container/Barter', 'Container')`, and CS
    /// *MenuTypes* "1008 = Container, Barter".
    const OBLIVION: Self = Self { barter: 1008 };
    /// xEdit `wbMenuModeEnum` 1053 `IsFO3('Barter', '')`.
    const FALLOUT: Self = Self { barter: 1053 };

    fn for_game(game: GameKind) -> Self {
        match game {
            GameKind::Oblivion => Self::OBLIVION,
            _ => Self::FALLOUT,
        }
    }
}

/// Select the load order's menu codes.
pub(crate) fn install_menu_codes(world: &mut World, index: &EsmIndex) {
    world.insert_resource(MenuCodes::for_game(index.game));
}

/// *MenuMode* codes (GECK and CS page *MenuTypes*): 1 the "main four" (inventory here), 2 any other menu, 3 the
/// console; and the specific menus 1001 Message, 1002 Inventory, 1007 Loading, 1009 Dialog, 1013 Pause, plus
/// the game's barter code ([`MenuCodes`]). `0` asks about any menu. A menu the engine has no screen for is never
/// open.
fn menu_open(world: &World, kind: u32) -> bool {
    let codes = world.try_resource::<MenuCodes>().map(|codes| *codes).unwrap_or_default();
    let ui = world.try_resource::<UiMenuState>().map(|state| *state).unwrap_or_default();
    let message = ui.message_box || world.try_resource::<crate::notifications::MessageBoxQueue>().is_some_and(|boxes| !boxes.0.is_empty());
    let barter = ui.barter || world.try_resource::<crate::barter::ActiveBarter>().is_some_and(|barter| barter.is_open());
    let loading = ui.loading;
    let other = message || barter || loading || ui.dialogue || ui.pause || ui.crafting || ui.chargen.is_some();
    match kind {
        0 => other || ui.inventory || ui.console,
        1 | 1002 => ui.inventory,
        2 => other,
        3 => ui.console,
        1001 => message,
        1007 => loading,
        1009 => ui.dialogue,
        1013 => ui.pause,
        code if code == codes.barter => barter,
        code if ui.chargen == Some(code) => true,
        _ => false,
    }
}

fn first_person(world: &World) -> bool {
    world.try_resource::<crate::player_body::PlayerCameraView>().is_none_or(|view| *view == crate::player_body::PlayerCameraView::FirstPerson)
}

/// The reach `PlaceAtReticle` uses when it is given no distance: the longest one a vanilla script writes
/// (`player.PlaceAtReticle EuclidTarget 1 0 20000`).
const DEFAULT_RETICLE_REACH: f32 = 20_000.0;

/// Where the crosshair ray first meets a solid within `distance` (Z-up world units).
fn reticle_point(world: &World, distance: f32) -> Option<[f32; 3]> {
    let (origin, direction) = crate::interaction::camera_ray(world)?;
    let reach = if distance > 0.0 { distance } else { DEFAULT_RETICLE_REACH };
    let player = byroredux_scripting::papyrus_demo::PapyrusPlayerEntity::get(world);
    let excluded = player.and_then(|player| world.get::<byroredux_physics::RapierHandles>(player).map(|handles| handles.body));
    let hit = world.try_resource::<byroredux_physics::PhysicsWorld>()?.cast_ray(origin, direction, reach, excluded)?;
    Some(byroredux_core::math::coord::yup_to_zup_pos((origin + direction * hit.distance).to_array()))
}

/// Where a trap or an actor was when a `TrapUpdate` last sampled it, and the velocity measured then.
#[derive(Debug, Clone, Copy)]
struct MotionSample {
    position: Vec3,
    at: f32,
    velocity: Vec3,
}

/// Positions seen at the previous `TrapUpdate`s, to measure how fast a trap and an actor move.
///
/// Each entity carries its own sample time (#6236): a velocity is measured only against a sample
/// taken on the previous frame. An actor that left a trap and touches one again later has no such
/// sample, so it starts at rest instead of "moving" from where it last touched over one frame's
/// `dt` (thousands of units a second, which tripped `fTrapMinVelocity` traps). Samples older than
/// a frame are useless and are dropped, so the map holds only what this frame and the last touched.
#[derive(Debug, Default)]
pub(crate) struct TrapMotionSamples {
    samples: HashMap<EntityId, MotionSample>,
}

impl Resource for TrapMotionSamples {}

/// Whether a sample `age` seconds old was taken on the previous frame. The slack covers a frame
/// length that changed between the two and `TotalTime`'s f32 rounding over a long session.
fn from_previous_frame(age: f32, frame_dt: f32) -> bool {
    age > 0.0 && age <= frame_dt * 1.5
}

impl TrapMotionSamples {
    /// Forget every sample neither this frame nor the previous one took.
    fn prune(&mut self, now: f32, frame_dt: f32) {
        self.samples.retain(|_, sample| sample.at == now || from_previous_frame(now - sample.at, frame_dt));
    }

    /// `entity`'s velocity at `now`, sampling it at `position`. A second trap asking in the same
    /// frame gets the velocity the first measured.
    fn velocity(&mut self, entity: EntityId, position: Vec3, now: f32, frame_dt: f32) -> Vec3 {
        let previous = self.samples.get(&entity).copied();
        if let Some(sample) = previous.filter(|sample| sample.at == now) {
            return sample.velocity;
        }
        let velocity = match previous {
            Some(sample) if from_previous_frame(now - sample.at, frame_dt) => (position - sample.position) / (now - sample.at),
            _ => Vec3::ZERO,
        };
        self.samples.insert(entity, MotionSample { position, at: now, velocity });
        velocity
    }
}

/// The speed of `trap` relative to each actor overlapping its body. Velocities come from how far the
/// transforms moved since the previous call (the trap's animation moves its collision with the mesh), so
/// the first call measures nothing.
fn trap_contacts(world: &World, trap: EntityId) -> Vec<TrapContact> {
    let Some(trap_body) = world.get::<byroredux_physics::RapierHandles>(trap).map(|handles| handles.body) else {
        return Vec::new();
    };
    let actors = crate::systems::trap_hits::live_actor_bodies(world);
    let overlapping = match world.try_resource::<byroredux_physics::PhysicsWorld>() {
        Some(physics) => physics.bodies_overlapping_body_where(trap_body, &|body| actors.contains_key(&body)),
        None => return Vec::new(),
    };
    let now = world.try_resource::<TotalTime>().map_or(0.0, |time| time.0);
    let frame_dt = world.try_resource::<byroredux_core::ecs::DeltaTime>().map_or(0.0, |dt| dt.0);
    let position_of = |entity: EntityId| world.get::<byroredux_core::ecs::components::GlobalTransform>(entity).map(|t| t.translation);
    let Some(mut samples) = world.try_resource_mut::<TrapMotionSamples>() else { return Vec::new() };
    samples.prune(now, frame_dt);
    let mut velocity_of = |entity: EntityId| position_of(entity).map_or(Vec3::ZERO, |position| samples.velocity(entity, position, now, frame_dt));
    let trap_velocity = velocity_of(trap);
    overlapping
        .iter()
        .filter_map(|body| actors.get(body).copied())
        .map(|actor| TrapContact { actor, relative_speed: (trap_velocity - velocity_of(actor)).length() })
        .collect()
}

/// Whether the actor stands in water that hurts (an authored damage per second — lava).
fn in_dangerous_water(world: &World, actor: EntityId) -> bool {
    world
        .get::<byroredux_core::ecs::components::WaterContact>(actor)
        .is_some_and(|contact| contact.submerged_fraction > 0.0 && contact.damage_per_second > 0.0)
}

/// Whether the player's character controller is off the ground.
fn player_airborne(world: &World) -> bool {
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return false;
    };
    world.get::<byroredux_physics::CharacterController>(player).is_some_and(|controller| !controller.is_grounded)
}

/// Install the queries and their state.
pub(crate) fn install(world: &mut World) {
    world.init_resource::<UiMenuState>();
    world.init_resource::<MenuCodes>();
    world.init_resource::<TrapMotionSamples>();
    world.insert_resource(EngineQueries {
        line_of_sight: Some(crate::systems::actor_has_line_of_sight),
        should_attack: Some(crate::systems::actor_would_attack),
        first_person: Some(first_person),
        menu_open: Some(menu_open),
        reticle_point: Some(reticle_point),
        placement_at: Some(crate::cell_loader::created::placement_at_position),
        trap_contacts: Some(trap_contacts),
        in_dangerous_water: Some(in_dangerous_water),
        player_airborne: Some(player_airborne),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_codes_read_the_published_pages() {
        let mut world = World::new();
        install(&mut world);
        assert!(!menu_open(&world, 0), "nothing open");
        *world.resource_mut::<UiMenuState>() = UiMenuState { pause: true, ..Default::default() };
        assert!(menu_open(&world, 0) && menu_open(&world, 1013) && menu_open(&world, 2));
        assert!(!menu_open(&world, 1053) && !menu_open(&world, 1), "the pause menu is not the barter page or the main four");
        *world.resource_mut::<UiMenuState>() = UiMenuState { inventory: true, ..Default::default() };
        assert!(menu_open(&world, 1) && menu_open(&world, 1002) && !menu_open(&world, 2));
        *world.resource_mut::<UiMenuState>() = UiMenuState { console: true, ..Default::default() };
        assert!(menu_open(&world, 3) && menu_open(&world, 0));
        assert!(!menu_open(&world, 1080), "a menu with no screen here is never open");
    }

    /// #6620 — the barter screen answers the load order's code and no other: 1008 on Oblivion (where 1053 is
    /// not a code), 1053 on FO3/FNV (where 1008 is the container screen, which has no page here).
    #[test]
    fn the_barter_code_is_selected_by_the_load_order() {
        let world_for = |game| {
            let mut world = World::new();
            install(&mut world);
            install_menu_codes(&mut world, &EsmIndex { game, ..Default::default() });
            *world.resource_mut::<UiMenuState>() = UiMenuState { barter: true, ..Default::default() };
            world
        };
        let oblivion = world_for(GameKind::Oblivion);
        assert!(menu_open(&oblivion, 1008), "Oblivion's barter code");
        assert!(!menu_open(&oblivion, 1053), "not a code on Oblivion");
        assert!(menu_open(&oblivion, 0) && menu_open(&oblivion, 2), "still a menu");
        let fallout = world_for(GameKind::Fallout3NV);
        assert!(menu_open(&fallout, 1053), "FO3/FNV's barter code");
        assert!(!menu_open(&fallout, 1008), "1008 is the container screen on FO3/FNV");
        // With the barter screen shut, its code is shut on both.
        let closed = world_for(GameKind::Oblivion);
        *closed.resource_mut::<UiMenuState>() = UiMenuState::default();
        assert!(!menu_open(&closed, 1008));
    }

    /// #6576 — a workbench is a menu (so "any menu" and "some other menu" are open) but not the Pause menu,
    /// and each page raises exactly its own flag.
    #[test]
    fn the_crafting_page_is_open_but_is_not_the_pause_menu() {
        let mut world = World::new();
        install(&mut world);
        *world.resource_mut::<UiMenuState>() = page_state(true, [false, false, false, false, true]);
        assert!(menu_open(&world, 0) && menu_open(&world, 2), "a bench is a menu");
        assert!(!menu_open(&world, 1013), "…and not the Pause menu");
        assert!(!menu_open(&world, 1) && !menu_open(&world, 1053), "…nor the main four or the barter page");
    }

    /// #6576 — with the game menu up, exactly one page flag is set whichever page it is on: `pause` for none
    /// of the named pages, otherwise that page alone; with the menu down, none.
    #[test]
    fn exactly_one_page_flag_is_set_for_each_open_page() {
        let flags = |state: UiMenuState| {
            [state.pause, state.inventory, state.dialogue, state.message_box, state.barter, state.crafting]
        };
        let lit = |state: UiMenuState| flags(state).iter().filter(|set| **set).count();
        let pause = page_state(true, [false; 5]);
        assert_eq!(flags(pause), [true, false, false, false, false, false], "the landing / settings page");
        for page in 0..5 {
            let mut pages = [false; 5];
            pages[page] = true;
            let state = page_state(true, pages);
            assert_eq!(lit(state), 1, "page {page}: {state:?}");
            assert!(!state.pause, "page {page} is not the Pause menu");
            assert!(flags(state)[page + 1], "page {page} sets its own flag");
        }
        assert_eq!(lit(page_state(false, [false; 5])), 0, "menu down: nothing open");
    }

    /// #6236 — a velocity is measured only against the previous frame's sample: an actor that
    /// re-touches a trap after a gap starts at rest, not at (distance since last touch) / one dt.
    #[test]
    fn trap_motion_samples_measure_only_against_the_previous_frame() {
        const DT: f32 = 1.0 / 60.0;
        let (actor, trap) = (EntityId::from(1u32), EntityId::from(2u32));
        let mut samples = TrapMotionSamples::default();
        let at = |samples: &mut TrapMotionSamples, frame: u32, entity, x: f32| {
            let now = frame as f32 * DT;
            samples.prune(now, DT);
            samples.velocity(entity, Vec3::new(x, 0.0, 0.0), now, DT)
        };
        assert_eq!(at(&mut samples, 1, actor, 0.0), Vec3::ZERO, "the first sample measures nothing");
        let walking = at(&mut samples, 2, actor, 1.0);
        assert!((walking.x - 60.0).abs() < 0.1, "consecutive frames measure the motion: {walking:?}");

        // The actor leaves contact for a second and walks 50 units, then touches a trap again.
        let recontact = at(&mut samples, 62, actor, 51.0);
        assert_eq!(recontact, Vec3::ZERO, "a re-contact is at rest, not 50 units over one frame");

        // A second trap asking in the same frame gets the same velocity, not a zero-dt division.
        let next = at(&mut samples, 63, actor, 52.0);
        assert_eq!(samples.velocity(actor, Vec3::new(52.0, 0.0, 0.0), 63.0 * DT, DT), next);

        // Nothing outlives a frame it was not touched in.
        at(&mut samples, 64, trap, 0.0);
        samples.prune(70.0 * DT, DT);
        assert!(samples.samples.is_empty(), "stale samples are dropped: {:?}", samples.samples.keys().collect::<Vec<_>>());
    }

    #[test]
    fn the_camera_defaults_to_first_person_until_switched() {
        let mut world = World::new();
        assert!(first_person(&world));
        world.insert_resource(crate::player_body::PlayerCameraView::ThirdPerson);
        assert!(!first_person(&world));
    }
}
