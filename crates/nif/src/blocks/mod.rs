//! NIF block type dispatch.
//!
//! Each serialized block in a NIF file has an RTTI class name from the
//! header's block type table. This module maps those names to parsers
//! and provides the NiObject trait that all parsed blocks implement.

pub mod base;
pub mod controller;
pub mod extra_data;
pub mod interpolator;
pub mod node;
pub mod properties;
pub mod shader;
pub mod texture;
pub mod traits;
pub mod tri_shape;

use crate::stream::NifStream;
use controller::{
    NiControllerManager, NiControllerSequence, NiMaterialColorController,
    NiMultiTargetTransformController, NiSingleInterpController, NiTimeController,
};
use extra_data::NiExtraData;
use interpolator::{
    NiBoolData, NiBoolInterpolator, NiFloatData, NiFloatInterpolator, NiPoint3Interpolator,
    NiPosData, NiTextKeyExtraData, NiTransformData, NiTransformInterpolator,
};
use node::NiNode;
use properties::{NiAlphaProperty, NiMaterialProperty, NiTexturingProperty};
use shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderNoLightingProperty,
    BSShaderPPLightingProperty, BSShaderTextureSet,
};
use std::any::Any;
use std::fmt::Debug;
use std::io;
use texture::NiSourceTexture;
use tri_shape::{NiTriShape, NiTriShapeData, NiTriStripsData};

/// Trait implemented by all parsed NIF blocks.
pub trait NiObject: Debug + Send + Sync {
    fn block_type_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn Any;

    /// Upcast to NiObjectNET if this block has name/extra_data/controller.
    fn as_object_net(&self) -> Option<&dyn traits::HasObjectNET> {
        None
    }
    /// Upcast to NiAVObject if this block has transform/flags/collision.
    fn as_av_object(&self) -> Option<&dyn traits::HasAVObject> {
        None
    }
    /// Upcast to shader refs if this block provides shader/alpha property refs.
    fn as_shader_refs(&self) -> Option<&dyn traits::HasShaderRefs> {
        None
    }
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
        "NiNode" | "BSFadeNode" | "BSLeafAnimNode" | "BSTreeNode" | "BSMultiBoundNode" => {
            Ok(Box::new(NiNode::parse(stream)?))
        }
        "NiTriShape" | "NiTriStrips" | "BSSegmentedTriShape" => {
            Ok(Box::new(NiTriShape::parse(stream)?))
        }
        "BSTriShape" | "BSMeshLODTriShape" => Ok(Box::new(tri_shape::BsTriShape::parse(stream)?)),
        "NiTriShapeData" => Ok(Box::new(NiTriShapeData::parse(stream)?)),
        "NiTriStripsData" => Ok(Box::new(NiTriStripsData::parse(stream)?)),
        "BSShaderPPLightingProperty" => Ok(Box::new(BSShaderPPLightingProperty::parse(stream)?)),
        "BSShaderNoLightingProperty" => Ok(Box::new(BSShaderNoLightingProperty::parse(stream)?)),
        "BSShaderTextureSet" => Ok(Box::new(BSShaderTextureSet::parse(stream)?)),
        "BSLightingShaderProperty" => Ok(Box::new(BSLightingShaderProperty::parse(stream)?)),
        "BSEffectShaderProperty" => Ok(Box::new(BSEffectShaderProperty::parse(stream)?)),
        "NiMaterialProperty" => Ok(Box::new(NiMaterialProperty::parse(stream)?)),
        "NiAlphaProperty" => Ok(Box::new(NiAlphaProperty::parse(stream)?)),
        "NiTexturingProperty" => Ok(Box::new(NiTexturingProperty::parse(stream)?)),
        "NiSourceTexture" => Ok(Box::new(NiSourceTexture::parse(stream)?)),
        "NiStringExtraData" | "NiBinaryExtraData" | "NiIntegerExtraData" | "BSXFlags"
        | "NiBooleanExtraData" => Ok(Box::new(NiExtraData::parse(stream, type_name)?)),
        // NiSingleInterpController subclasses (base + interpolator ref)
        "NiTextureTransformController" => Ok(Box::new(
            controller::NiTextureTransformController::parse(stream)?,
        )),
        "NiTransformController"
        | "NiVisController"
        | "NiAlphaController"
        | "BSEffectShaderPropertyFloatController"
        | "BSEffectShaderPropertyColorController"
        | "BSLightingShaderPropertyFloatController"
        | "BSLightingShaderPropertyColorController" => {
            Ok(Box::new(NiSingleInterpController::parse(stream)?))
        }
        "NiMaterialColorController" => Ok(Box::new(NiMaterialColorController::parse(stream)?)),
        "NiMultiTargetTransformController" => {
            Ok(Box::new(NiMultiTargetTransformController::parse(stream)?))
        }
        "NiControllerManager" => Ok(Box::new(NiControllerManager::parse(stream)?)),
        "NiControllerSequence" => Ok(Box::new(NiControllerSequence::parse(stream)?)),
        // Interpolator blocks (animation keyframe data)
        "NiTransformInterpolator" | "BSRotAccumTransfInterpolator" => {
            Ok(Box::new(NiTransformInterpolator::parse(stream)?))
        }
        "NiTransformData" | "NiKeyframeData" => Ok(Box::new(NiTransformData::parse(stream)?)),
        "NiFloatInterpolator" => Ok(Box::new(NiFloatInterpolator::parse(stream)?)),
        "NiFloatData" => Ok(Box::new(NiFloatData::parse(stream)?)),
        "NiPoint3Interpolator" => Ok(Box::new(NiPoint3Interpolator::parse(stream)?)),
        "NiPosData" => Ok(Box::new(NiPosData::parse(stream)?)),
        "NiBoolInterpolator" => Ok(Box::new(NiBoolInterpolator::parse(stream)?)),
        "NiBoolData" => Ok(Box::new(NiBoolData::parse(stream)?)),
        "NiTextKeyExtraData" => Ok(Box::new(NiTextKeyExtraData::parse(stream)?)),
        // Base NiTimeController fallback for unknown controller subtypes
        "NiTimeController" => Ok(Box::new(NiTimeController::parse(stream)?)),
        _ => {
            // Unknown block type — skip it if we know the size
            if let Some(size) = block_size {
                let start = stream.position();
                let data = stream.read_bytes(size as usize)?;
                log::debug!(
                    "Skipping unknown block type '{}' ({} bytes at offset {})",
                    type_name,
                    size,
                    start
                );
                Ok(Box::new(NiUnknown {
                    type_name: type_name.to_string(),
                    data,
                }))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "unknown block type '{}' and no block size available to skip",
                        type_name
                    ),
                ))
            }
        }
    }
}
