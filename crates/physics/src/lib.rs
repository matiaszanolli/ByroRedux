//! ByroRedux physics — Rapier3D-backed simulation layer.
//!
//! Reads the `CollisionShape` / `RigidBodyData` components that the NIF
//! importer already attaches to entities, spawns matching Rapier bodies
//! and colliders, steps the simulation with a fixed-timestep accumulator,
//! and writes dynamic body poses back into the ECS `Transform`.
//!
//! # Crate layout
//!
//! - [`convert`] — glam ↔ nalgebra conversions + `collision_shape_to_shared_shape`
//! - [`components`] — `RapierHandles`, `PlayerBody`
//! - [`world`] — `PhysicsWorld` resource (pipeline, sets, accumulator)
//! - [`sync`] — `physics_sync_system` (4-phase per-tick sync)
//!
//! The crate is additive: if `PhysicsWorld` is not inserted into the
//! world, nothing happens. The loose-NIF demo path opts out this way.

pub mod components;
pub mod convert;
pub mod sync;
pub mod world;

pub use components::{PlayerBody, RapierHandles};
pub use sync::{physics_sync_system, set_linear_velocity};
pub use world::PhysicsWorld;
