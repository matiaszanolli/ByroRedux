//! NiDefaultAVObjectPalette — maps object names to scene graph nodes.
//!
//! Used by NiControllerManager to bind animation sequences to scene
//! graph objects by name.

use crate::stream::NifStream;
use crate::types::BlockRef;
use crate::NiObject;
use std::any::Any;
use std::io;

/// An entry in the object palette: name → block reference.
#[derive(Debug)]
pub struct AVObject {
    pub name: String,
    pub av_object_ref: BlockRef,
}

/// Maps node names to scene graph objects for animation binding.
///
/// Referenced by NiControllerManager via `object_palette_ref`.
/// Each entry associates a string name with a NiAVObject block index.
#[derive(Debug)]
pub struct NiDefaultAVObjectPalette {
    pub scene_ref: BlockRef,
    pub objs: Vec<AVObject>,
}

impl NiObject for NiDefaultAVObjectPalette {
    fn block_type_name(&self) -> &'static str {
        "NiDefaultAVObjectPalette"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiDefaultAVObjectPalette {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let scene_ref = stream.read_block_ref()?;
        let num_objs = stream.read_u32_le()?;
        let mut objs = stream.allocate_vec(num_objs)?;
        for _ in 0..num_objs {
            let name = stream.read_sized_string()?;
            let av_object_ref = stream.read_block_ref()?;
            objs.push(AVObject {
                name,
                av_object_ref,
            });
        }
        Ok(Self { scene_ref, objs })
    }

    /// Look up a block reference by name.
    pub fn find_by_name(&self, name: &str) -> Option<BlockRef> {
        self.objs
            .iter()
            .find(|obj| obj.name == name)
            .map(|obj| obj.av_object_ref)
    }
}
