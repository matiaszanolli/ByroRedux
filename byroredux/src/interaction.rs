//! Player input actions and the canonical world-interaction pipeline.
//!
//! `InputState` remains the platform-facing physical-key snapshot. This
//! module translates it into stable gameplay actions with per-frame edges,
//! chooses one camera-forward interaction target, and emits the same
//! `ActivateEvent` that script/package-driven activation already uses.

use std::collections::HashMap;

use byroredux_core::ecs::{
    ActiveCamera, EntityId, GlobalTransform, Resource, Transform, World, WorldBound,
};
use byroredux_core::math::Vec3;
use winit::keyboard::KeyCode;

use crate::components::{DoorTeleport, InputState};

/// Maximum camera-forward activation reach in Bethesda units.
///
/// 192 BU matches the familiar Creation-era default interaction reach and
/// keeps the reticle from selecting objects through an entire room.
pub(crate) const INTERACTION_REACH_BU: f32 = 192.0;
const FALLBACK_INTERACTION_RADIUS_BU: f32 = 24.0;

/// Stable gameplay intents, independent of their current physical bindings.
///
/// Only [`Self::Activate`] has a gameplay consumer in this first playable
/// slice. The remaining actions establish the binding/state seam that
/// movement, combat, inventory, and pause migrate onto incrementally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[allow(dead_code)] // Mouse/gamepad sources for these declared actions land next.
pub(crate) enum InputAction {
    MoveForward,
    MoveBackward,
    StrafeLeft,
    StrafeRight,
    Jump,
    Sprint,
    Activate,
    Attack,
    Block,
    Inventory,
    Pause,
}

impl InputAction {
    const fn bit(self) -> u16 {
        1_u16 << self as u8
    }
}

/// Runtime-remappable keyboard bindings.
///
/// Mouse/gamepad sources will join this resource when their physical state is
/// promoted into `InputState`; action consumers do not need to change.
#[derive(Debug, Clone)]
pub(crate) struct ActionBindings {
    keyboard: HashMap<KeyCode, InputAction>,
}

impl Resource for ActionBindings {}

impl Default for ActionBindings {
    fn default() -> Self {
        Self {
            keyboard: HashMap::from([
                (KeyCode::KeyW, InputAction::MoveForward),
                (KeyCode::KeyS, InputAction::MoveBackward),
                (KeyCode::KeyA, InputAction::StrafeLeft),
                (KeyCode::KeyD, InputAction::StrafeRight),
                (KeyCode::Space, InputAction::Jump),
                (KeyCode::ControlLeft, InputAction::Sprint),
                (KeyCode::KeyE, InputAction::Activate),
                (KeyCode::Tab, InputAction::Inventory),
            ]),
        }
    }
}

impl ActionBindings {
    /// Replace the action produced by a physical keyboard key.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn bind_key(&mut self, key: KeyCode, action: InputAction) {
        self.keyboard.insert(key, action);
    }

    fn held_mask(&self, keys_held: &std::collections::HashSet<KeyCode>) -> u16 {
        keys_held
            .iter()
            .filter_map(|key| self.keyboard.get(key))
            .fold(0, |mask, action| mask | action.bit())
    }
}

/// Derived per-frame action state with held/pressed/released semantics.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ActionState {
    held: u16,
    pressed: u16,
    released: u16,
}

impl Resource for ActionState {}

impl ActionState {
    fn refresh(&mut self, next_held: u16) {
        self.pressed = next_held & !self.held;
        self.released = self.held & !next_held;
        self.held = next_held;
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_held(&self, action: InputAction) -> bool {
        self.held & action.bit() != 0
    }

    pub(crate) fn was_pressed(&self, action: InputAction) -> bool {
        self.pressed & action.bit() != 0
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn was_released(&self, action: InputAction) -> bool {
        self.released & action.bit() != 0
    }
}

/// Presentation/behavior category for the currently selected reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InteractionKind {
    Activate,
    Door,
}

impl InteractionKind {
    pub(crate) const fn verb(self) -> &'static str {
        match self {
            Self::Activate => "Activate",
            Self::Door => "Open",
        }
    }
}

/// The single reference selected by the camera-forward interaction query.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct InteractionTarget {
    pub(crate) entity: EntityId,
    pub(crate) kind: InteractionKind,
    pub(crate) distance: f32,
}

/// Derived interaction state read by the HUD and action consumer.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct InteractionState {
    pub(crate) target: Option<InteractionTarget>,
}

impl Resource for InteractionState {}

impl InteractionState {
    pub(crate) fn prompt(&self) -> Option<String> {
        self.target
            .map(|target| format!("[E] {}", target.kind.verb()))
    }
}

/// First gameplay-facing interaction slice.
///
/// Runs first in `Stage::Update`, before every `OnActivate` consumer. A fresh
/// E press therefore emits an event and lets scripts observe it in the same
/// frame; end-of-frame event cleanup remains unchanged.
pub(crate) fn interaction_system(world: &World, _dt: f32) {
    let activate_pressed = refresh_action_state(world);
    let target = select_interaction_target(world);

    if let Some(mut state) = world.try_resource_mut::<InteractionState>() {
        state.target = target;
    }

    if activate_pressed {
        if let Some(target) = target {
            activate_target(world, target);
        }
    }
}

fn refresh_action_state(world: &World) -> bool {
    let Some(input) = world.try_resource::<InputState>() else {
        return false;
    };
    let keys_held = input.keys_held.clone();
    drop(input);
    let Some(bindings) = world.try_resource::<ActionBindings>() else {
        return false;
    };
    let next_held = bindings.held_mask(&keys_held);
    drop(bindings);

    let Some(mut state) = world.try_resource_mut::<ActionState>() else {
        return false;
    };
    state.refresh(next_held);
    state.was_pressed(InputAction::Activate)
}

fn select_interaction_target(world: &World) -> Option<InteractionTarget> {
    let (origin, direction) = camera_ray(world)?;
    let candidates = collect_candidates(world);

    candidates
        .into_iter()
        .filter(|(entity, _)| !activation_is_blocked(world, *entity))
        .filter_map(|(entity, kind)| {
            let bound = interaction_bound(world, entity)?;
            let distance = ray_sphere_distance(origin, direction, bound)?;
            (distance <= INTERACTION_REACH_BU).then_some(InteractionTarget {
                entity,
                kind,
                distance,
            })
        })
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
}

fn camera_ray(world: &World) -> Option<(Vec3, Vec3)> {
    let camera = world.try_resource::<ActiveCamera>()?.0;
    let pose = world
        .get::<Transform>(camera)
        .map(|transform| (transform.translation, transform.rotation))
        .or_else(|| {
            world
                .get::<GlobalTransform>(camera)
                .map(|transform| (transform.translation, transform.rotation))
        })?;
    let direction = (pose.1 * Vec3::NEG_Z).normalize_or_zero();
    (direction.length_squared() > 0.0).then_some((pose.0, direction))
}

fn collect_candidates(world: &World) -> Vec<(EntityId, InteractionKind)> {
    let mut candidates = HashMap::<EntityId, InteractionKind>::new();

    if let Some(query) = world.query::<DoorTeleport>() {
        candidates.extend(
            query
                .iter()
                .map(|(entity, _)| (entity, InteractionKind::Door)),
        );
    }
    if let Some(query) = world.query::<byroredux_scripting::papyrus_demo::RumbleOnActivate>() {
        for (entity, script) in query.iter() {
            if matches!(
                script.state,
                byroredux_scripting::papyrus_demo::RumbleState::Active
            ) {
                candidates
                    .entry(entity)
                    .or_insert(InteractionKind::Activate);
            }
        }
    }
    if let Some(query) =
        world.query::<byroredux_scripting::papyrus_demo::quest_advance::QuestAdvanceOnActivate>()
    {
        for (entity, _) in query.iter() {
            candidates
                .entry(entity)
                .or_insert(InteractionKind::Activate);
        }
    }
    if let Some(query) = world.query::<byroredux_scripting::TwoStateActivator>() {
        for (entity, state) in query.iter() {
            if !(state.is_animating || state.do_once && state.activated_once) {
                candidates
                    .entry(entity)
                    .or_insert(InteractionKind::Activate);
            }
        }
    }
    if let Some(query) =
        world.query::<byroredux_scripting::papyrus_demo::mg07_door::MG07LabyrinthianDoor>()
    {
        for (entity, door) in query.iter() {
            if !door.disabled && !door.activation_blocked {
                candidates
                    .entry(entity)
                    .or_insert(InteractionKind::Activate);
            }
        }
    }

    candidates.into_iter().collect()
}

fn activation_is_blocked(world: &World, entity: EntityId) -> bool {
    world
        .get::<byroredux_scripting::papyrus_demo::mg07_door::MG07LabyrinthianDoor>(entity)
        .is_some_and(|door| door.disabled || door.activation_blocked)
}

fn interaction_bound(world: &World, entity: EntityId) -> Option<WorldBound> {
    if let Some(bound) = world.get::<WorldBound>(entity).map(|bound| *bound) {
        if bound.radius > 0.0 {
            return Some(bound);
        }
    }

    world
        .get::<GlobalTransform>(entity)
        .map(|transform| WorldBound::new(transform.translation, FALLBACK_INTERACTION_RADIUS_BU))
        .or_else(|| {
            world.get::<Transform>(entity).map(|transform| {
                WorldBound::new(transform.translation, FALLBACK_INTERACTION_RADIUS_BU)
            })
        })
}

fn ray_sphere_distance(origin: Vec3, direction: Vec3, bound: WorldBound) -> Option<f32> {
    let from_center = origin - bound.center;
    let projection = from_center.dot(direction);
    let discriminant =
        projection * projection - (from_center.length_squared() - bound.radius * bound.radius);
    if discriminant < 0.0 {
        return None;
    }

    let root = discriminant.sqrt();
    let near = -projection - root;
    let far = -projection + root;
    if far < 0.0 {
        None
    } else {
        Some(near.max(0.0))
    }
}

fn activate_target(world: &World, target: InteractionTarget) {
    if let Err(error) = emit_activate_event(world, target.entity) {
        log::error!("interaction: {error}");
    }

    if target.kind == InteractionKind::Door {
        match crate::cell_loader::queue_door_transition(world, target.entity) {
            Ok(queued) => log::info!(
                "interaction: entity {} activated; queued {}",
                target.entity,
                queued.destination_label
            ),
            Err(error) => log::warn!(
                "interaction: entity {} activated, but its door transition was not queued: {}",
                target.entity,
                error
            ),
        }
    }
}

/// Emit the engine-canonical activation marker and return the resolved
/// activator entity. Normal input and diagnostic commands both use this path.
pub(crate) fn emit_activate_event(
    world: &World,
    target: EntityId,
) -> Result<EntityId, &'static str> {
    let activator = world
        .try_resource::<byroredux_scripting::papyrus_demo::PlayerEntity>()
        .map(|player| player.0)
        .unwrap_or(0);

    let mut events = world
        .query_mut::<byroredux_scripting::ActivateEvent>()
        .ok_or("ActivateEvent storage is not registered")?;
    events.insert(target, byroredux_scripting::ActivateEvent { activator });
    Ok(activator)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::math::Quat;

    fn input_fixture() -> World {
        let mut world = World::new();
        world.insert_resource(InputState::default());
        world.insert_resource(ActionBindings::default());
        world.insert_resource(ActionState::default());
        world.insert_resource(InteractionState::default());
        world
    }

    #[test]
    fn action_state_emits_edges_once_while_key_is_held() {
        let world = input_fixture();
        world
            .resource_mut::<InputState>()
            .keys_held
            .insert(KeyCode::KeyE);

        assert!(refresh_action_state(&world));
        assert!(world
            .resource::<ActionState>()
            .is_held(InputAction::Activate));
        assert!(
            !refresh_action_state(&world),
            "held key must not auto-repeat"
        );

        world
            .resource_mut::<InputState>()
            .keys_held
            .remove(&KeyCode::KeyE);
        assert!(!refresh_action_state(&world));
        assert!(world
            .resource::<ActionState>()
            .was_released(InputAction::Activate));
    }

    #[test]
    fn bindings_can_remap_activate_without_changing_consumers() {
        let world = input_fixture();
        world
            .resource_mut::<ActionBindings>()
            .bind_key(KeyCode::KeyR, InputAction::Activate);
        world
            .resource_mut::<InputState>()
            .keys_held
            .insert(KeyCode::KeyR);

        assert!(refresh_action_state(&world));
    }

    #[test]
    fn ray_sphere_rejects_behind_and_returns_near_surface_distance() {
        let forward = WorldBound::new(Vec3::new(0.0, 0.0, -100.0), 10.0);
        assert_eq!(
            ray_sphere_distance(Vec3::ZERO, Vec3::NEG_Z, forward),
            Some(90.0)
        );
        let behind = WorldBound::new(Vec3::new(0.0, 0.0, 100.0), 10.0);
        assert_eq!(ray_sphere_distance(Vec3::ZERO, Vec3::NEG_Z, behind), None);
    }

    #[test]
    fn interaction_selects_nearest_door_and_emits_activate_event() {
        let mut world = input_fixture();
        world.register::<byroredux_scripting::ActivateEvent>();

        let camera = world.spawn();
        world.insert(camera, Transform::IDENTITY);
        world.insert_resource(ActiveCamera(camera));
        world.insert_resource(byroredux_scripting::papyrus_demo::PlayerEntity(camera));

        let far = spawn_test_door(&mut world, Vec3::new(0.0, 0.0, -150.0));
        let near = spawn_test_door(&mut world, Vec3::new(0.0, 0.0, -80.0));
        world
            .resource_mut::<InputState>()
            .keys_held
            .insert(KeyCode::KeyE);

        interaction_system(&world, 0.0);

        let selected = world.resource::<InteractionState>().target.unwrap();
        assert_eq!(selected.entity, near);
        assert_eq!(selected.kind, InteractionKind::Door);
        let events = world.query::<byroredux_scripting::ActivateEvent>().unwrap();
        assert!(events.get(near).is_some());
        assert!(events.get(far).is_none());
    }

    fn spawn_test_door(world: &mut World, center: Vec3) -> EntityId {
        let entity = world.spawn();
        world.insert(entity, Transform::new(center, Quat::IDENTITY, 1.0));
        world.insert(entity, WorldBound::new(center, 10.0));
        world.insert(
            entity,
            DoorTeleport {
                destination_form_id: 0x1234,
                position_zup: [0.0; 3],
                rotation_zup: [0.0; 3],
            },
        );
        entity
    }
}
