//! NifScene — resolved scene graph after linking.

use crate::blocks::NiObject;

/// A fully parsed and linked NIF file.
#[derive(Debug)]
pub struct NifScene {
    /// All parsed blocks in file order.
    pub blocks: Vec<Box<dyn NiObject>>,
    /// Index of the root block (typically first NiNode).
    pub root_index: Option<usize>,
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
