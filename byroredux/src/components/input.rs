//! Keyboard / mouse input state for the fly camera. (#5467 split from `components.rs`)

use byroredux_core::ecs::Resource;
// #5173 — the per-lane ground-cover arrays below size from the same splat
// constant the terrain tile + shader loops size from, so a lane bump cannot
// leave this component silently 8-wide while the rest of the chain moves.
use std::collections::HashSet;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

/// Tracks keyboard and mouse input state for the fly camera.
pub(crate) const DEFAULT_LOOK_SENSITIVITY: f32 = 0.002;

pub(crate) struct InputState {
    pub(crate) keys_held: HashSet<KeyCode>,
    pub(crate) mouse_buttons_held: HashSet<MouseButton>,
    /// Yaw (horizontal) and pitch (vertical) in radians.
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) mouse_captured: bool,
    pub(crate) move_speed: f32,
    pub(crate) look_sensitivity: f32,
    pub(crate) invert_look_y: bool,
}

impl Resource for InputState {}

impl Default for InputState {
    fn default() -> Self {
        Self {
            keys_held: HashSet::new(),
            mouse_buttons_held: HashSet::new(),
            yaw: 0.0,
            pitch: 0.0,
            mouse_captured: false,
            move_speed: 200.0, // Bethesda units per second
            look_sensitivity: DEFAULT_LOOK_SENSITIVITY,
            invert_look_y: false,
        }
    }
}

// ── M44 Phase 3.5 — footstep gameplay loop ──────────────────────────
