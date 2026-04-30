//! Skinned mesh binding — links a rendered mesh to the bones that deform it.
//!
//! Populated by NIF import after node entities are spawned: the importer
//! produces `ImportedSkin` with bone names, which scene assembly resolves to
//! `EntityId`s by walking the spawned skeleton. Per frame, the renderer walks
//! entities with both `SkinnedMesh` and a mesh handle, reads each bone's
//! `GlobalTransform`, multiplies by the corresponding `bind_inverse`, and
//! uploads the resulting 4x4 palette to the bone SSBO.
//!
//! The vertex shader then samples the palette using per-vertex `bone_indices`
//! + `bone_weights` baked into the vertex buffer at import time.
//!
//! Storage: sparse — only actor / creature meshes are skinned; statics,
//! weapons on the ground, and world geometry are not. See issue #178.
//!
//! Sparse per-bone data (indices + weights) lives on the vertex buffer, not
//! on this component, because it's per-vertex and is uploaded to the GPU
//! once at mesh upload time. This component only carries the per-mesh
//! binding state that changes at runtime (which entities drive which bones).

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};
use crate::math::Mat4;

/// Maximum bones per skinned mesh. Matches the renderer's `MAX_BONES_PER_MESH`
/// — the palette SSBO reserves this many slots per instance. Skyrim humanoid
/// skeletons run ~60 bones; beast races push ~80; creatures vary. 128 gives
/// comfortable headroom without ballooning GPU memory.
pub const MAX_BONES_PER_MESH: usize = 128;

/// Binds a mesh entity to the bones that deform it each frame.
///
/// Constructed by scene assembly after both the skeleton nodes and the
/// mesh entity exist. The `bones` vector is parallel to `bind_inverses`
/// and indexes into per-vertex `bone_indices` on the vertex buffer.
#[derive(Debug, Clone)]
pub struct SkinnedMesh {
    /// Entity of the skeleton root node (for debugging / introspection).
    /// `None` if the importer could not identify a root bone.
    pub skeleton_root: Option<EntityId>,
    /// Bone entities, in the order referenced by per-vertex `bone_indices`.
    /// Any entry may be `None` if a bone name from `ImportedSkin` could not
    /// be resolved — the palette computation substitutes identity for those.
    pub bones: Vec<Option<EntityId>>,
    /// Mesh-space → bone-space transforms at bind time, parallel to `bones`.
    /// Multiply by the bone's current world matrix to get the skinning
    /// matrix for the palette.
    pub bind_inverses: Vec<Mat4>,
    /// `NiSkinData::skinTransform` after Y-up conversion (the per-skin
    /// global transform). M41.0 Phase 1b.x — Bethesda body NIFs ship a
    /// non-identity cyclic-permutation rotation here that the
    /// pre-Phase-1b.x palette computation dropped, producing the
    /// stretched-ribbon vertex artifact. `Mat4::IDENTITY` for FO4+
    /// BSSkin paths that don't carry the field.
    pub global_skin_transform: Mat4,
}

impl SkinnedMesh {
    /// Create a new skinned-mesh binding. Panics if `bones.len() != bind_inverses.len()`
    /// or if the bone count exceeds `MAX_BONES_PER_MESH`.
    pub fn new(
        skeleton_root: Option<EntityId>,
        bones: Vec<Option<EntityId>>,
        bind_inverses: Vec<Mat4>,
    ) -> Self {
        Self::new_with_global(skeleton_root, bones, bind_inverses, Mat4::IDENTITY)
    }

    /// Construct with an explicit `global_skin_transform`
    /// (`NiSkinData::skinTransform`). Pass `Mat4::IDENTITY` to match the
    /// legacy (pre-Phase-1b.x) behaviour.
    pub fn new_with_global(
        skeleton_root: Option<EntityId>,
        bones: Vec<Option<EntityId>>,
        bind_inverses: Vec<Mat4>,
        global_skin_transform: Mat4,
    ) -> Self {
        assert_eq!(
            bones.len(),
            bind_inverses.len(),
            "SkinnedMesh: bones and bind_inverses must be parallel"
        );
        assert!(
            bones.len() <= MAX_BONES_PER_MESH,
            "SkinnedMesh: {} bones exceeds MAX_BONES_PER_MESH ({})",
            bones.len(),
            MAX_BONES_PER_MESH
        );
        Self {
            skeleton_root,
            bones,
            bind_inverses,
            global_skin_transform,
        }
    }

    /// Number of bones this mesh binds to.
    pub fn bone_count(&self) -> usize {
        self.bones.len()
    }

    /// Compute the per-frame skinning matrix palette.
    ///
    /// For each bone, looks up its world transform via `world_transform_of`
    /// and multiplies by the baked `bind_inverse`. Missing bones (unresolved
    /// `None`, or a bone whose entity has no `GlobalTransform`) fall back to
    /// the identity matrix so the vertex renders in bind pose rather than
    /// collapsing to the origin.
    ///
    /// The closure takes an `EntityId` and returns its world-space matrix;
    /// this indirection keeps the function unit-testable without needing a
    /// full `World` — see the tests below.
    pub fn compute_palette<F>(&self, world_transform_of: F) -> Vec<Mat4>
    where
        F: FnMut(EntityId) -> Option<Mat4>,
    {
        let mut out = Vec::with_capacity(self.bones.len());
        self.compute_palette_into(&mut out, world_transform_of);
        out
    }

    /// Compute the palette into a caller-owned buffer, avoiding per-call
    /// heap allocation. The buffer is cleared and filled with one `Mat4`
    /// per bone. Callers should `.clear()` their scratch Vec between
    /// entities (this method does it internally).
    pub fn compute_palette_into<F>(&self, out: &mut Vec<Mat4>, mut world_transform_of: F)
    where
        F: FnMut(EntityId) -> Option<Mat4>,
    {
        out.clear();
        out.extend(self.bones.iter().zip(self.bind_inverses.iter()).map(
            |(maybe_bone, bind_inv)| {
                let Some(bone) = maybe_bone else {
                    return Mat4::IDENTITY;
                };
                match world_transform_of(*bone) {
                    Some(world) => world * *bind_inv,
                    None => Mat4::IDENTITY,
                }
            },
        ));
    }
}

impl Component for SkinnedMesh {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Quat, Vec3};

    fn identity_bind(n: usize) -> Vec<Mat4> {
        vec![Mat4::IDENTITY; n]
    }

    #[test]
    fn new_enforces_parallel_vectors() {
        let sm = SkinnedMesh::new(None, vec![Some(1), Some(2)], identity_bind(2));
        assert_eq!(sm.bone_count(), 2);
    }

    #[test]
    #[should_panic(expected = "must be parallel")]
    fn new_panics_on_length_mismatch() {
        SkinnedMesh::new(None, vec![Some(1)], identity_bind(2));
    }

    #[test]
    #[should_panic(expected = "exceeds MAX_BONES_PER_MESH")]
    fn new_panics_on_too_many_bones() {
        let bones = vec![Some(0_u32); MAX_BONES_PER_MESH + 1];
        let binds = identity_bind(MAX_BONES_PER_MESH + 1);
        SkinnedMesh::new(None, bones, binds);
    }

    #[test]
    fn palette_identity_when_all_bind_inverse_identity_and_world_identity() {
        let sm = SkinnedMesh::new(None, vec![Some(10), Some(20)], identity_bind(2));
        let palette = sm.compute_palette(|_| Some(Mat4::IDENTITY));
        assert_eq!(palette.len(), 2);
        for m in &palette {
            assert_eq!(*m, Mat4::IDENTITY);
        }
    }

    #[test]
    fn palette_uses_identity_fallback_for_unresolved_bone() {
        let sm = SkinnedMesh::new(None, vec![None, Some(5)], identity_bind(2));
        let palette = sm.compute_palette(|e| {
            assert_eq!(e, 5);
            Some(Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)))
        });
        assert_eq!(palette[0], Mat4::IDENTITY);
        assert_eq!(palette[1], Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)));
    }

    #[test]
    fn palette_uses_identity_fallback_when_world_lookup_fails() {
        let sm = SkinnedMesh::new(None, vec![Some(42)], identity_bind(1));
        let palette = sm.compute_palette(|_| None);
        assert_eq!(palette[0], Mat4::IDENTITY);
    }

    #[test]
    fn palette_applies_world_times_bind_inverse() {
        // Bone's bind pose: translated to (10, 0, 0). Bind inverse moves
        // world back to origin.
        let bind_inv = Mat4::from_translation(Vec3::new(-10.0, 0.0, 0.0));
        let sm = SkinnedMesh::new(None, vec![Some(1)], vec![bind_inv]);

        // Current world: translated to (12, 0, 0). Skinning matrix should
        // be (world * bind_inverse) = translate by (+2, 0, 0).
        let world = Mat4::from_translation(Vec3::new(12.0, 0.0, 0.0));
        let palette = sm.compute_palette(|_| Some(world));

        let expected = Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0));
        let diff = palette[0].to_cols_array();
        let exp = expected.to_cols_array();
        for (a, b) in diff.iter().zip(exp.iter()) {
            assert!((a - b).abs() < 1e-5, "mismatch: {} vs {}", a, b);
        }
    }

    #[test]
    fn palette_applies_world_times_bind_inverse_with_rotation() {
        // Bone bound at rotation=identity. Bind inverse = identity.
        // Current world rotated 90° around Y.
        let bind_inv = Mat4::IDENTITY;
        let sm = SkinnedMesh::new(None, vec![Some(1)], vec![bind_inv]);

        let world = Mat4::from_quat(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        let palette = sm.compute_palette(|_| Some(world));

        // A point at (1, 0, 0) in bone space should go to (0, 0, -1).
        let pt = palette[0] * Vec3::new(1.0, 0.0, 0.0).extend(1.0);
        assert!(pt.x.abs() < 1e-5);
        assert!((pt.z + 1.0).abs() < 1e-5);
    }
}
