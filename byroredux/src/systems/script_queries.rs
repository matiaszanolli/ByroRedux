//! M47.3 — the engine state ObScript commands ask about that the scripting crate cannot read itself
//! (physics, the camera, the UI): line of sight, would-attack, camera mode, open menus, the point under
//! the crosshair, and the actors touching a trap. Installed at boot as
//! [`byroredux_scripting::EngineQueries`]; a command whose query is absent declines.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::{EntityId, TotalTime, World};
use byroredux_core::math::Vec3;
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
    /// The developer console / overlay.
    pub console: bool,
    /// The loading cover.
    pub loading: bool,
}

impl Resource for UiMenuState {}

/// Read the open pages off the UI and publish them.
pub(crate) fn publish_ui_menus(world: &World, ui: Option<&byroredux_debug_ui::DebugUiState>, loading: bool) {
    let Some(ui) = ui else { return };
    let inventory = ui.inventory_menu_visible();
    let dialogue = ui.dialogue_menu_visible();
    let message_box = ui.message_box_menu_visible();
    let barter = ui.barter_menu_visible();
    let state = UiMenuState {
        pause: ui.game_menu_visible() && !(inventory || dialogue || message_box || barter),
        inventory,
        dialogue,
        message_box,
        barter,
        console: ui.visible,
        loading,
    };
    if let Some(mut published) = world.try_resource_mut::<UiMenuState>() {
        *published = state;
    }
}

/// GECK *MenuMode* codes (page *MenuTypes*): 1 the "main four" (inventory here), 2 any other menu, 3 the
/// console; and the specific menus 1001 Message, 1002 Inventory, 1007 Loading, 1009 Dialog, 1013 Pause,
/// 1053 Barter. `0` asks about any menu. A menu the engine has no screen for is never open.
fn menu_open(world: &World, kind: u32) -> bool {
    let ui = world.try_resource::<UiMenuState>().map(|state| *state).unwrap_or_default();
    let message = ui.message_box || world.try_resource::<crate::notifications::MessageBoxQueue>().is_some_and(|boxes| !boxes.0.is_empty());
    let barter = ui.barter || world.try_resource::<crate::barter::ActiveBarter>().is_some_and(|barter| barter.is_open());
    let loading = ui.loading;
    let other = message || barter || loading || ui.dialogue || ui.pause;
    match kind {
        0 => other || ui.inventory || ui.console,
        1 | 1002 => ui.inventory,
        2 => other,
        3 => ui.console,
        1001 => message,
        1007 => loading,
        1009 => ui.dialogue,
        1013 => ui.pause,
        1053 => barter,
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
