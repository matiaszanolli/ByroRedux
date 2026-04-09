//! NifScene — resolved scene graph after linking.

use crate::blocks::NiObject;

/// A fully parsed and linked NIF file.
#[derive(Debug)]
pub struct NifScene {
    /// All parsed blocks in file order.
    pub blocks: Vec<Box<dyn NiObject>>,
    /// Index of the root block (typically first NiNode).
    pub root_index: Option<usize>,
    /// `true` when the parse loop aborted early because a block
    /// parser returned `Err` on a NIF file without per-block sizes
    /// (Oblivion era) — any blocks after the failure point are
    /// missing from `blocks` and the scene graph may reference
    /// unreachable indices. The first NiNode heuristic for
    /// `root_index` may also pick a subtree rather than the real
    /// root when truncation hits before the scene root. Consumers
    /// that need complete scenes should treat this as a hard error;
    /// consumers that can tolerate partial geometry (e.g. cell
    /// loaders doing best-effort import) can ignore it. See #175.
    pub truncated: bool,
}

impl NifScene {
    /// Get a block by index.
    pub fn get(&self, index: usize) -> Option<&dyn NiObject> {
        self.blocks.get(index).map(|b| b.as_ref())
    }

    /// Get a block by index, downcasted to a concrete type.
    pub fn get_as<T: 'static>(&self, index: usize) -> Option<&T> {
        self.blocks.get(index)?.as_any().downcast_ref::<T>()
    }

    /// Get the root block (typically NiNode).
    pub fn root(&self) -> Option<&dyn NiObject> {
        self.root_index.and_then(|i| self.get(i))
    }

    /// Number of blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}
