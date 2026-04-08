//! Entity-Component-System with pluggable storage backends.
//!
//! Components declare their preferred storage via `Component::Storage`.
//! Two built-in backends:
//! - [`SparseSetStorage`] — O(1) insert/remove, dense iteration (default)
//! - [`PackedStorage`] — sorted by entity, cache-friendly iteration (opt-in)

pub mod components;
mod lock_tracker;
pub mod packed;
pub mod query;
pub mod resource;
pub mod resources;
pub mod scheduler;
pub mod sparse_set;
pub mod storage;
pub mod system;
pub mod world;

pub use components::{
    ActiveCamera, AnimatedAlpha, AnimatedColor, AnimatedVisibility, Camera, Children,
    GlobalTransform, LightSource, Material, MeshHandle, Name, Parent, SkinnedMesh, TextureHandle,
    Transform, MAX_BONES_PER_MESH,
};
pub use packed::PackedStorage;
pub use query::{ComponentRef, QueryRead, QueryWrite};
pub use resource::{Resource, ResourceRead, ResourceWrite};
pub use resources::{DebugStats, DeltaTime, EngineConfig, TotalTime};
pub use scheduler::Scheduler;
pub use sparse_set::SparseSetStorage;
pub use storage::{Component, ComponentStorage, EntityId};
pub use system::System;
pub use world::World;
