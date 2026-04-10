//! Application-specific marker components and resources.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, Resource, SparseSetStorage};
use byroredux_core::string::FixedString;
use std::collections::{HashMap, HashSet};
use winit::keyboard::KeyCode;

/// Marker component for entities that should spin in the demo scene.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Spinning;
impl Component for Spinning {
    type Storage = SparseSetStorage<Self>;
}

/// Marker component for entities that use alpha blending.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AlphaBlend;
impl Component for AlphaBlend {
    type Storage = SparseSetStorage<Self>;
}

/// Marker component for entities that need two-sided rendering (no backface culling).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TwoSided;
impl Component for TwoSided {
    type Storage = SparseSetStorage<Self>;
}

/// Marker component for decal geometry (renders on top of coplanar surfaces).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Decal;
impl Component for Decal {
    type Storage = SparseSetStorage<Self>;
}

/// Bindless texture handle for a normal map (parallels TextureHandle for diffuse).
#[derive(Debug, Clone, Copy)]
pub(crate) struct NormalMapHandle(pub(crate) u32);
impl Component for NormalMapHandle {
    type Storage = SparseSetStorage<Self>;
}

/// System names stored as a resource for the `systems` console command.
pub(crate) struct SystemList(pub(crate) Vec<String>);
impl Resource for SystemList {}

/// Cell lighting from the ESM (ambient + directional + fog).
pub(crate) struct CellLightingRes {
    pub(crate) ambient: [f32; 3],
    pub(crate) directional_color: [f32; 3],
    /// Direction vector in Y-up space (computed from rotation).
    pub(crate) directional_dir: [f32; 3],
    /// Fog color (RGB 0-1).
    pub(crate) fog_color: [f32; 3],
    /// Fog near distance (game units).
    pub(crate) fog_near: f32,
    /// Fog far distance (game units).
    pub(crate) fog_far: f32,
}
impl Resource for CellLightingRes {}

/// Cached name→entity mapping for the animation system.
/// Rebuilt only when the entity count changes (no per-frame allocations).
pub(crate) struct NameIndex {
    pub(crate) map: HashMap<FixedString, EntityId>,
    pub(crate) generation: u32,
}
impl Resource for NameIndex {}

impl NameIndex {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
            generation: u32::MAX, // Force rebuild on first use.
        }
    }
}

/// Tracks keyboard and mouse input state for the fly camera.
pub(crate) struct InputState {
    pub(crate) keys_held: HashSet<KeyCode>,
    /// Yaw (horizontal) and pitch (vertical) in radians.
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) mouse_captured: bool,
    pub(crate) move_speed: f32,
    pub(crate) look_sensitivity: f32,
}

impl Resource for InputState {}

impl Default for InputState {
    fn default() -> Self {
        Self {
            keys_held: HashSet::new(),
            yaw: 0.0,
            pitch: 0.0,
            mouse_captured: false,
            move_speed: 200.0, // Bethesda units per second
            look_sensitivity: 0.002,
        }
    }
}
