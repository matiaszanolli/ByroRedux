//! ByroRedux physics — Rapier3D-backed simulation layer.
//!
//! Reads the `CollisionShape` / `RigidBodyData` components that the NIF
//! importer already attaches to entities, spawns matching Rapier bodies
//! and colliders, steps the simulation with a fixed-timestep accumulator,
//! and writes dynamic body poses back into the ECS `Transform`.
//!
//! # Crate layout
//!
//! - [`config`] — `ContactConfig` resource (engine-wide tunables)
//! - [`convert`] — glam ↔ nalgebra conversions + `collision_shape_to_parts`
//! - [`components`] — `RapierHandles`, `CharacterController`
//! - [`world`] — `PhysicsWorld` resource (pipeline, sets, accumulator)
//! - [`water`] — WATAL physics sink: `PhysicsWaterConstants` + `buoyancy_force`
//! - [`sync`] — `physics_sync_system` (4-phase per-tick sync)
//!
//! The crate is additive: if `PhysicsWorld` is not inserted into the
//! world, nothing happens. The loose-NIF demo path opts out this way.

pub mod components;
pub mod config;
pub mod convert;
pub mod ragdoll;
pub mod sync;
pub mod water;
pub mod world;

pub use components::{CharacterController, Ragdoll, RapierHandles};
pub use config::{ContactConfig, TriMeshFlagBits};
pub use ragdoll::{
    build_ragdoll, RagdollBodySpec, RagdollConstraintSpec, RagdollJointSpec, RagdollSpec,
};
pub use sync::{physics_sync_system, set_kinematic_translation, set_linear_velocity};
pub use water::{buoyancy_force, submerged_fraction, PhysicsWaterConstants};
pub use world::{CharacterMoveParams, CharacterMoveResult, PhysicsWorld, PHYSICS_DT};
