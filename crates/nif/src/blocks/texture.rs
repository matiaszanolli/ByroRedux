//! NiSourceTexture — texture file reference.

use crate::stream::NifStream;
use crate::types::BlockRef;
use super::NiObject;
use std::any::Any;
use std::io;

/// Reference to an external texture file or embedded pixel data.
#[derive(Debug)]
pub struct NiSourceTexture {
    pub name: Option<String>,
    pub extra_data_refs: Vec<BlockRef>,
    pub controller_ref: BlockRef,
    pub use_external: bool,
    pub filename: Option<String>,
    pub pixel_data_ref: BlockRef,
    pub pixel_layout: u32,
    pub use_mipmaps: u32,
    pub alpha_format: u32,
    pub is_static: bool,
}

impl NiObject for NiSourceTexture {
    fn block_type_name(&self) -> &'static str {
        "NiSourceTexture"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiSourceTexture {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiObjectNET
        let name = stream.read_string()?;
        let extra_data_refs = stream.read_block_ref_list()?;
        let controller_ref = stream.read_block_ref()?;

        let use_external = stream.read_u8()? != 0;

        let (filename, pixel_data_ref) = if use_external {
            let fname = stream.read_sized_string()?;
            // Unknown link in newer versions
            if stream.version() >= crate::version::NifVersion(0x0A010000) {
                let _unknown_ref = stream.read_block_ref()?;
            }
            (Some(fname), BlockRef::NULL)
        } else {
            // Internal texture — skip unknown byte, read pixel data ref
            if stream.version() >= crate::version::NifVersion(0x0A010000) {
                let _unknown = stream.read_sized_string()?;
            }
            let pix_ref = stream.read_block_ref()?;
            (None, pix_ref)
        };

        let pixel_layout = stream.read_u32_le()?;
        let use_mipmaps = stream.read_u32_le()?;
        let alpha_format = stream.read_u32_le()?;
        let is_static = stream.read_u8()? != 0;

        // Direct render flag in newer versions
        if stream.version() >= crate::version::NifVersion(0x0A010006) {
            let _direct_render = stream.read_bool()?;
        }

        // Persist render data flag (version >= 20.2.0.7)
        if stream.version() >= crate::version::NifVersion::V20_2_0_7 {
            let _persist_render_data = stream.read_bool()?;
        }

        Ok(Self {
            name,
            extra_data_refs,
            controller_ref,
            use_external,
            filename,
            pixel_data_ref,
            pixel_layout,
            use_mipmaps,
            alpha_format,
            is_static,
        })
    }
}
