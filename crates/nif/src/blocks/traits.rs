//! Consumer traits for NIF block queries.
//!
//! These traits let import.rs and other consumers query block properties
//! without knowing concrete types or using downcast_ref chains.

use crate::types::{BlockRef, NiTransform};
use std::sync::Arc;

/// Any block with NiObjectNET fields (name, extra data, controller).
pub trait HasObjectNET {
    fn name(&self) -> Option<&str>;
    fn extra_data_refs(&self) -> &[BlockRef];
    fn controller_ref(&self) -> BlockRef;

    /// Borrow the underlying `Arc<str>` name storage if the
    /// implementor stores names as refcounted strings (the default
    /// for every block backed by `NiObjectNETData`). Resolvers that
    /// build `Vec<Arc<str>>` use this to refcount-bump rather than
    /// allocate a fresh `Arc<str>` from the `&str` form. See #872.
    ///
    /// Default returns `None` — implementors that don't expose the
    /// `Arc<str>` directly fall back to the cloning path.
    fn name_arc(&self) -> Option<&Arc<str>> {
        None
    }
}

/// Any block with NiAVObject fields (scene graph participant with transform).
pub trait HasAVObject: HasObjectNET {
    fn flags(&self) -> u32;
    fn transform(&self) -> &NiTransform;
    fn properties(&self) -> &[BlockRef];
    fn collision_ref(&self) -> BlockRef;
}

/// Any block that provides direct shader + alpha property references.
/// Implemented by NiTriShape (Skyrim+) and BSTriShape.
pub trait HasShaderRefs {
    fn shader_property_ref(&self) -> BlockRef;
    fn alpha_property_ref(&self) -> BlockRef;
}
