//! NiSourceTexture — texture file reference.

use crate::stream::NifStream;
use crate::types::BlockRef;
use super::base::NiObjectNETData;
use super::NiObject;
use std::any::Any;
use std::io;

/// Reference to an external texture file or embedded pixel data.
#[derive(Debug)]
pub struct NiSourceTexture {
    pub net: NiObjectNETData,
    pub use_external: bool,
    pub filename: Option<String>,
    pub pixel_data_ref: BlockRef,
    pub pixel_layout: u32,
    pub use_mipmaps: u32,
    pub alpha_format: u32,
    pub is_static: bool,
}

impl NiObject for NiSourceTexture {
    fn block_type_name(&self) -> &'static str { "NiSourceTexture" }
    fn as_any(&self) -> &dyn Any { self }
}

impl NiSourceTexture {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;

        let use_external = stream.read_u8()? != 0;
        let use_string_table = stream.version() >= crate::version::NifVersion::V20_2_0_7;

        let (filename, pixel_data_ref) = if use_external {
            let fname = if use_string_table {
                stream.read_string()?
            } else {
                Some(stream.read_sized_string()?)
            };
            if stream.version() >= crate::version::NifVersion(0x0A010000) {
                let _unknown_ref = stream.read_block_ref()?;
            }
            (fname, BlockRef::NULL)
        } else {
            if stream.version() >= crate::version::NifVersion(0x0A010000) {
                if use_string_table {
                    let _unknown = stream.read_string()?;
                } else {
                    let _unknown = stream.read_sized_string()?;
                }
            }
            let pix_ref = stream.read_block_ref()?;
            (None, pix_ref)
        };

        let pixel_layout = stream.read_u32_le()?;
        let use_mipmaps = stream.read_u32_le()?;
        let alpha_format = stream.read_u32_le()?;
        let is_static = stream.read_u8()? != 0;

        if stream.version() >= crate::version::NifVersion(0x0A010006) {
            let _direct_render = stream.read_byte_bool()?;
        }

        if stream.version() >= crate::version::NifVersion::V20_2_0_7 {
            let _persist_render_data = stream.read_byte_bool()?;
        }

        Ok(Self {
            net, use_external, filename, pixel_data_ref,
            pixel_layout, use_mipmaps, alpha_format, is_static,
        })
    }
}
