//! Root motion extraction from animation data.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// Per-frame root motion displacement extracted from the accumulation root node.
///
/// Stored in **renderer-space Y-up** — same frame as the entity's
/// `Transform`. The accum root's NIF translation channel has already
/// passed through `zup_to_yup_pos` during import (Gamebryo Z-up →
/// renderer Y-up: `(x, y, z) → (x, z, -y)`), so by the time it reaches
/// this component, Gamebryo's XY walking plane has become renderer XZ.
/// A character controller reading this delta must rotate it by the
/// entity's yaw before applying world-space motion. See #526.
pub struct RootMotionDelta(pub Vec3);

impl Component for RootMotionDelta {
    type Storage = SparseSetStorage<Self>;
}

/// Split a sampled root-node translation into the part that stays on
/// the `Transform` and the part that becomes a `RootMotionDelta`.
///
/// Both input and output are in **renderer Y-up space** (post
/// `zup_to_yup_pos` import conversion). Y is vertical (pre-import
/// Gamebryo Z); XZ is the horizontal walking plane (pre-import
/// Gamebryo XY).
///
/// Returns `(animation_translation, root_motion_delta)`:
/// - `animation_translation` — vertical-only (jump / crouch pose),
///   written back to the `Transform` so the skeleton still bobs
///   relative to its root.
/// - `root_motion_delta` — horizontal-only, consumed by whichever
///   system advances the entity through the world.
pub fn split_root_motion(translation: Vec3) -> (Vec3, Vec3) {
    let anim = Vec3::new(0.0, translation.y, 0.0);
    let delta = Vec3::new(translation.x, 0.0, translation.z);
    (anim, delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A walking animation in Gamebryo authors forward motion on +Y.
    /// `zup_to_yup_pos` rewrites that to renderer -Z at import. The
    /// split must then route the -Z component into `delta` and leave
    /// `anim` zero.
    #[test]
    fn walk_forward_routes_to_delta() {
        let forward_in_renderer_space = Vec3::new(0.0, 0.0, -1.0);
        let (anim, delta) = split_root_motion(forward_in_renderer_space);
        assert_eq!(anim, Vec3::ZERO, "no vertical component");
        assert_eq!(
            delta,
            Vec3::new(0.0, 0.0, -1.0),
            "walking forward in Gamebryo (+Y) survives coord-convert to renderer (-Z) and lands in delta"
        );
    }

    /// A jump authors vertical motion on Gamebryo +Z → renderer +Y.
    /// The split must preserve Y on the transform so the mesh bobs,
    /// and emit zero delta so the character controller doesn't
    /// double-apply gravity.
    #[test]
    fn jump_vertical_routes_to_transform() {
        let jump_in_renderer_space = Vec3::new(0.0, 1.0, 0.0);
        let (anim, delta) = split_root_motion(jump_in_renderer_space);
        assert_eq!(
            anim,
            Vec3::new(0.0, 1.0, 0.0),
            "vertical stays on transform"
        );
        assert_eq!(delta, Vec3::ZERO, "no horizontal translation");
    }

    /// Mixed diagonal step. Confirms Y is strictly a mask — X and Z
    /// pass through independent of sign.
    #[test]
    fn diagonal_splits_y_from_xz() {
        let t = Vec3::new(0.25, 0.5, -0.75);
        let (anim, delta) = split_root_motion(t);
        assert_eq!(anim, Vec3::new(0.0, 0.5, 0.0));
        assert_eq!(delta, Vec3::new(0.25, 0.0, -0.75));
        // Components must reconstruct the original exactly (no loss).
        assert_eq!(anim + delta, t);
    }
}
