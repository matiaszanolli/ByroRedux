//! Player input actions and the canonical world-interaction pipeline.
//!
//! `InputState` remains the platform-facing physical-key snapshot. This
//! module translates it into stable gameplay actions with per-frame edges,
//! chooses one camera-forward interaction target, and emits the same
//! `ActivateEvent` that script/package-driven activation already uses.

use std::collections::HashMap;

use byroredux_core::ecs::components::{FormIdComponent, PhysicsSourceForm};
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
const OCCLUSION_EPSILON_BU: f32 = 1.0;

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

    fn action_for_key(&self, key: KeyCode) -> Option<InputAction> {
        self.keyboard.get(&key).copied()
    }
}

/// One-frame physical-key pulse used by real-data smoke automation.
///
/// This is deliberately upstream of [`ActionState`]: `input.press activate`
/// exercises the same E-key binding and edge detector as the window event
/// path, while releasing automatically on the following frame.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct InjectedKeyPulse {
    key: Option<KeyCode>,
}

impl Resource for InjectedKeyPulse {}

pub(crate) fn queue_debug_activate_press(world: &World) -> Result<(), &'static str> {
    let mut pulse = world
        .try_resource_mut::<InjectedKeyPulse>()
        .ok_or("InjectedKeyPulse resource is not installed")?;
    pulse.key = Some(KeyCode::KeyE);
    Ok(())
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

/// Last canonical activation retained past transient-event cleanup.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InteractionTraceEntry {
    pub(crate) target: InteractionTarget,
    pub(crate) activator: Option<EntityId>,
    pub(crate) event_emitted: bool,
    pub(crate) outcome: String,
}

/// Lightweight runtime evidence for smoke tests and operator diagnostics.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct InteractionTrace {
    pub(crate) activation_count: u64,
    pub(crate) last: Option<InteractionTraceEntry>,
}

impl Resource for InteractionTrace {}

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
    let injected_key = world
        .try_resource_mut::<InjectedKeyPulse>()
        .and_then(|mut pulse| pulse.key.take());
    let Some(bindings) = world.try_resource::<ActionBindings>() else {
        return false;
    };
    let mut next_held = bindings.held_mask(&keys_held);
    if let Some(action) = injected_key.and_then(|key| bindings.action_for_key(key)) {
        next_held |= action.bit();
    }
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

    let mut targets: Vec<_> = candidates
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
        .collect();
    targets.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    targets
        .into_iter()
        .find(|target| target_has_line_of_sight(world, *target, origin, direction))
}

fn target_has_line_of_sight(
    world: &World,
    target: InteractionTarget,
    origin: Vec3,
    direction: Vec3,
) -> bool {
    let Some(_) = world.try_resource::<byroredux_physics::PhysicsWorld>() else {
        return true;
    };

    // Resolve the player exclusion and body→ECS ownership before acquiring
    // PhysicsWorld, keeping the resource/component lock order non-overlapping.
    let player = world
        .try_resource::<byroredux_scripting::papyrus_demo::PlayerEntity>()
        .map(|player| player.0);
    let (excluded_body, owners) = match world.query::<byroredux_physics::RapierHandles>() {
        Some(handles) => {
            let excluded = player.and_then(|entity| handles.get(entity).map(|h| h.body));
            let owners = handles
                .iter()
                .map(|(entity, handles)| (entity, handles.body))
                .collect::<Vec<_>>();
            (excluded, owners)
        }
        None => (None, Vec::new()),
    };

    let hit = {
        let physics = world.resource::<byroredux_physics::PhysicsWorld>();
        physics.cast_ray(
            origin,
            direction,
            target.distance + OCCLUSION_EPSILON_BU,
            excluded_body,
        )
    };
    let Some(hit) = hit else {
        return true;
    };
    let Some(hit_body) = hit.body else {
        return false;
    };
    let Some(hit_owner) = owners
        .iter()
        .find_map(|(entity, body)| (*body == hit_body).then_some(*entity))
    else {
        return false;
    };

    collider_belongs_to_target(world, hit_owner, target.entity)
}

fn collider_belongs_to_target(world: &World, collider_entity: EntityId, target: EntityId) -> bool {
    if collider_entity == target {
        return true;
    }
    let target_form = world.get::<FormIdComponent>(target).map(|form| form.0);
    let collider_form = world
        .get::<FormIdComponent>(collider_entity)
        .map(|form| form.0)
        .or_else(|| {
            world
                .get::<PhysicsSourceForm>(collider_entity)
                .map(|form| form.0)
        });
    target_form.is_some() && target_form == collider_form
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
    let event = emit_activate_event(world, target.entity);
    let (activator, event_emitted, event_outcome) = match event {
        Ok(activator) => (Some(activator), true, "ActivateEvent emitted".to_string()),
        Err(error) => {
            log::error!("interaction: {error}");
            (None, false, format!("ActivateEvent failed: {error}"))
        }
    };

    let outcome = if target.kind == InteractionKind::Door {
        match crate::cell_loader::queue_door_transition(world, target.entity) {
            Ok(queued) => {
                log::info!(
                    "interaction: entity {} activated; queued {}",
                    target.entity,
                    queued.destination_label
                );
                format!("{event_outcome}; queued {}", queued.destination_label)
            }
            Err(error) => {
                log::warn!(
                    "interaction: entity {} activated, but its door transition was not queued: {}",
                    target.entity,
                    error
                );
                format!("{event_outcome}; door queue failed: {error}")
            }
        }
    } else {
        event_outcome
    };

    if let Some(mut trace) = world.try_resource_mut::<InteractionTrace>() {
        trace.activation_count = trace.activation_count.saturating_add(1);
        trace.last = Some(InteractionTraceEntry {
            target,
            activator,
            event_emitted,
            outcome,
        });
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
    use byroredux_core::ecs::components::{CollisionShape, RigidBodyData};
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_core::math::Quat;

    fn input_fixture() -> World {
        let mut world = World::new();
        world.insert_resource(InputState::default());
        world.insert_resource(ActionBindings::default());
        world.insert_resource(ActionState::default());
        world.insert_resource(InjectedKeyPulse::default());
        world.insert_resource(InteractionState::default());
        world.insert_resource(InteractionTrace::default());
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
    fn injected_e_key_pulse_uses_binding_and_releases_next_frame() {
        let world = input_fixture();
        queue_debug_activate_press(&world).unwrap();

        assert!(refresh_action_state(&world));
        assert!(world
            .resource::<ActionState>()
            .is_held(InputAction::Activate));
        assert!(!refresh_action_state(&world));
        assert!(world
            .resource::<ActionState>()
            .was_released(InputAction::Activate));
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
        let trace = world.resource::<InteractionTrace>();
        assert_eq!(trace.activation_count, 1);
        assert!(trace.last.as_ref().unwrap().event_emitted);
    }

    #[test]
    fn solid_collider_between_camera_and_door_blocks_selection() {
        let mut world = physics_fixture();
        spawn_camera(&mut world);
        spawn_test_door(&mut world, Vec3::new(0.0, 0.0, -100.0));
        spawn_static_collider(&mut world, Vec3::new(0.0, 0.0, -50.0), None);
        byroredux_physics::physics_sync_system(&world, 0.0);

        assert_eq!(select_interaction_target(&world), None);
    }

    #[test]
    fn physics_source_form_identifies_the_doors_own_collider() {
        let mut world = physics_fixture();
        spawn_camera(&mut world);
        let door = spawn_test_door(&mut world, Vec3::new(0.0, 0.0, -100.0));
        let form_id = world.resource_mut::<FormIdPool>().intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(0x1234),
        });
        world.insert(door, FormIdComponent(form_id));
        spawn_static_collider(&mut world, Vec3::new(0.0, 0.0, -80.0), Some(form_id));
        byroredux_physics::physics_sync_system(&world, 0.0);

        assert_eq!(select_interaction_target(&world).unwrap().entity, door);
    }

    fn physics_fixture() -> World {
        let mut world = input_fixture();
        world.insert_resource(FormIdPool::new());
        world.insert_resource(byroredux_physics::PhysicsWorld::new());
        world.register::<byroredux_physics::RapierHandles>();
        world
    }

    fn spawn_camera(world: &mut World) -> EntityId {
        let camera = world.spawn();
        world.insert(camera, Transform::IDENTITY);
        world.insert_resource(ActiveCamera(camera));
        world.insert_resource(byroredux_scripting::papyrus_demo::PlayerEntity(camera));
        camera
    }

    fn spawn_static_collider(
        world: &mut World,
        center: Vec3,
        source_form: Option<byroredux_core::form_id::FormId>,
    ) -> EntityId {
        let entity = world.spawn();
        world.insert(entity, Transform::new(center, Quat::IDENTITY, 1.0));
        world.insert(entity, GlobalTransform::new(center, Quat::IDENTITY, 1.0));
        world.insert(
            entity,
            CollisionShape::Cuboid {
                half_extents: Vec3::splat(5.0),
            },
        );
        world.insert(entity, RigidBodyData::STATIC);
        if let Some(form_id) = source_form {
            world.insert(entity, PhysicsSourceForm(form_id));
        }
        entity
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
