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

pub use components::{
    ActorBoneCollider, ActorColliderOwner, CharacterController, Ragdoll, RapierHandles,
};
pub use config::{ContactConfig, TriMeshFlagBits};
pub use ragdoll::{
    build_ragdoll, RagdollBodySpec, RagdollConstraintSpec, RagdollJointSpec, RagdollSpec,
};
pub use sync::{
    dump_spawn_collider_census, physics_sync_system, set_kinematic_translation,
    set_linear_velocity, SpawnCensusAuthoring, SpawnCensusEntry, SpawnCensusProbe,
    SpawnProbeVerdict,
};
pub use water::{
    authored_wave_height_with_weather, buoyancy_force, submerged_fraction,
    weather_wave_adjustment, wind_force, PhysicsWaterConstants,
};
pub use world::{
    CharacterMoveParams, CharacterMoveResult, NearbyCollider, PhysicsRayHit, PhysicsWorld,
    ACTOR_BONE_GROUP, PHYSICS_DT,
};
