//! Mesh handle component — lightweight reference to GPU-side geometry.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Handle to a mesh stored in the renderer's MeshRegistry.
///
/// The actual GPU vertex/index buffers live in the renderer — this
/// component is just a lightweight u32 index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshHandle(pub u32);

impl Component for MeshHandle {
    type Storage = SparseSetStorage<Self>;
}
