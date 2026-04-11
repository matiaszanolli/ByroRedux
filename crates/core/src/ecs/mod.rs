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
pub mod systems;
pub mod world;

pub use components::{
    ActiveCamera, AnimatedAlpha, AnimatedColor, AnimatedVisibility, BSBound, BSXFlags, Billboard,
    BillboardMode, Camera, Children, GlobalTransform, LightSource, LocalBound, Material,
    MeshHandle, Name, Parent, SkinnedMesh, TextureHandle, Transform, WorldBound,
    MAX_BONES_PER_MESH,
};
pub use packed::PackedStorage;
pub use query::{ComponentRef, QueryRead, QueryWrite};
pub use resource::{Resource, ResourceRead, ResourceWrite};
pub use resources::{DebugStats, DeltaTime, EngineConfig, TotalTime};
pub use scheduler::{Scheduler, Stage};
pub use sparse_set::SparseSetStorage;
pub use storage::{Component, ComponentStorage, EntityId};
pub use system::System;
pub use systems::make_transform_propagation_system;
pub use world::World;
