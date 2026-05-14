//! Per-frame scene data buffers for multi-light rendering.
//!
//! Manages SSBOs (instance / light / bone / material / terrain / indirect)
//! and UBOs (camera) for the scene pass, double-buffered per
//! frame-in-flight to avoid write-after-read hazards. Bound as
//! descriptor set 1 in the pipeline layout.
//!
//! ## Module layout
//!
//! Split out of the 2 334-LOC monolith into:
//!
//! - [`constants`]   — `MAX_*` ceilings, `INSTANCE_FLAG_*`, material kinds
//! - [`gpu_types`]   — `#[repr(C)]` shader-contract structs (`GpuInstance`, `GpuLight`, etc.)
//! - [`buffers`]     — `SceneBuffers` storage + `new` + accessors + `destroy`
//! - [`upload`]      — per-SSBO upload-and-flush routines
//! - [`descriptors`] — descriptor-set writes for AO / GBuffer / cluster / TLAS
//!
//! Tests live as `#[cfg(test)]` siblings under the same directory.

mod buffers;
mod constants;
mod descriptors;
mod gpu_types;
mod upload;

pub use constants::*;
pub use gpu_types::{GpuCamera, GpuInstance, GpuLight, GpuTerrainTile};
pub use buffers::SceneBuffers;

#[cfg(test)]
mod gpu_instance_layout_tests;
#[cfg(test)]
mod material_hash_tests;
#[cfg(test)]
mod scene_descriptor_reflection_tests;
