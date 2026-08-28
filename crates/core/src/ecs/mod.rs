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
    AnimatedTextureFlip, AnimatedUvTransform, AnimatedVisibility, BSBound, BSXFlags, Billboard,
    BillboardMode, Camera, CellFormId, CellRoot, Children, CombustionState, EmitterShape,
    FogBounds, FogProfile, FogShape, FogSource, FogVolume, GlobalTransform, LightFlicker,
    LightKind, LightSource, LocalBound, Material, MeshHandle, Name, Parent, ParticleEmitter,
    ParticleForceField, ParticleSoA, RenderLayer, SceneFlags, SkinnedMesh, SpeedTreeWind,
    TextureFlipEntry, TextureHandle, Transform, WorldBound, LIGHT_FLAG_FLICKER,
    LIGHT_FLAG_FLICKER_SLOW, LIGHT_FLAG_PULSE, LIGHT_FLAG_PULSE_SLOW, LIGHT_FLAG_SHADOW_HEMISPHERE,
    LIGHT_FLAG_SHADOW_MASK, LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL, LIGHT_FLAG_SHADOW_SPOTLIGHT,
    LIGHT_FLAG_SPOT, MAX_BONES_PER_MESH, MAX_PARTICLES_CEILING,
};
pub use debug_load::{PendingDebugLoad, PendingDebugLoadSlot, PendingUpscalerSwitch};
pub use game_profiles::{GameProfileEntry, GameProfileRegistry};
pub use metrics::MetricsSnapshot;
pub use packed::PackedStorage;
pub use query::{ComponentRef, QueryRead, QueryWrite};
pub use resource::{Resource, ResourceRead, ResourceWrite};
pub use resources::{
    format_gpu_bracket_ms, CpuFrameTimings, DebugStats, DeltaTime, EngineConfig, FindingKind,
    ImageHealth, LodCoverageStats, OwnerClass, OwnershipFinding, OwnershipSnapshot,
    OwnershipTelemetry, OwnershipTracker, ReclaimPolicy, RtIntegrityStats, SchedulerAccessReport,
    ScratchRow, ScratchTelemetry, ScreenshotBridge, SelectedRef, SkinCoverageStats, SystemList,
    TerrainSeamStats, TotalTime, UpscalerTelemetry,
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
