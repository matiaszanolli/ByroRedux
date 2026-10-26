//! Public ECS-side data types produced by the NIF import pipeline.
//!
//! `ImportedScene` is the top-level result of [`super::import_nif_scene`].
//! It carries a tree of `ImportedNode`s (preserving NIF hierarchy via
//! parent indices), a flat list of `ImportedMesh` leaves, plus the
//! lights / texture effects / particle emitters / skinning data
//! extracted along the way.
//!
//! All transforms are local (relative to parent). Vertex / index data
//! is stored as plain `Vec<Vertex>` / `Vec<u32>` ready for GPU upload
//! via `MeshRegistry::upload()`.

use super::material::{BsEffectShaderData, NoLightingFalloff};
use byroredux_core::ecs::components::collision::{CollisionShape, RigidBodyData};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::FixedString;
use std::sync::Arc;

mod light;
mod material;
mod mesh;
mod node;
mod particles;
mod ragdoll;
mod scene;

pub use light::*;
pub use material::*;
pub use mesh::*;
pub use node::*;
pub use particles::*;
pub use ragdoll::*;
pub use scene::*;
