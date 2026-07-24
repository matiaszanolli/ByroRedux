//! Computed world-space transform.
//!
//! PackedStorage: read every frame by the renderer for model matrices.
//! Written only by `transform_propagation_system`, never by user code.

use crate::ecs::packed::PackedStorage;
use crate::ecs::storage::Component;
use crate::math::{Mat4, Quat, Vec3};

/// World-space transform computed from the local `Transform` and parent chain.
///
/// For root entities (no `Parent`), this equals the entity's `Transform`.
/// For child entities, this is the composed result of all ancestor transforms.
///
/// The renderer reads this (not `Transform`) for the model matrix.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct GlobalTransform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: f32,
}

impl GlobalTransform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: 1.0,
    };

    pub fn new(translation: Vec3, rotation: Quat, scale: f32) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    /// Build a 4x4 model matrix: scale → rotate → translate.
    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::splat(self.scale),
            self.rotation,
            self.translation,
        )
    }

    /// Compose a parent's global transform with a child's local transform.
    pub fn compose(
        parent: &GlobalTransform,
        local_translation: Vec3,
        local_rotation: Quat,
        local_scale: f32,
    ) -> Self {
        let (translation, rotation, scale) = Self::compose_trs(
            parent.translation,
            parent.rotation,
            parent.scale,
            local_translation,
            local_rotation,
            local_scale,
        );
        Self {
            translation,
            rotation,
            scale,
        }
    }

    /// Compose a parent's world transform (loose translation / rotation /
    /// uniform scale) with a child's local TRS, returning loose components.
    ///
    /// The single home for the parent→child composition order.
    /// [`Self::compose`] is the `GlobalTransform`-typed wrapper; placement
    /// code that works in loose `(Vec3, Quat, f32)` triples — REFR / SCOL /
    /// LOD spawn — calls this so a future composition-order change lands in
    /// exactly one place (TD2-106 / #2065).
    pub fn compose_trs(
        parent_translation: Vec3,
        parent_rotation: Quat,
        parent_scale: f32,
        local_translation: Vec3,
        local_rotation: Quat,
        local_scale: f32,
    ) -> (Vec3, Quat, f32) {
        (
            Self::compose_translation(
                parent_translation,
                parent_rotation,
                parent_scale,
                local_translation,
            ),
            parent_rotation * local_rotation,
            parent_scale * local_scale,
        )
    }

    /// Translation-only half of [`Self::compose_trs`]: place a child point
    /// `local_translation` into the parent's world frame. Used by placement
    /// sites (lights, particle emitters) that carry no meaningful child
    /// rotation/scale, so they share the exact translation formula rather
    /// than re-deriving it.
    pub fn compose_translation(
        parent_translation: Vec3,
        parent_rotation: Quat,
        parent_scale: f32,
        local_translation: Vec3,
    ) -> Vec3 {
        parent_translation + parent_rotation * (parent_scale * local_translation)
    }
}

impl Default for GlobalTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Component for GlobalTransform {
    type Storage = PackedStorage<Self>;
    // Change-tracking ON: `make_world_bound_propagation_system` drains this
    // dirty set every frame (the required consumer — see the original note
    // about not enabling without a drainer). The dirty set = the entities
    // whose world transform was (re)written this frame by transform
    // propagation / billboards, which is exactly the set of leaf bounds that
    // need recomputing. The render-instance cache (a future second consumer,
    // deferred to the M40 parallel scheduler) will need a fan-out resource
    // rather than a second `take_dirty`, since draining is destructive.
    const TRACK_CHANGES: bool = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn identity_matrix() {
        let g = GlobalTransform::IDENTITY;
        assert_eq!(g.to_matrix(), Mat4::IDENTITY);
    }

    #[test]
    fn compose_translation_only() {
        let parent = GlobalTransform::new(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let child =
            GlobalTransform::compose(&parent, Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        assert!((child.translation.x - 15.0).abs() < 1e-5);
    }

    #[test]
    fn compose_with_scale() {
        let parent = GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 2.0);
        let child =
            GlobalTransform::compose(&parent, Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        // Parent scale 2.0 * child local offset 5.0 = 10.0
        assert!((child.translation.x - 10.0).abs() < 1e-5);
        assert!((child.scale - 2.0).abs() < 1e-5);
    }

    #[test]
    fn compose_with_rotation() {
        let parent = GlobalTransform::new(Vec3::ZERO, Quat::from_rotation_y(FRAC_PI_2), 1.0);
        // Child at (1, 0, 0) local → parent rotates 90° around Y → (0, 0, -1) world
        let child =
            GlobalTransform::compose(&parent, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        assert!(child.translation.x.abs() < 1e-4);
        assert!((child.translation.z + 1.0).abs() < 1e-4);
    }

    #[test]
    fn compose_chain_three_levels() {
        let root = GlobalTransform::new(Vec3::new(100.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let mid = GlobalTransform::compose(&root, Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 2.0);
        let leaf = GlobalTransform::compose(&mid, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        // root(100) + mid_local(10) = 110, mid_scale=2, leaf_local(1)*2 = 2 → 112
        assert!((leaf.translation.x - 112.0).abs() < 1e-4);
        assert!((leaf.scale - 2.0).abs() < 1e-5);
    }

    /// TD2-106 / #2065 — `compose_trs` is the loose-component home the
    /// REFR/SCOL/LOD placement sites now call. It must produce exactly the
    /// hand-rolled formula those sites used (`parent_rot * (parent_scale *
    /// local) + parent_pos`, `parent_rot * local_rot`, `parent_scale *
    /// local_scale`), and `compose` must agree with it component-for-
    /// component so the typed and loose entry points can't diverge.
    #[test]
    fn compose_trs_matches_inline_formula_and_compose() {
        let parent_pos = Vec3::new(3.0, -4.0, 5.0);
        let parent_rot = Quat::from_rotation_y(FRAC_PI_2);
        let parent_scale = 2.5_f32;
        let local_pos = Vec3::new(1.0, 2.0, -3.0);
        let local_rot = Quat::from_rotation_x(FRAC_PI_2);
        let local_scale = 0.5_f32;

        let (pos, rot, scale) = GlobalTransform::compose_trs(
            parent_pos,
            parent_rot,
            parent_scale,
            local_pos,
            local_rot,
            local_scale,
        );

        // The exact expression every placement site hand-rolled pre-fix.
        let expect_pos = parent_rot * (parent_scale * local_pos) + parent_pos;
        let expect_rot = parent_rot * local_rot;
        let expect_scale = parent_scale * local_scale;
        assert!((pos - expect_pos).length() < 1e-6);
        assert!((rot.dot(expect_rot).abs() - 1.0).abs() < 1e-6);
        assert!((scale - expect_scale).abs() < 1e-6);

        // The typed wrapper must be bit-for-bit consistent with the loose one.
        let parent = GlobalTransform::new(parent_pos, parent_rot, parent_scale);
        let composed = GlobalTransform::compose(&parent, local_pos, local_rot, local_scale);
        assert_eq!(composed.translation, pos);
        assert_eq!(composed.rotation, rot);
        assert_eq!(composed.scale, scale);

        // Translation-only helper == the translation component of the full one.
        assert_eq!(
            GlobalTransform::compose_translation(parent_pos, parent_rot, parent_scale, local_pos),
            pos
        );
    }
}
