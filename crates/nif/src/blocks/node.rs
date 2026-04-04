//! NiNode — scene graph parent node.
//!
//! NiNode is the fundamental grouping object: it has a transform,
//! a list of children (NiAVObject refs), and a list of properties.

use crate::stream::NifStream;
use crate::types::{BlockRef, NiTransform};
use super::base::NiAVObjectData;
use super::traits::{HasAVObject, HasObjectNET};
use super::NiObject;
use std::any::Any;
use std::io;

/// Scene graph node (NiNode, BSFadeNode, etc.).
#[derive(Debug)]
pub struct NiNode {
    /// NiObjectNET + NiAVObject base fields.
    pub av: NiAVObjectData,
    /// NiNode-specific: child node/geometry references.
    pub children: Vec<BlockRef>,
    /// NiNode-specific: dynamic effect references (removed in FO4+).
    pub effects: Vec<BlockRef>,

    // Public accessors for backward compatibility with existing code
    // that accesses fields directly. These will be removed once all
    // consumers migrate to trait-based access.
}

// Convenience accessors for direct field access (backward compat).
impl NiNode {
    pub fn name(&self) -> Option<&str> { self.av.net.name.as_deref() }
    pub fn flags(&self) -> u32 { self.av.flags }
    pub fn transform(&self) -> &NiTransform { &self.av.transform }
    pub fn collision_ref(&self) -> BlockRef { self.av.collision_ref }
    pub fn properties(&self) -> &[BlockRef] { &self.av.properties }
    pub fn extra_data_refs(&self) -> &[BlockRef] { &self.av.net.extra_data_refs }
    pub fn controller_ref(&self) -> BlockRef { self.av.net.controller_ref }
}

impl NiObject for NiNode {
    fn block_type_name(&self) -> &'static str {
        "NiNode"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_object_net(&self) -> Option<&dyn HasObjectNET> { Some(self) }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> { Some(self) }
}

impl HasObjectNET for NiNode {
    fn name(&self) -> Option<&str> { self.av.net.name.as_deref() }
    fn extra_data_refs(&self) -> &[BlockRef] { &self.av.net.extra_data_refs }
    fn controller_ref(&self) -> BlockRef { self.av.net.controller_ref }
}

impl HasAVObject for NiNode {
    fn flags(&self) -> u32 { self.av.flags }
    fn transform(&self) -> &NiTransform { &self.av.transform }
    fn properties(&self) -> &[BlockRef] { &self.av.properties }
    fn collision_ref(&self) -> BlockRef { self.av.collision_ref }
}

impl NiNode {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let av = NiAVObjectData::parse(stream)?;

        // NiNode-specific fields
        let children = stream.read_block_ref_list()?;
        // FO4+ removes the effects list from NiNode (BSVER >= 130).
        let effects = if stream.variant().has_effects_list() {
            stream.read_block_ref_list()?
        } else {
            Vec::new()
        };

        Ok(Self {
            av,
            children,
            effects,
        })
    }
}
