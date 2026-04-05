//! Root motion extraction from animation data.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// Per-frame root motion displacement extracted from the accumulation root node.
///
/// Horizontal (XZ) translation from the accum root's animation channel is stored
/// here instead of being applied to the node's Transform. A character controller
/// or physics system reads this to move the entity through the world.
pub struct RootMotionDelta(pub Vec3);

impl Component for RootMotionDelta {
    type Storage = SparseSetStorage<Self>;
}

/// Extract root motion from a translation sample.
///
/// Returns (animation_translation, root_motion_delta):
/// - animation_translation: vertical (Y) component preserved, horizontal zeroed
/// - root_motion_delta: horizontal (XZ) components, vertical zeroed
pub fn split_root_motion(translation: Vec3) -> (Vec3, Vec3) {
    let anim = Vec3::new(0.0, translation.y, 0.0);
    let delta = Vec3::new(translation.x, 0.0, translation.z);
    (anim, delta)
}
