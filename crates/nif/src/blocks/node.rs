//! NiNode — scene graph parent node.
//!
//! NiNode is the fundamental grouping object: it has a transform,
//! a list of children (NiAVObject refs), and a list of properties.

use crate::stream::NifStream;
use crate::types::{BlockRef, NiTransform};
use super::NiObject;
use std::any::Any;
use std::io;

/// Scene graph node (NiNode, BSFadeNode, etc.).
#[derive(Debug)]
pub struct NiNode {
    pub name: Option<String>,
    pub extra_data_refs: Vec<BlockRef>,
    pub controller_ref: BlockRef,
    pub flags: u32,
    pub transform: NiTransform,
    pub properties: Vec<BlockRef>,
    pub collision_ref: BlockRef,
    pub children: Vec<BlockRef>,
    pub effects: Vec<BlockRef>,
}

impl NiObject for NiNode {
    fn block_type_name(&self) -> &'static str {
        "NiNode"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiNode {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiObjectNET fields
        let name = stream.read_string()?;
        let extra_data_refs = stream.read_block_ref_list()?;
        let controller_ref = stream.read_block_ref()?;

        // NiAVObject fields
        let flags = if stream.version() >= crate::version::NifVersion::V20_2_0_7 {
            stream.read_u32_le()?
        } else {
            stream.read_u16_le()? as u32
        };
        let transform = stream.read_ni_transform()?;
        let properties = stream.read_block_ref_list()?;
        let collision_ref = stream.read_block_ref()?;

        // NiNode-specific fields
        let children = stream.read_block_ref_list()?;
        let effects = stream.read_block_ref_list()?;

        Ok(Self {
            name,
            extra_data_refs,
            controller_ref,
            flags,
            transform,
            properties,
            collision_ref,
            children,
            effects,
        })
    }
}
