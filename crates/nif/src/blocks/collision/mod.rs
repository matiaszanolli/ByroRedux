//! Havok collision block parsers (bhk* types).
//!
//! These blocks form the collision geometry tree in Bethesda NIF files.
//! The pipeline: bhkCollisionObject → bhkRigidBody → shape tree.
//! Parsed data feeds a physics-agnostic ECS representation (CollisionShape),
//! which will be converted to Rapier colliders in the physics system (M28).
//!
//! ## Module layout
//!
//! Split out of the 2 184-LOC monolith into per-topic submodules:
//!
//! - [`collision_object`] — collision-object wrappers (Bhk / BhkNP / BhkP / SystemBinary)
//! - [`rigid_body`] — `BhkRigidBody`
//! - [`ragdoll`] — bone-pose + ragdoll templates (FO3+)
//! - [`shape_primitive`] — Sphere, MultiSphere, Box, Capsule, Cylinder
//! - [`shape_compound`] — Convex, List, Transform, MoppBvTree, ConvexList
//! - [`shape_mesh`] — NiTriStrips + PackedNiTriStrips + per-strip data
//! - [`compressed_mesh`] — Skyrim+ `BhkCompressedMeshShape` + data
//! - [`constraints`] — `BhkConstraint`, `BhkBreakableConstraint`
//! - [`phantom_action`] — phantoms + LiquidAction + OrientHingedBodyAction
//!
//! Every block type is `pub use`-d at this module's root so external
//! callers (`crate::blocks::collision::TypeName`) keep working
//! unchanged.

mod collision_object;
mod compressed_mesh;
mod constraints;
mod phantom_action;
mod ragdoll;
mod rigid_body;
mod shape_compound;
mod shape_mesh;
mod shape_primitive;

pub use collision_object::{
    BhkCollisionObject, BhkNPCollisionObject, BhkPCollisionObject, BhkSystemBinary,
    NiCollisionObjectBase,
};
pub use compressed_mesh::{
    BhkCompressedMeshShape, BhkCompressedMeshShapeData, CmsBigTri, CmsChunk, CmsTransform,
};
pub use constraints::{BhkBreakableConstraint, BhkConstraint};
pub use phantom_action::{
    BhkAabbPhantom, BhkLiquidAction, BhkOrientHingedBodyAction, BhkSimpleShapePhantom,
};
pub use ragdoll::{BhkPoseArray, BhkRagdollTemplate, BhkRagdollTemplateData, BonePose, BoneTransform};
pub use rigid_body::BhkRigidBody;
pub use shape_compound::{
    BhkConvexListShape, BhkConvexVerticesShape, BhkListShape, BhkMoppBvTreeShape, BhkTransformShape,
};
pub use shape_mesh::{
    BhkNiTriStripsShape, BhkPackedNiTriStripsShape, HkPackedNiTriStripsData, HkSubPartData,
    PackedTriangle,
};
pub use shape_primitive::{
    BhkBoxShape, BhkCapsuleShape, BhkCylinderShape, BhkMultiSphereShape, BhkSphereShape,
};

use crate::stream::NifStream;
use crate::version::NifVersion;
use std::io;

// ── Shared low-level readers ────────────────────────────────────────
//
// Each sibling submodule imports these via `use super::{...}`. Private
// `fn` is fine — Rust visibility makes private items defined in this
// module reachable from its descendants.

fn read_havok_material(stream: &mut NifStream) -> io::Result<u32> {
    if stream.version() <= NifVersion::V10_0_1_2 {
        let _unknown_int = stream.read_u32_le()?;
    }
    stream.read_u32_le()
}

fn read_vec4(stream: &mut NifStream) -> io::Result<[f32; 4]> {
    Ok([
        stream.read_f32_le()?,
        stream.read_f32_le()?,
        stream.read_f32_le()?,
        stream.read_f32_le()?,
    ])
}

fn read_matrix3(stream: &mut NifStream) -> io::Result<[f32; 12]> {
    let mut m = [0.0f32; 12];
    for val in &mut m {
        *val = stream.read_f32_le()?;
    }
    Ok(m)
}

#[cfg(test)]
mod bhk_blend_collision_object_tests;
#[cfg(test)]
mod bhk_breakable_constraint_tests;
#[cfg(test)]
mod bhk_ragdoll_tests;
#[cfg(test)]
mod bhk_rigid_body_tests;
#[cfg(test)]
mod hk_packed_ni_tri_strips_data_tests;
