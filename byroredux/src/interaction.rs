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
use byroredux_core::settings::{
    SettingChange, SettingChoice, SettingEntry, SettingValue, SettingsError, SettingsRegistry,
};
use rustc_hash::FxHashMap;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

use crate::components::{DoorTeleport, InputState, Locked, DEFAULT_LOOK_SENSITIVITY};

pub(crate) const MOUSE_SENSITIVITY_SETTING_ID: &str = "controls.mouse_sensitivity";
pub(crate) const INVERT_LOOK_Y_SETTING_ID: &str = "controls.invert_y";

/// Maximum camera-forward activation reach in Bethesda units.
///
/// 192 BU matches the familiar Creation-era default interaction reach and
/// keeps the reticle from selecting objects through an entire room.
pub(crate) const INTERACTION_REACH_BU: f32 = 192.0;
const FALLBACK_INTERACTION_RADIUS_BU: f32 = 24.0;
const OCCLUSION_EPSILON_BU: f32 = 1.0;

/// Stable gameplay intents, independent of their current physical bindings.
///
/// Movement, jump, sprint, and activation are the first gameplay consumers.
/// The remaining actions establish the same seam for combat, inventory, and
/// pause to migrate onto incrementally.
///
/// #2732 — the three variants with no producer yet carry their own
/// `#[expect(dead_code)]` rather than the enum carrying a blanket
/// `#[allow]`. Two reasons. A blanket allow silently absorbs the *next*
/// unproduced variant as well, so it stops being evidence of anything; and
/// `expect` is self-expiring — the build warns "unfulfilled lint
/// expectation" the moment a variant gains a producer, so the attribute is
/// deleted by whoever wires it up instead of being inherited indefinitely.
/// (`Inventory` was on the audit's dead list and is bound to Tab below, so
/// the enum-level allow had already outlived part of its own justification.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
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
    Quicksave,
    Quickload,
    #[expect(dead_code, reason = "no input source yet — see the enum docs (#2732)")]
    Pause,
}

impl InputAction {
    const fn bit(self) -> u16 {
        1_u16 << self as u8
    }

    const CONFIGURABLE: [Self; 10] = [
        Self::MoveForward,
        Self::MoveBackward,
        Self::StrafeLeft,
        Self::StrafeRight,
        Self::Jump,
        Self::Sprint,
        Self::Activate,
        Self::Attack,
        Self::Block,
        Self::Inventory,
    ];

    const fn setting_id(self) -> &'static str {
        match self {
            Self::MoveForward => "controls.bind.move_forward",
            Self::MoveBackward => "controls.bind.move_backward",
            Self::StrafeLeft => "controls.bind.strafe_left",
            Self::StrafeRight => "controls.bind.strafe_right",
            Self::Jump => "controls.bind.jump",
            Self::Sprint => "controls.bind.sprint",
            Self::Activate => "controls.bind.activate",
            Self::Attack => "controls.bind.attack",
            Self::Block => "controls.bind.block",
            Self::Inventory => "controls.bind.inventory",
            Self::Quicksave | Self::Quickload => "",
            Self::Pause => "",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::MoveForward => "Move forward",
            Self::MoveBackward => "Move backward",
            Self::StrafeLeft => "Strafe left",
            Self::StrafeRight => "Strafe right",
            Self::Jump => "Jump / ascend",
            Self::Sprint => "Sprint / boost",
            Self::Activate => "Activate",
            Self::Inventory => "Inventory",
            Self::Quicksave => "Quicksave",
            Self::Quickload => "Quickload",
            Self::Attack => "Attack",
            Self::Block => "Block",
            Self::Pause => "Pause",
        }
    }
}

/// Runtime-remappable keyboard bindings.
///
/// Mouse/gamepad sources will join this resource when their physical state is
/// promoted into `InputState`; action consumers do not need to change.
#[derive(Debug, Clone)]
pub(crate) struct ActionBindings {
    keyboard: HashMap<KeyCode, InputAction>,
    mouse: HashMap<MouseButton, InputAction>,
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
                (KeyCode::ShiftLeft, InputAction::Sprint),
                (KeyCode::KeyE, InputAction::Activate),
                (KeyCode::KeyR, InputAction::Attack),
                (KeyCode::KeyC, InputAction::Block),
                (KeyCode::Tab, InputAction::Inventory),
                (KeyCode::F5, InputAction::Quicksave),
                (KeyCode::F9, InputAction::Quickload),
            ]),
            mouse: HashMap::from([
                (MouseButton::Left, InputAction::Attack),
                (MouseButton::Right, InputAction::Block),
            ]),
        }
    }
}

impl ActionBindings {
    /// Replace the action produced by a physical keyboard key.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn bind_key(&mut self, key: KeyCode, action: InputAction) {
        self.rebind_key(key, action);
    }

    /// Assign one physical key to an action. If the key already belongs to a
    /// different action, that action receives the target action's old key.
    /// This keeps every configured action reachable and every key unique.
    fn rebind_key(&mut self, key: KeyCode, action: InputAction) -> Option<(InputAction, KeyCode)> {
        let old_key = self.key_for_action(action)?;
        if old_key == key {
            return None;
        }
        let displaced = self.keyboard.remove(&key);
        self.keyboard.remove(&old_key);
        if let Some(displaced_action) = displaced.filter(|other| *other != action) {
            self.keyboard.insert(old_key, displaced_action);
            self.keyboard.insert(key, action);
            return Some((displaced_action, old_key));
        }
        self.keyboard.insert(key, action);
        None
    }

    pub(crate) fn key_for_action(&self, action: InputAction) -> Option<KeyCode> {
        self.keyboard
            .iter()
            .find_map(|(key, bound)| (*bound == action).then_some(*key))
    }

    pub(crate) fn binding_label(&self, action: InputAction) -> &'static str {
        self.key_for_action(action)
            .map(key_label)
            .unwrap_or("Unbound")
    }

    fn held_mask(
        &self,
        keys_held: &std::collections::HashSet<KeyCode>,
        mouse_buttons_held: &std::collections::HashSet<MouseButton>,
    ) -> u16 {
        let keyboard = keys_held
            .iter()
            .filter_map(|key| self.keyboard.get(key))
            .fold(0, |mask, action| mask | action.bit());
        mouse_buttons_held
            .iter()
            .filter_map(|button| self.mouse.get(button))
            .fold(keyboard, |mask, action| mask | action.bit())
    }

    pub(crate) fn action_for_key(&self, key: KeyCode) -> Option<InputAction> {
        self.keyboard.get(&key).copied()
    }
}

/// Register the player-control settings owned by the action layer.
pub(crate) fn register_input_settings(
    registry: &mut SettingsRegistry,
) -> Result<(), SettingsError> {
    registry.register(SettingEntry::slider(
        MOUSE_SENSITIVITY_SETTING_ID,
        "Controls",
        "Mouse sensitivity",
        "Scale horizontal and vertical mouse-look speed.",
        1.0,
        0.1,
        4.0,
        0.05,
        "×",
    ))?;
    registry.register(SettingEntry::toggle(
        INVERT_LOOK_Y_SETTING_ID,
        "Controls",
        "Invert vertical look",
        "Move the view up when the mouse moves down, and vice versa.",
        false,
    ))?;

    let defaults = ActionBindings::default();
    for action in InputAction::CONFIGURABLE {
        let key = defaults
            .key_for_action(action)
            .expect("every configurable action has a default binding");
        registry.register(SettingEntry::choice(
            action.setting_id(),
            "Controls",
            action.label(),
            "Selecting a key already in use swaps the two bindings.",
            key_id(key),
            key_choices(),
        ))?;
    }
    Ok(())
}

/// Apply a control-owned setting to live input state. A key collision returns
/// the companion registry update produced by the binding swap.
pub(crate) fn apply_control_setting(
    world: &World,
    change: &SettingChange,
) -> Option<SettingChange> {
    match (change.id.as_str(), &change.value) {
        (MOUSE_SENSITIVITY_SETTING_ID, SettingValue::Number(multiplier)) => {
            world.resource_mut::<InputState>().look_sensitivity =
                DEFAULT_LOOK_SENSITIVITY * multiplier;
            None
        }
        (INVERT_LOOK_Y_SETTING_ID, SettingValue::Bool(inverted)) => {
            world.resource_mut::<InputState>().invert_look_y = *inverted;
            None
        }
        (id, SettingValue::Choice(key)) => {
            let action = action_for_setting(id)?;
            let key = parse_key_id(key)?;
            let swapped = world
                .resource_mut::<ActionBindings>()
                .rebind_key(key, action);
            // A held key must not migrate into a newly-bound action halfway
            // through a press. The next physical press establishes fresh
            // held/pressed edges.
            world.resource_mut::<InputState>().keys_held.clear();
            world
                .resource_mut::<InputState>()
                .mouse_buttons_held
                .clear();
            swapped.map(|(other, other_key)| {
                SettingChange::new(
                    other.setting_id(),
                    SettingValue::Choice(key_id(other_key).to_owned()),
                )
            })
        }
        _ => None,
    }
}

/// Reapply all registered controls after persisted settings are overlaid.
pub(crate) fn sync_registered_settings(world: &World) {
    let changes: Vec<SettingChange> = world
        .resource::<SettingsRegistry>()
        .entries()
        .filter(|entry| {
            entry.id == MOUSE_SENSITIVITY_SETTING_ID
                || entry.id == INVERT_LOOK_Y_SETTING_ID
                || action_for_setting(&entry.id).is_some()
        })
        .map(|entry| SettingChange::new(&entry.id, entry.value.clone()))
        .collect();

    for change in changes {
        if let Some(companion) = apply_control_setting(world, &change) {
            if let Err(error) = world
                .resource_mut::<SettingsRegistry>()
                .set(&companion.id, companion.value)
            {
                log::warn!("could not synchronize swapped binding: {error}");
            }
        }
    }
}

fn action_for_setting(id: &str) -> Option<InputAction> {
    InputAction::CONFIGURABLE
        .into_iter()
        .find(|action| action.setting_id() == id)
}

fn key_choices() -> Vec<SettingChoice> {
    SUPPORTED_KEYS
        .iter()
        .map(|key| SettingChoice::new(key_id(*key), key_label(*key)))
        .collect()
}

const SUPPORTED_KEYS: &[KeyCode] = &[
    KeyCode::KeyW,
    KeyCode::KeyA,
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::KeyE,
    // F (walk/fly toggle) and Q (fly-camera descend) remain global debug
    // controls, so exposing either here would create a binding that fires two
    // actions at once. Add them only after those paths join ActionState.
    KeyCode::KeyR,
    KeyCode::KeyC,
    KeyCode::KeyI,
    KeyCode::KeyJ,
    KeyCode::KeyK,
    KeyCode::KeyL,
    KeyCode::KeyX,
    KeyCode::KeyZ,
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::Space,
    KeyCode::ShiftLeft,
    KeyCode::ControlLeft,
    KeyCode::AltLeft,
    KeyCode::Tab,
    KeyCode::Enter,
];

fn key_id(key: KeyCode) -> &'static str {
    match key {
        KeyCode::KeyW => "key_w",
        KeyCode::KeyA => "key_a",
        KeyCode::KeyS => "key_s",
        KeyCode::KeyD => "key_d",
        KeyCode::KeyE => "key_e",
        KeyCode::KeyF => "key_f",
        KeyCode::KeyQ => "key_q",
        KeyCode::KeyR => "key_r",
        KeyCode::KeyC => "key_c",
        KeyCode::KeyI => "key_i",
        KeyCode::KeyJ => "key_j",
        KeyCode::KeyK => "key_k",
        KeyCode::KeyL => "key_l",
        KeyCode::KeyX => "key_x",
        KeyCode::KeyZ => "key_z",
        KeyCode::ArrowUp => "arrow_up",
        KeyCode::ArrowDown => "arrow_down",
        KeyCode::ArrowLeft => "arrow_left",
        KeyCode::ArrowRight => "arrow_right",
        KeyCode::Space => "space",
        KeyCode::ShiftLeft => "left_shift",
        KeyCode::ControlLeft => "left_control",
        KeyCode::AltLeft => "left_alt",
        KeyCode::Tab => "tab",
        KeyCode::Enter => "enter",
        _ => "unsupported",
    }
}

fn parse_key_id(id: &str) -> Option<KeyCode> {
    SUPPORTED_KEYS
        .iter()
        .copied()
        .find(|key| key_id(*key) == id)
}

fn key_label(key: KeyCode) -> &'static str {
    match key {
        KeyCode::KeyW => "W",
        KeyCode::KeyA => "A",
        KeyCode::KeyS => "S",
        KeyCode::KeyD => "D",
        KeyCode::KeyE => "E",
        KeyCode::KeyF => "F",
        KeyCode::KeyQ => "Q",
        KeyCode::KeyR => "R",
        KeyCode::KeyC => "C",
        KeyCode::KeyI => "I",
        KeyCode::KeyJ => "J",
        KeyCode::KeyK => "K",
        KeyCode::KeyL => "L",
        KeyCode::KeyX => "X",
        KeyCode::KeyZ => "Z",
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Down",
        KeyCode::ArrowLeft => "Left",
        KeyCode::ArrowRight => "Right",
        KeyCode::Space => "Space",
        KeyCode::ShiftLeft => "Left Shift",
        KeyCode::ControlLeft => "Left Ctrl",
        KeyCode::AltLeft => "Left Alt",
        KeyCode::Tab => "Tab",
        KeyCode::Enter => "Enter",
        _ => "Unknown",
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

/// Bounded physical-key hold used by real-data traversal automation.
///
/// Like [`InjectedKeyPulse`], this sits upstream of [`ActionState`]. The
/// command resolves an action to its *current* keyboard binding once, then
/// feeds that physical key through the normal binding map for exactly the
/// requested number of action refreshes. It therefore exercises the same
/// held/pressed/released edges as a real keyboard hold without depending on
/// wall-clock sleeps in smoke scripts.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct InjectedKeyHold {
    key: Option<KeyCode>,
    remaining_frames: u32,
}

impl Resource for InjectedKeyHold {}

impl InjectedKeyHold {
    fn take_key_for_frame(&mut self) -> Option<KeyCode> {
        if self.remaining_frames == 0 {
            self.key = None;
            return None;
        }
        self.remaining_frames -= 1;
        let key = self.key;
        if self.remaining_frames == 0 {
            self.key = None;
        }
        key
    }

    fn clear(&mut self) {
        self.key = None;
        self.remaining_frames = 0;
    }
}

pub(crate) fn queue_debug_action_press(world: &World, action_name: &str) -> Result<String, String> {
    let action =
        debug_action(action_name).ok_or_else(|| format!("unknown action `{action_name}`"))?;
    let (key, label) = {
        let bindings = world
            .try_resource::<ActionBindings>()
            .ok_or_else(|| "ActionBindings resource is not installed".to_string())?;
        let key = bindings
            .key_for_action(action)
            .ok_or_else(|| format!("{} is unbound", action.label()))?;
        (key, bindings.binding_label(action))
    };
    let mut pulse = world
        .try_resource_mut::<InjectedKeyPulse>()
        .ok_or_else(|| "InjectedKeyPulse resource is not installed".to_string())?;
    pulse.key = Some(key);
    // Machine-readable tokens are part of the playable-slice smoke contract.
    // Keep the prose-free `action=... binding=...` shape stable so a harmless
    // wording change cannot silently make the P0/P1/P2 gates unusable again.
    Ok(format!(
        "input.press: queued action={} binding={label}",
        action.label()
    ))
}

/// Queue a finite physical-key hold for a named gameplay action.
///
/// The debug frontend is intentionally only an alternate input source: the
/// returned key still passes through [`ActionBindings`] and the ordinary
/// character controller on subsequent frames.
pub(crate) fn queue_debug_action_hold(
    world: &World,
    action_name: &str,
    frames: u32,
) -> Result<String, String> {
    if frames == 0 {
        return Err("frame count must be greater than zero".to_string());
    }
    let action =
        debug_action(action_name).ok_or_else(|| format!("unknown action `{action_name}`"))?;
    let (key, label) = {
        let bindings = world
            .try_resource::<ActionBindings>()
            .ok_or_else(|| "ActionBindings resource is not installed".to_string())?;
        let key = bindings
            .key_for_action(action)
            .ok_or_else(|| format!("{} is unbound", action.label()))?;
        (key, bindings.binding_label(action))
    };
    let mut hold = world
        .try_resource_mut::<InjectedKeyHold>()
        .ok_or_else(|| "InjectedKeyHold resource is not installed".to_string())?;
    hold.key = Some(key);
    hold.remaining_frames = frames;
    Ok(format!(
        "input.hold: queued {} through the {label} binding for {frames} frames",
        action.label()
    ))
}

fn debug_action(name: &str) -> Option<InputAction> {
    match name.trim().to_ascii_lowercase().as_str() {
        "forward" | "move_forward" | "w" => Some(InputAction::MoveForward),
        "backward" | "move_backward" | "s" => Some(InputAction::MoveBackward),
        "left" | "strafe_left" | "a" => Some(InputAction::StrafeLeft),
        "right" | "strafe_right" | "d" => Some(InputAction::StrafeRight),
        "jump" | "space" => Some(InputAction::Jump),
        "sprint" | "shift" => Some(InputAction::Sprint),
        "activate" | "e" => Some(InputAction::Activate),
        "attack" | "r" => Some(InputAction::Attack),
        "block" | "c" => Some(InputAction::Block),
        "inventory" | "tab" => Some(InputAction::Inventory),
        _ => None,
    }
}

/// Cancel all synthetic physical input when a modal frontend takes focus.
pub(crate) fn clear_debug_input(world: &World) {
    if let Some(mut pulse) = world.try_resource_mut::<InjectedKeyPulse>() {
        pulse.key = None;
    }
    if let Some(mut hold) = world.try_resource_mut::<InjectedKeyHold>() {
        hold.clear();
    }
}

pub(crate) fn injected_hold_frames_remaining(world: &World) -> u32 {
    world
        .try_resource::<InjectedKeyHold>()
        .map(|hold| hold.remaining_frames)
        .unwrap_or(0)
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
    pub(crate) fn prompt_verb(&self) -> Option<&'static str> {
        self.target.map(|target| target.kind.verb())
    }
}

/// #3059 (PERF-D1-02) — reusable buffer for [`collect_candidates`],
/// mirroring `FootstepScratch`'s per-frame Vec reuse (`components.rs`):
/// `collect_candidates` clears and refills this map instead of
/// allocating a fresh `std::collections::HashMap` (SipHash) every frame
/// the crosshair path runs. `select_interaction_target` moves the map
/// out via `std::mem::take` for the duration of its own use and hands it
/// back afterward so the allocated capacity survives to next frame.
pub(crate) struct InteractionCandidateScratch {
    pub(crate) candidates: FxHashMap<EntityId, InteractionKind>,
}

impl Resource for InteractionCandidateScratch {}

impl Default for InteractionCandidateScratch {
    fn default() -> Self {
        Self {
            candidates: FxHashMap::default(),
        }
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
    let activate_pressed = world
        .try_resource::<ActionState>()
        .is_some_and(|state| state.was_pressed(InputAction::Activate));
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

/// Refresh the physical-input → gameplay-action snapshot once per frame.
///
/// [`crate::systems::player_controller_system`] calls this in `Stage::Early`
/// before either movement mode reads actions. [`interaction_system`] then
/// observes the same edge snapshot later in `Stage::Update`, preventing a
/// second refresh from consuming one-frame presses before activation runs.
pub(crate) fn refresh_action_state(world: &World) {
    let Some(input) = world.try_resource::<InputState>() else {
        return;
    };
    // #3060 — the two `HashSet` clones this used to make existed purely
    // to release `input`'s guard before acquiring `InjectedKeyPulse` /
    // `InjectedKeyHold` (both `_mut`) and `ActionBindings`. Since those
    // are three independent resource types with their own locks, holding
    // `input`'s READ guard alongside them costs nothing — RwLock readers
    // never conflict with a different type's guard, and nothing here
    // re-enters `InputState` itself. Keeping `input` alive through
    // `held_mask` below reads its two sets directly instead of cloning
    // them, then drops it in the same place the clones used to.
    let injected_key = world
        .try_resource_mut::<InjectedKeyPulse>()
        .and_then(|mut pulse| pulse.key.take());
    let injected_held_key = world
        .try_resource_mut::<InjectedKeyHold>()
        .and_then(|mut hold| hold.take_key_for_frame());
    let Some(bindings) = world.try_resource::<ActionBindings>() else {
        return;
    };
    let mut next_held = bindings.held_mask(&input.keys_held, &input.mouse_buttons_held);
    drop(input);
    if let Some(action) = injected_key.and_then(|key| bindings.action_for_key(key)) {
        next_held |= action.bit();
    }
    if let Some(action) = injected_held_key.and_then(|key| bindings.action_for_key(key)) {
        next_held |= action.bit();
    }
    drop(bindings);

    let Some(mut state) = world.try_resource_mut::<ActionState>() else {
        return;
    };
    state.refresh(next_held);
}

fn select_interaction_target(world: &World) -> Option<InteractionTarget> {
    let (origin, direction) = camera_ray(world)?;
    let candidates = collect_candidates(world);

    let mut targets: Vec<_> = candidates
        .iter()
        .filter(|(entity, _)| !activation_is_blocked(world, **entity))
        .filter_map(|(entity, kind)| {
            let bound = interaction_bound(world, *entity)?;
            let distance = ray_sphere_distance(origin, direction, bound)?;
            (distance <= INTERACTION_REACH_BU).then_some(InteractionTarget {
                entity: *entity,
                kind: *kind,
                distance,
            })
        })
        .collect();
    // #3059 — hand the map's allocated capacity back to the scratch
    // resource for next frame instead of letting it drop here.
    if let Some(mut scratch) = world.try_resource_mut::<InteractionCandidateScratch>() {
        scratch.candidates = candidates;
    }
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

    // #3058 — resolve only the player's own excluded body before
    // acquiring PhysicsWorld (a single targeted `get`, not a full
    // entity->body `Vec` collected up front for every rigid body in the
    // world). Keeps the resource/component lock order non-overlapping,
    // same as before. The reverse lookup (hit body -> owning entity)
    // below re-acquires the same query AFTER the raycast completes and
    // scans it directly — no intermediate `Vec` is ever materialised.
    let player = world
        .try_resource::<byroredux_scripting::papyrus_demo::PlayerEntity>()
        .map(|player| player.0);
    let excluded_body = player.and_then(|entity| {
        world
            .query::<byroredux_physics::RapierHandles>()
            .and_then(|handles| handles.get(entity).map(|h| h.body))
    });

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
    let Some(hit_owner) = world
        .query::<byroredux_physics::RapierHandles>()
        .and_then(|handles| {
            handles
                .iter()
                .find_map(|(entity, handles)| (handles.body == hit_body).then_some(entity))
        })
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

pub(crate) fn camera_ray(world: &World) -> Option<(Vec3, Vec3)> {
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

/// #3059 (PERF-D1-02) — reuses [`InteractionCandidateScratch`] when
/// registered (the live engine, via `boot.rs`), falling back to a fresh
/// map otherwise (bare test worlds) so correctness never depends on the
/// scratch resource being present. Either way the map is `FxHashMap`
/// (SipHash → FxHash over an `EntityId` keyspace, per the project's
/// hot-path hashing rule, #2923/#1368/#2174) and there is no trailing
/// `Vec` conversion — callers iterate the map directly.
fn collect_candidates(world: &World) -> FxHashMap<EntityId, InteractionKind> {
    if let Some(mut scratch) = world.try_resource_mut::<InteractionCandidateScratch>() {
        scratch.candidates.clear();
        populate_candidates(world, &mut scratch.candidates);
        std::mem::take(&mut scratch.candidates)
    } else {
        let mut candidates = FxHashMap::default();
        populate_candidates(world, &mut candidates);
        candidates
    }
}

fn populate_candidates(world: &World, candidates: &mut FxHashMap<EntityId, InteractionKind>) {
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
}

fn activation_is_blocked(world: &World, entity: EntityId) -> bool {
    // #3098 — locked ⇒ not activatable. This is the deliberately blunt
    // first policy: no key check, no lockpicking, no "locked but you
    // have the key" carve-out. Every REFR that reaches this gate with a
    // `Locked` component was parsed off an authored `XLOC`, so this
    // covers doors today and containers as soon as they gain an
    // activation path of their own (see `Locked`'s doc for scope).
    if world.get::<Locked>(entity).is_some() {
        return true;
    }
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
        world.insert_resource(InjectedKeyHold::default());
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

        refresh_action_state(&world);
        {
            let actions = world.resource::<ActionState>();
            assert!(actions.is_held(InputAction::Activate));
            assert!(actions.was_pressed(InputAction::Activate));
        }
        refresh_action_state(&world);
        assert!(
            !world
                .resource::<ActionState>()
                .was_pressed(InputAction::Activate),
            "held key must not auto-repeat"
        );

        world
            .resource_mut::<InputState>()
            .keys_held
            .remove(&KeyCode::KeyE);
        refresh_action_state(&world);
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

        refresh_action_state(&world);
        assert!(world
            .resource::<ActionState>()
            .was_pressed(InputAction::Activate));
    }

    #[test]
    fn injected_e_key_pulse_uses_binding_and_releases_next_frame() {
        let world = input_fixture();
        let message = queue_debug_action_press(&world, "activate").unwrap();
        assert_eq!(message, "input.press: queued action=Activate binding=E");

        refresh_action_state(&world);
        assert!(world
            .resource::<ActionState>()
            .is_held(InputAction::Activate));
        refresh_action_state(&world);
        assert!(world
            .resource::<ActionState>()
            .was_released(InputAction::Activate));
    }

    #[test]
    fn injected_hold_uses_remapped_binding_for_exact_frame_count() {
        let world = input_fixture();
        world
            .resource_mut::<ActionBindings>()
            .bind_key(KeyCode::KeyR, InputAction::MoveForward);
        let queued = queue_debug_action_hold(&world, "forward", 3).unwrap();
        assert!(queued.contains("through the R binding for 3 frames"));

        for frame in 0..3 {
            refresh_action_state(&world);
            let actions = world.resource::<ActionState>();
            assert!(actions.is_held(InputAction::MoveForward), "frame {frame}");
            assert_eq!(
                actions.was_pressed(InputAction::MoveForward),
                frame == 0,
                "only the first held frame is a press edge"
            );
        }

        refresh_action_state(&world);
        let actions = world.resource::<ActionState>();
        assert!(!actions.is_held(InputAction::MoveForward));
        assert!(actions.was_released(InputAction::MoveForward));
    }

    #[test]
    fn movement_jump_and_sprint_follow_remappable_actions() {
        let world = input_fixture();
        world
            .resource_mut::<ActionBindings>()
            .bind_key(KeyCode::KeyR, InputAction::MoveForward);
        world.resource_mut::<InputState>().keys_held.extend([
            KeyCode::KeyR,
            KeyCode::KeyD,
            KeyCode::Space,
            KeyCode::ShiftLeft,
        ]);

        refresh_action_state(&world);
        let actions = world.resource::<ActionState>();
        for action in [
            InputAction::MoveForward,
            InputAction::StrafeRight,
            InputAction::Jump,
            InputAction::Sprint,
        ] {
            assert!(actions.is_held(action), "{action:?} binding was ignored");
        }
    }

    #[test]
    fn mouse_buttons_drive_attack_and_block_actions() {
        let world = input_fixture();
        world
            .resource_mut::<InputState>()
            .mouse_buttons_held
            .extend([MouseButton::Left, MouseButton::Right]);

        refresh_action_state(&world);
        let actions = world.resource::<ActionState>();
        assert!(actions.was_pressed(InputAction::Attack));
        assert!(actions.is_held(InputAction::Block));
    }

    #[test]
    fn injected_attack_pulse_uses_the_live_keyboard_binding() {
        let world = input_fixture();
        world
            .resource_mut::<ActionBindings>()
            .bind_key(KeyCode::KeyX, InputAction::Attack);
        let message = queue_debug_action_press(&world, "attack").unwrap();
        assert_eq!(message, "input.press: queued action=Attack binding=X");

        refresh_action_state(&world);
        assert!(world
            .resource::<ActionState>()
            .was_pressed(InputAction::Attack));
    }

    #[test]
    fn rebinding_an_occupied_key_swaps_actions_and_clears_held_input() {
        let world = input_fixture();
        world
            .resource_mut::<InputState>()
            .keys_held
            .insert(KeyCode::KeyW);

        let companion = apply_control_setting(
            &world,
            &SettingChange::new(
                InputAction::Activate.setting_id(),
                SettingValue::Choice("key_w".to_owned()),
            ),
        )
        .expect("W was occupied by MoveForward, so the swap must be reported");

        let bindings = world.resource::<ActionBindings>();
        assert_eq!(
            bindings.key_for_action(InputAction::Activate),
            Some(KeyCode::KeyW)
        );
        assert_eq!(
            bindings.key_for_action(InputAction::MoveForward),
            Some(KeyCode::KeyE)
        );
        assert_eq!(bindings.binding_label(InputAction::Activate), "W");
        drop(bindings);
        assert_eq!(companion.id, InputAction::MoveForward.setting_id());
        assert_eq!(companion.value, SettingValue::Choice("key_e".to_owned()));
        assert!(world.resource::<InputState>().keys_held.is_empty());
    }

    #[test]
    fn registered_control_settings_apply_sensitivity_and_invert_y() {
        let world = input_fixture();
        apply_control_setting(
            &world,
            &SettingChange::new(MOUSE_SENSITIVITY_SETTING_ID, SettingValue::Number(2.5)),
        );
        apply_control_setting(
            &world,
            &SettingChange::new(INVERT_LOOK_Y_SETTING_ID, SettingValue::Bool(true)),
        );
        let input = world.resource::<InputState>();
        assert_eq!(input.look_sensitivity, DEFAULT_LOOK_SENSITIVITY * 2.5);
        assert!(input.invert_look_y);
    }

    #[test]
    fn every_configurable_action_registers_a_valid_binding_setting() {
        let mut registry = SettingsRegistry::default();
        register_input_settings(&mut registry).unwrap();
        assert_eq!(
            registry.entries().len(),
            InputAction::CONFIGURABLE.len() + 2
        );
        for action in InputAction::CONFIGURABLE {
            let entry = registry
                .get(action.setting_id())
                .expect("missing configurable action setting");
            assert!(matches!(entry.value, SettingValue::Choice(_)));
        }
        assert!(parse_key_id("key_f").is_none());
        assert!(parse_key_id("key_q").is_none());
    }

    #[test]
    fn modal_input_focus_releases_actions_without_retriggering_them() {
        let world = input_fixture();
        world
            .resource_mut::<InputState>()
            .keys_held
            .extend([KeyCode::KeyW, KeyCode::KeyE]);
        world
            .resource_mut::<InputState>()
            .mouse_buttons_held
            .insert(MouseButton::Left);
        refresh_action_state(&world);

        queue_debug_action_hold(&world, "forward", 30).unwrap();

        assert!(!crate::ui_input::release_world_input(&world));
        refresh_action_state(&world);

        let actions = world.resource::<ActionState>();
        for action in [
            InputAction::MoveForward,
            InputAction::Activate,
            InputAction::Attack,
        ] {
            assert!(!actions.is_held(action));
            assert!(actions.was_released(action));
            assert!(!actions.was_pressed(action));
        }
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

        refresh_action_state(&world);
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

    /// #3059 — `select_interaction_target` must still find the same target
    /// when `InteractionCandidateScratch` is registered (the live-engine
    /// path via `boot.rs`) as when it isn't (every other test in this
    /// module, via `input_fixture`/`physics_fixture`) — the scratch is a
    /// reuse optimisation, not a behaviour change.
    #[test]
    fn selection_is_unaffected_by_the_candidate_scratch_resource() {
        let mut world = physics_fixture();
        world.insert_resource(InteractionCandidateScratch::default());
        spawn_camera(&mut world);
        let door = spawn_test_door(&mut world, Vec3::new(0.0, 0.0, -100.0));
        byroredux_physics::physics_sync_system(&world, 0.0);

        assert_eq!(select_interaction_target(&world).unwrap().entity, door);
    }

    /// #3059 — the whole point of moving the map in and out of
    /// `InteractionCandidateScratch` via `mem::take`/restore is that its
    /// allocated capacity survives across frames instead of being
    /// reallocated from empty every call. Two candidates in, drop one,
    /// call again: capacity must not have reset to whatever a fresh
    /// `FxHashMap::default()` would start at.
    #[test]
    fn candidate_scratch_capacity_survives_across_calls() {
        let mut world = physics_fixture();
        world.insert_resource(InteractionCandidateScratch::default());
        spawn_camera(&mut world);
        let door_a = spawn_test_door(&mut world, Vec3::new(0.0, 0.0, -100.0));
        let door_b = spawn_test_door(&mut world, Vec3::new(50.0, 0.0, -100.0));
        byroredux_physics::physics_sync_system(&world, 0.0);

        select_interaction_target(&world);
        let capacity_after_first = world
            .resource::<InteractionCandidateScratch>()
            .candidates
            .capacity();
        assert!(
            capacity_after_first >= 2,
            "scratch must hold at least the two candidates just collected, \
             got capacity {capacity_after_first}"
        );

        // Despawn one candidate and call again — capacity must not shrink
        // back to a fresh-allocation baseline; `clear()` (used by
        // `collect_candidates`) never releases capacity, unlike
        // rebuilding a new map from scratch every call would.
        world.despawn(door_b);
        select_interaction_target(&world);
        let capacity_after_second = world
            .resource::<InteractionCandidateScratch>()
            .candidates
            .capacity();
        assert!(
            capacity_after_second >= capacity_after_first,
            "reused scratch capacity must not shrink between calls: \
             {capacity_after_first} -> {capacity_after_second}"
        );
        let _ = door_a;
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

    #[test]
    fn save_actions_have_conventional_default_bindings() {
        let bindings = ActionBindings::default();
        assert_eq!(
            bindings.action_for_key(KeyCode::F5),
            Some(InputAction::Quicksave)
        );
        assert_eq!(
            bindings.action_for_key(KeyCode::F9),
            Some(InputAction::Quickload)
        );
    }
}
