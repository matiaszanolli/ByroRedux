//! Entity-Component-System with pluggable storage backends.
//!
//! Components declare their preferred storage via `Component::Storage`.
//! Two built-in backends:
//! - [`SparseSetStorage`] — O(1) insert/remove, dense iteration (default)
//! - [`PackedStorage`] — sorted by entity, cache-friendly iteration (opt-in)

pub mod access;
pub mod components;
pub mod debug_load;
pub mod game_profiles;
mod lock_tracker;
pub mod metrics;
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

pub use access::{analyze_pair, Access, AccessConflict, AccessEntry, ConflictKind, ConflictPair};
pub use components::{
    ActiveCamera, AnimatedAlpha, AnimatedAmbientColor, AnimatedDiffuseColor, AnimatedEmissiveColor,
    AnimatedMorphWeights, AnimatedShaderColor, AnimatedShaderFloat, AnimatedSpecularColor,
    AnimatedUvTransform, AnimatedVisibility, BSBound, BSXFlags, Billboard, BillboardMode, Camera,
    CellRoot, Children, EmitterShape, GlobalTransform, LightFlicker, LightSource, LocalBound,
    Material, MeshHandle, Name, Parent, ParticleEmitter, ParticleForceField, ParticleSoA,
    RenderLayer, SceneFlags, SkinnedMesh, TextureHandle, Transform, WorldBound, LIGHT_FLAG_FLICKER,
    LIGHT_FLAG_FLICKER_SLOW, LIGHT_FLAG_PULSE, LIGHT_FLAG_PULSE_SLOW, MAX_BONES_PER_MESH,
};
pub use debug_load::{PendingDebugLoad, PendingDebugLoadSlot};
pub use game_profiles::{GameProfileEntry, GameProfileRegistry};
pub use metrics::MetricsSnapshot;
pub use packed::PackedStorage;
pub use query::{ComponentRef, QueryRead, QueryWrite};
pub use resource::{Resource, ResourceRead, ResourceWrite};
pub use resources::{
    CpuFrameTimings, DebugStats, DeltaTime, EngineConfig, SchedulerAccessReport, ScratchRow,
    ScratchTelemetry, ScreenshotBridge, SelectedRef, SkinCoverageStats, SystemList, TotalTime,
};
pub use scheduler::{
    AccessReport, Scheduler, SchedulerSystemTimings, Stage, StageConflictRow, StageReport,
    SystemAccessRow,
};
pub use sparse_set::SparseSetStorage;
pub use storage::DynStorage;
pub use storage::{Component, ComponentStorage, EntityId};
pub use system::System;
pub use systems::make_transform_propagation_system;
pub use world::World;
