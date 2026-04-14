//! Light source component for placed lights (LIGH records).

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// A point/spot light source placed in the world.
///
/// Populated from LIGH record DATA subrecord (radius, color, flags).
/// Not rendered yet — this is a data component for future lighting systems.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct LightSource {
    /// Light radius in Bethesda units.
    pub radius: f32,
    /// Light color (RGB, normalized 0..1).
    pub color: [f32; 3],
    /// LIGH flags (dynamic, can be carried, flicker, etc.).
    pub flags: u32,
}

impl Component for LightSource {
    type Storage = SparseSetStorage<Self>;
}
