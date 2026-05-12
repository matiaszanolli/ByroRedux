//! Light source component for placed lights (LIGH records).

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// A point/spot light source placed in the world.
///
/// Populated from LIGH record DATA subrecord (radius, color, flags).
/// Per-frame controller animation (NiLight{Color,Dimmer,Intensity,Radius}
/// Controller from the source NIF) mutates the fields below as
/// channels sample; the renderer's light-buffer build reads
/// `final_color = color * dimmer * intensity` and uses `radius`
/// directly. Pre-fix the `dimmer` / `intensity` slots didn't exist
/// and the import dropped every NiLight animation controller on the
/// floor — every torch / lantern / plasma weapon emitted constant
/// light regardless of authored flicker / pulse / dim. See #983.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct LightSource {
    /// Light radius in Bethesda units.
    pub radius: f32,
    /// Light color (RGB, normalized 0..1).
    pub color: [f32; 3],
    /// LIGH flags (dynamic, can be carried, flicker, etc.).
    pub flags: u32,
    /// `NiLightBase.dimmer` — multiplicative scalar applied to
    /// `color` at render time. Default `1.0`. Animated by
    /// `NiLightDimmerController` (FloatTarget::LightDimmer).
    pub dimmer: f32,
    /// Animated intensity multiplier from `NiLightIntensityController`
    /// (FloatTarget::LightIntensity). Default `1.0`. Distinct from
    /// `dimmer`: dimmer is the authored NiLightBase scalar; intensity
    /// is a separate controller-driven channel that some content
    /// authors layer on top (lantern flicker rides on `intensity`
    /// while the steady `dimmer` stays constant). Renderer
    /// composes `color * dimmer * intensity`.
    pub intensity: f32,
}

impl Default for LightSource {
    fn default() -> Self {
        Self {
            radius: 0.0,
            color: [1.0, 1.0, 1.0],
            flags: 0,
            dimmer: 1.0,
            intensity: 1.0,
        }
    }
}

impl Component for LightSource {
    type Storage = SparseSetStorage<Self>;
}
