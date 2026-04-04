//! Consumer traits for NIF block queries.
//!
//! These traits let import.rs and other consumers query block properties
//! without knowing concrete types or using downcast_ref chains.

use crate::types::{BlockRef, NiTransform};

/// Any block with NiObjectNET fields (name, extra data, controller).
pub trait HasObjectNET {
    fn name(&self) -> Option<&str>;
    fn extra_data_refs(&self) -> &[BlockRef];
    fn controller_ref(&self) -> BlockRef;
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
