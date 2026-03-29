//! NIF block type dispatch.
//!
//! Each serialized block in a NIF file has an RTTI class name from the
//! header's block type table. This module maps those names to parsers
//! and provides the NiObject trait that all parsed blocks implement.

pub mod node;
pub mod tri_shape;
pub mod properties;
pub mod texture;
pub mod extra_data;
pub mod controller;

use crate::stream::NifStream;
use node::NiNode;
use tri_shape::{NiTriShape, NiTriShapeData};
use properties::{NiMaterialProperty, NiAlphaProperty, NiTexturingProperty};
use texture::NiSourceTexture;
use extra_data::NiExtraData;
use controller::NiTimeController;
use std::any::Any;
use std::fmt::Debug;
use std::io;

/// Trait implemented by all parsed NIF blocks.
pub trait NiObject: Debug + Send + Sync {
    fn block_type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;
}

/// A parsed block that we don't have a specific parser for.
/// Preserved so block indices remain valid.
#[derive(Debug)]
pub struct NiUnknown {
    pub type_name: String,
    pub data: Vec<u8>,
}

impl NiObject for NiUnknown {
    fn block_type_name(&self) -> &'static str {
        "NiUnknown"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Parse a single block given its type name and a stream positioned at the block data.
/// If the block size is known, `block_size` enables graceful skip of unknown types.
pub fn parse_block(
    type_name: &str,
    stream: &mut NifStream,
    block_size: Option<u32>,
) -> io::Result<Box<dyn NiObject>> {
    match type_name {
        "NiNode" | "BSFadeNode" | "BSLeafAnimNode" | "BSTreeNode" => {
            Ok(Box::new(NiNode::parse(stream)?))
        }
        "NiTriShape" | "BSTriShape" => {
            Ok(Box::new(NiTriShape::parse(stream)?))
        }
        "NiTriShapeData" => {
            Ok(Box::new(NiTriShapeData::parse(stream)?))
        }
        "NiMaterialProperty" => {
            Ok(Box::new(NiMaterialProperty::parse(stream)?))
        }
        "NiAlphaProperty" => {
            Ok(Box::new(NiAlphaProperty::parse(stream)?))
        }
        "NiTexturingProperty" => {
            Ok(Box::new(NiTexturingProperty::parse(stream)?))
        }
        "NiSourceTexture" => {
            Ok(Box::new(NiSourceTexture::parse(stream)?))
        }
        "NiStringExtraData" | "NiBinaryExtraData" | "NiIntegerExtraData" |
        "BSXFlags" | "NiBooleanExtraData" => {
            Ok(Box::new(NiExtraData::parse(stream, type_name)?))
        }
        "NiTimeController" | "NiTransformController" | "NiVisController" |
        "NiAlphaController" | "NiTextureTransformController" |
        "NiMaterialColorController" | "NiMultiTargetTransformController" |
        "BSEffectShaderPropertyFloatController" |
        "BSEffectShaderPropertyColorController" |
        "BSLightingShaderPropertyFloatController" |
        "BSLightingShaderPropertyColorController" |
        "NiControllerManager" | "NiControllerSequence" => {
            Ok(Box::new(NiTimeController::parse(stream)?))
        }
        _ => {
            // Unknown block type — skip it if we know the size
            if let Some(size) = block_size {
                let start = stream.position();
                let data = stream.read_bytes(size as usize)?;
                log::debug!("Skipping unknown block type '{}' ({} bytes at offset {})",
                    type_name, size, start);
                Ok(Box::new(NiUnknown {
                    type_name: type_name.to_string(),
                    data,
                }))
            } else {
                Err(io::Error::new(io::ErrorKind::InvalidData,
                    format!("unknown block type '{}' and no block size available to skip", type_name)))
            }
        }
    }
}
