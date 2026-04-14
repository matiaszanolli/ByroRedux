//! Animated property components — set by the animation system each frame.
//!
//! Sparse — only entities with active non-transform animation channels need these.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// Animated visibility toggle. When false, the renderer skips this entity.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedVisibility(pub bool);
impl Component for AnimatedVisibility {
    type Storage = SparseSetStorage<Self>;
}

/// Animated alpha override (0.0–1.0). Used for material alpha animation.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedAlpha(pub f32);
impl Component for AnimatedAlpha {
    type Storage = SparseSetStorage<Self>;
}

/// Animated color override (RGB). Used for material/shader color animation.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedColor(pub Vec3);
impl Component for AnimatedColor {
    type Storage = SparseSetStorage<Self>;
}
