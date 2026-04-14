//! Texture handle component — lightweight reference to GPU-side texture.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Handle to a texture stored in the renderer's TextureRegistry.
///
/// The actual GPU image/sampler/descriptor sets live in the renderer — this
/// component is just a lightweight u32 index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct TextureHandle(pub u32);

impl Component for TextureHandle {
    type Storage = SparseSetStorage<Self>;
}
