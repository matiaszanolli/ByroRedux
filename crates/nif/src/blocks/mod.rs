//! NIF block type dispatch.
//!
//! Each serialized block in a NIF file has an RTTI class name from the
//! header's block type table. This module maps those names to parsers
//! and provides the NiObject trait that all parsed blocks implement.

pub mod base;
pub mod controller;
pub mod extra_data;
pub mod interpolator;
pub mod multibound;
pub mod node;
pub mod palette;
pub mod properties;
pub mod shader;
pub mod skin;
pub mod texture;
pub mod traits;
pub mod tri_shape;

use crate::stream::NifStream;
use controller::{
    NiControllerManager, NiControllerSequence, NiGeomMorpherController, NiMaterialColorController,
    NiMorphData, NiMultiTargetTransformController, NiSingleInterpController, NiTimeController,
};
use extra_data::{
    BsBehaviorGraphExtraData, BsBound, BsClothExtraData, BsConnectPointChildren,
    BsConnectPointParents, BsDecalPlacementVectorExtraData, BsInvMarker, NiExtraData,
};
use multibound::{BsMultiBound, BsMultiBoundAABB, BsMultiBoundOBB};
use interpolator::{
    NiBlendBoolInterpolator, NiBlendFloatInterpolator, NiBlendPoint3Interpolator,
    NiBlendTransformInterpolator, NiBoolData, NiBoolInterpolator, NiFloatData, NiFloatInterpolator,
    NiPoint3Interpolator, NiPosData, NiTextKeyExtraData, NiTransformData, NiTransformInterpolator,
};
use node::{BsOrderedNode, BsValueNode, NiNode};
use properties::{
    NiAlphaProperty, NiFlagProperty, NiMaterialProperty, NiStencilProperty,
    NiStringPalette, NiTexturingProperty, NiVertexColorProperty, NiZBufferProperty,
};
use shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderNoLightingProperty,
    BSShaderPPLightingProperty, BSShaderTextureSet,
};
use skin::{BsDismemberSkinInstance, NiSkinData, NiSkinInstance, NiSkinPartition};
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
        "NiNode" | "BSFadeNode" | "BSLeafAnimNode" | "BSTreeNode" | "BSMultiBoundNode"
        | "RootCollisionNode" => {
            Ok(Box::new(NiNode::parse(stream)?))
        }
        "BSOrderedNode" => Ok(Box::new(BsOrderedNode::parse(stream)?)),
        "BSValueNode" => Ok(Box::new(BsValueNode::parse(stream)?)),
        // Multi-bound spatial volumes
        "BSMultiBound" => Ok(Box::new(BsMultiBound::parse(stream)?)),
        "BSMultiBoundAABB" => Ok(Box::new(BsMultiBoundAABB::parse(stream)?)),
        "BSMultiBoundOBB" => Ok(Box::new(BsMultiBoundOBB::parse(stream)?)),
        "NiTriShape" | "NiTriStrips" | "BSSegmentedTriShape" => {
            Ok(Box::new(NiTriShape::parse(stream)?))
        }
        "BSTriShape" | "BSMeshLODTriShape" | "BSSubIndexTriShape" => {
            Ok(Box::new(tri_shape::BsTriShape::parse(stream)?))
        }
        "NiTriShapeData" => Ok(Box::new(NiTriShapeData::parse(stream)?)),
        "NiTriStripsData" => Ok(Box::new(NiTriStripsData::parse(stream)?)),
        "BSShaderPPLightingProperty" => Ok(Box::new(BSShaderPPLightingProperty::parse(stream)?)),
        "BSShaderNoLightingProperty" => Ok(Box::new(BSShaderNoLightingProperty::parse(stream)?)),
        "BSShaderTextureSet" => Ok(Box::new(BSShaderTextureSet::parse(stream)?)),
        "BSLightingShaderProperty" => Ok(Box::new(BSLightingShaderProperty::parse(stream)?)),
        "BSEffectShaderProperty" => Ok(Box::new(BSEffectShaderProperty::parse(stream)?)),
        "NiMaterialProperty" => Ok(Box::new(NiMaterialProperty::parse(stream)?)),
        "NiAlphaProperty" => Ok(Box::new(NiAlphaProperty::parse(stream)?)),
        "NiStencilProperty" => Ok(Box::new(NiStencilProperty::parse(stream)?)),
        "NiZBufferProperty" => Ok(Box::new(NiZBufferProperty::parse(stream)?)),
        "NiVertexColorProperty" => Ok(Box::new(NiVertexColorProperty::parse(stream)?)),
        "NiTexturingProperty" => Ok(Box::new(NiTexturingProperty::parse(stream)?)),
        // Simple flag-only properties (Oblivion)
        "NiSpecularProperty" => {
            Ok(Box::new(NiFlagProperty::parse(stream, "NiSpecularProperty")?))
        }
        "NiWireframeProperty" => {
            Ok(Box::new(NiFlagProperty::parse(stream, "NiWireframeProperty")?))
        }
        "NiDitherProperty" => {
            Ok(Box::new(NiFlagProperty::parse(stream, "NiDitherProperty")?))
        }
        "NiShadeProperty" => {
            Ok(Box::new(NiFlagProperty::parse(stream, "NiShadeProperty")?))
        }
        "NiStringPalette" => Ok(Box::new(NiStringPalette::parse(stream)?)),
        "NiSourceTexture" => Ok(Box::new(NiSourceTexture::parse(stream)?)),
        // Skinning blocks
        "NiSkinInstance" => Ok(Box::new(NiSkinInstance::parse(stream)?)),
        "BSDismemberSkinInstance" => Ok(Box::new(BsDismemberSkinInstance::parse(stream)?)),
        "NiSkinData" => Ok(Box::new(NiSkinData::parse(stream)?)),
        "NiSkinPartition" => Ok(Box::new(NiSkinPartition::parse(stream)?)),
        "NiStringExtraData" | "NiBinaryExtraData" | "NiIntegerExtraData" | "BSXFlags"
        | "NiBooleanExtraData" => Ok(Box::new(NiExtraData::parse(stream, type_name)?)),
        "BSBound" => Ok(Box::new(BsBound::parse(stream)?)),
        "BSDecalPlacementVectorExtraData" => {
            Ok(Box::new(BsDecalPlacementVectorExtraData::parse(stream)?))
        }
        "BSBehaviorGraphExtraData" => Ok(Box::new(BsBehaviorGraphExtraData::parse(stream)?)),
        "BSInvMarker" => Ok(Box::new(BsInvMarker::parse(stream)?)),
        "BSClothExtraData" => Ok(Box::new(BsClothExtraData::parse(stream)?)),
        "BSConnectPoint::Parents" => Ok(Box::new(BsConnectPointParents::parse(stream)?)),
        "BSConnectPoint::Children" => Ok(Box::new(BsConnectPointChildren::parse(stream)?)),
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
        "NiGeomMorpherController" => Ok(Box::new(NiGeomMorpherController::parse(stream)?)),
        "NiMorphData" => Ok(Box::new(NiMorphData::parse(stream)?)),
        "NiControllerManager" => Ok(Box::new(NiControllerManager::parse(stream)?)),
        "NiControllerSequence" => Ok(Box::new(NiControllerSequence::parse(stream)?)),
        "NiDefaultAVObjectPalette" => {
            Ok(Box::new(palette::NiDefaultAVObjectPalette::parse(stream)?))
        }
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
        // Blend interpolators (NiControllerManager animation blending)
        "NiBlendTransformInterpolator" => {
            Ok(Box::new(NiBlendTransformInterpolator::parse(stream)?))
        }
        "NiBlendFloatInterpolator" => Ok(Box::new(NiBlendFloatInterpolator::parse(stream)?)),
        "NiBlendPoint3Interpolator" => Ok(Box::new(NiBlendPoint3Interpolator::parse(stream)?)),
        "NiBlendBoolInterpolator" => Ok(Box::new(NiBlendBoolInterpolator::parse(stream)?)),
        // Base NiTimeController fallback for unknown controller subtypes
        "NiTimeController" => Ok(Box::new(NiTimeController::parse(stream)?)),
        // Havok collision blocks — skip via block_size (no rendering use).
        // On FO3+ (v20.2.0.7) block_size is always available.
        // On Oblivion (v20.0.0.5) these will fall through to the hard error
        // since block sizes aren't in the header — full Havok parsers needed
        // for Oblivion collision support (future milestone).
        "bhkCollisionObject"
        | "bhkBlendCollisionObject"
        | "bhkSPCollisionObject"
        | "bhkRigidBody"
        | "bhkRigidBodyT"
        | "bhkSimpleShapePhantom"
        | "bhkMoppBvTreeShape"
        | "bhkCompressedMeshShape"
        | "bhkCompressedMeshShapeData"
        | "bhkConvexVerticesShape"
        | "bhkBoxShape"
        | "bhkSphereShape"
        | "bhkCapsuleShape"
        | "bhkListShape"
        | "bhkNiTriStripsShape"
        | "bhkPackedNiTriStripsShape"
        | "hkPackedNiTriStripsData"
        | "bhkTransformShape"
        | "bhkConvexTransformShape"
        | "bhkMalleableConstraint"
        | "bhkRagdollConstraint"
        | "bhkLimitedHingeConstraint"
        | "bhkHingeConstraint"
        | "bhkBallAndSocketConstraint"
        | "bhkStiffSpringConstraint"
        | "bhkPrismaticConstraint"
        | "NiCollisionObject"
        | "bhkNPCollisionObject"
        | "bhkPhysicsSystem"
        | "bhkRagdollSystem" => {
            // These are recognized but not parsed — skip via block_size.
            if let Some(size) = block_size {
                let data = stream.read_bytes(size as usize)?;
                Ok(Box::new(NiUnknown {
                    type_name: type_name.to_string(),
                    data,
                }))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Havok collision block '{}' requires block_size to skip (Oblivion NIFs need dedicated parsers)",
                        type_name
                    ),
                ))
            }
        }
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
