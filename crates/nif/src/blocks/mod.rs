//! NIF block type dispatch.
//!
//! Each serialized block in a NIF file has an RTTI class name from the
//! header's block type table. This module maps those names to parsers
//! and provides the NiObject trait that all parsed blocks implement.

pub mod base;
pub mod collision;
pub mod controller;
pub mod extra_data;
pub mod interpolator;
pub mod multibound;
pub mod node;
pub mod palette;
pub mod particle;
pub mod properties;
pub mod shader;
pub mod skin;
pub mod texture;
pub mod traits;
pub mod tri_shape;

use crate::stream::NifStream;
use collision::{
    BhkBoxShape, BhkCapsuleShape, BhkCollisionObject, BhkCompressedMeshShape,
    BhkCompressedMeshShapeData, BhkConvexVerticesShape, BhkCylinderShape, BhkListShape,
    BhkMoppBvTreeShape, BhkNiTriStripsShape, BhkPackedNiTriStripsShape, BhkRigidBody,
    BhkSimpleShapePhantom, BhkSphereShape, BhkTransformShape, HkPackedNiTriStripsData,
};
use controller::{
    NiControllerManager, NiControllerSequence, NiGeomMorpherController, NiMaterialColorController,
    NiMorphData, NiMultiTargetTransformController, NiSequenceStreamHelper,
    NiSingleInterpController, NiTimeController,
};
use extra_data::{
    BsBehaviorGraphExtraData, BsBound, BsClothExtraData, BsConnectPointChildren,
    BsConnectPointParents, BsDecalPlacementVectorExtraData, BsInvMarker, NiExtraData,
};
use interpolator::{
    NiBlendBoolInterpolator, NiBlendFloatInterpolator, NiBlendPoint3Interpolator,
    NiBlendTransformInterpolator, NiBoolData, NiBoolInterpolator, NiFloatData, NiFloatInterpolator,
    NiPoint3Interpolator, NiPosData, NiTextKeyExtraData, NiTransformData, NiTransformInterpolator,
};
use multibound::{BsMultiBound, BsMultiBoundAABB, BsMultiBoundOBB};
use node::{BsOrderedNode, BsValueNode, NiNode};
use properties::{
    NiAlphaProperty, NiFlagProperty, NiMaterialProperty, NiStencilProperty, NiStringPalette,
    NiTexturingProperty, NiVertexColorProperty, NiZBufferProperty,
};
use shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderNoLightingProperty,
    BSShaderPPLightingProperty, BSShaderTextureSet,
};
use skin::{
    BsDismemberSkinInstance, BsSkinBoneData, BsSkinInstance, NiSkinData, NiSkinInstance,
    NiSkinPartition,
};
use std::any::Any;
use std::fmt::Debug;
use std::io;
use texture::{NiPixelData, NiSourceTexture};
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
        | "RootCollisionNode" => Ok(Box::new(NiNode::parse(stream)?)),
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
        // All Oblivion-era BSShaderLightingProperty specializations share the
        // base texture-set + flags layout, so alias them to BSShaderPPLighting.
        // Specializing the differences (e.g. sky scroll, water reflection) can
        // come later — for now this unblocks Oblivion exterior cells, which
        // hard-failed on any of these.
        "BSShaderPPLightingProperty"
        | "SkyShaderProperty"
        | "WaterShaderProperty"
        | "TallGrassShaderProperty"
        | "Lighting30ShaderProperty"
        | "TileShaderProperty"
        | "HairShaderProperty"
        | "VolumetricFogShaderProperty"
        | "DistantLODShaderProperty"
        | "BSDistantTreeShaderProperty"
        | "BSSkyShaderProperty"
        | "BSWaterShaderProperty" => Ok(Box::new(BSShaderPPLightingProperty::parse(stream)?)),
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
        "NiSpecularProperty" => Ok(Box::new(NiFlagProperty::parse(
            stream,
            "NiSpecularProperty",
        )?)),
        "NiWireframeProperty" => Ok(Box::new(NiFlagProperty::parse(
            stream,
            "NiWireframeProperty",
        )?)),
        "NiDitherProperty" => Ok(Box::new(NiFlagProperty::parse(stream, "NiDitherProperty")?)),
        "NiShadeProperty" => Ok(Box::new(NiFlagProperty::parse(stream, "NiShadeProperty")?)),
        "NiStringPalette" => Ok(Box::new(NiStringPalette::parse(stream)?)),
        "NiSourceTexture" => Ok(Box::new(NiSourceTexture::parse(stream)?)),
        "NiPixelData" | "NiPersistentSrcTextureRendererData" => {
            Ok(Box::new(NiPixelData::parse(stream)?))
        }
        // Skinning blocks
        "NiSkinInstance" => Ok(Box::new(NiSkinInstance::parse(stream)?)),
        "BSDismemberSkinInstance" => Ok(Box::new(BsDismemberSkinInstance::parse(stream)?)),
        "NiSkinData" => Ok(Box::new(NiSkinData::parse(stream)?)),
        "NiSkinPartition" => Ok(Box::new(NiSkinPartition::parse(stream)?)),
        "BSSkin::Instance" => Ok(Box::new(BsSkinInstance::parse(stream)?)),
        "BSSkin::BoneData" => Ok(Box::new(BsSkinBoneData::parse(stream)?)),
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
        // NiKeyframeController is the pre-Skyrim per-bone animation driver
        // (Oblivion / Morrowind / FO3 / FNV KF files). It inherits from
        // NiSingleInterpController with no extra fields at Oblivion-era
        // versions, so it parses identically — see issue #144.
        "NiTransformController"
        | "NiKeyframeController"
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
        // Pre-Skyrim KF animation root — see issue #144. NiObjectNET with
        // no extra fields; the per-bone controller chain and text keys
        // hang off its extra_data / controller refs.
        "NiSequenceStreamHelper" => Ok(Box::new(NiSequenceStreamHelper::parse(stream)?)),
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
        // ── Particle system blocks ──────────────────────────────────
        // Geometry nodes
        "NiParticles" => Ok(Box::new(particle::parse_particle_system(
            stream,
            "NiParticles",
        )?)),
        "NiParticleSystem" | "NiMeshParticleSystem" => Ok(Box::new(
            particle::parse_particle_system(stream, type_name)?,
        )),
        "BSStripParticleSystem" => Ok(Box::new(particle::parse_strip_particle_system(stream)?)),
        "BSMasterParticleSystem" => Ok(Box::new(particle::parse_master_particle_system(stream)?)),
        // Data blocks
        "NiParticlesData" | "NiPSysData" | "NiMeshPSysData" | "BSStripPSysData" => {
            Ok(Box::new(particle::parse_particles_data(stream, type_name)?))
        }
        "NiPSysEmitterCtlrData" => Ok(Box::new(particle::parse_emitter_ctlr_data(stream)?)),
        // Modifiers
        "NiPSysPositionModifier" => Ok(Box::new(particle::parse_modifier_only(
            stream,
            "NiPSysPositionModifier",
        )?)),
        "NiPSysAgeDeathModifier" => Ok(Box::new(particle::parse_age_death_modifier(stream)?)),
        "NiPSysBombModifier" => Ok(Box::new(particle::parse_bomb_modifier(stream)?)),
        "NiPSysBoundUpdateModifier" => Ok(Box::new(particle::parse_bound_update_modifier(stream)?)),
        "NiPSysColliderManager" => Ok(Box::new(particle::parse_collider_manager(stream)?)),
        "NiPSysColorModifier" => Ok(Box::new(particle::parse_color_modifier(stream)?)),
        "NiPSysDragModifier" => Ok(Box::new(particle::parse_drag_modifier(stream)?)),
        "NiPSysGravityModifier" => Ok(Box::new(particle::parse_gravity_modifier(stream)?)),
        "NiPSysGrowFadeModifier" => Ok(Box::new(particle::parse_grow_fade_modifier(stream)?)),
        "NiPSysRotationModifier" => Ok(Box::new(particle::parse_rotation_modifier(stream)?)),
        "NiPSysSpawnModifier" => Ok(Box::new(particle::parse_spawn_modifier(stream)?)),
        "NiPSysMeshUpdateModifier" => Ok(Box::new(particle::parse_mesh_update_modifier(
            stream,
            "NiPSysMeshUpdateModifier",
        )?)),
        "BSPSysHavokUpdateModifier" => Ok(Box::new(particle::parse_havok_update_modifier(stream)?)),
        "BSParentVelocityModifier" => Ok(Box::new(particle::parse_float_modifier(
            stream,
            "BSParentVelocityModifier",
        )?)),
        "BSWindModifier" => Ok(Box::new(particle::parse_float_modifier(
            stream,
            "BSWindModifier",
        )?)),
        "BSPSysInheritVelocityModifier" => {
            Ok(Box::new(particle::parse_inherit_velocity_modifier(stream)?))
        }
        "BSPSysRecycleBoundModifier" => {
            Ok(Box::new(particle::parse_recycle_bound_modifier(stream)?))
        }
        "BSPSysSubTexModifier" => Ok(Box::new(particle::parse_sub_tex_modifier(stream)?)),
        "BSPSysLODModifier" => Ok(Box::new(particle::parse_lod_modifier(stream)?)),
        "BSPSysScaleModifier" => Ok(Box::new(particle::parse_scale_modifier(stream)?)),
        "BSPSysSimpleColorModifier" => Ok(Box::new(particle::parse_simple_color_modifier(stream)?)),
        "BSPSysStripUpdateModifier" => Ok(Box::new(particle::parse_strip_update_modifier(stream)?)),
        // Emitters
        "NiPSysBoxEmitter" => Ok(Box::new(particle::parse_box_emitter(stream)?)),
        "NiPSysCylinderEmitter" => Ok(Box::new(particle::parse_cylinder_emitter(stream)?)),
        "NiPSysSphereEmitter" => Ok(Box::new(particle::parse_sphere_emitter(stream)?)),
        "BSPSysArrayEmitter" => Ok(Box::new(particle::parse_array_emitter(stream)?)),
        "NiPSysMeshEmitter" => Ok(Box::new(particle::parse_mesh_emitter(stream)?)),
        // Colliders
        "NiPSysPlanarCollider" => Ok(Box::new(particle::parse_planar_collider(stream)?)),
        "NiPSysSphericalCollider" => Ok(Box::new(particle::parse_spherical_collider(stream)?)),
        // Field modifiers
        "NiPSysVortexFieldModifier" | "NiPSysGravityFieldModifier" => Ok(Box::new(
            particle::parse_field_modifier_vec3(stream, type_name)?,
        )),
        "NiPSysDragFieldModifier" => Ok(Box::new(particle::parse_drag_field_modifier(stream)?)),
        "NiPSysTurbulenceFieldModifier" => {
            Ok(Box::new(particle::parse_turbulence_field_modifier(stream)?))
        }
        "NiPSysAirFieldModifier" => Ok(Box::new(particle::parse_air_field_modifier(stream)?)),
        "NiPSysRadialFieldModifier" => Ok(Box::new(particle::parse_radial_field_modifier(stream)?)),
        // Controllers
        "NiPSysUpdateCtlr" | "NiPSysResetOnLoopCtlr" => Ok(Box::new(
            particle::parse_time_controller(stream, type_name)?,
        )),
        "NiPSysEmitterCtlr" => Ok(Box::new(particle::parse_emitter_ctlr(stream)?)),
        "BSPSysMultiTargetEmitterCtlr" => {
            Ok(Box::new(particle::parse_multi_target_emitter_ctlr(stream)?))
        }
        "NiPSysModifierActiveCtlr"
        | "NiPSysEmitterDeclinationCtlr"
        | "NiPSysEmitterDeclinationVarCtlr"
        | "NiPSysEmitterInitialRadiusCtlr"
        | "NiPSysEmitterLifeSpanCtlr"
        | "NiPSysEmitterSpeedCtlr"
        | "NiPSysGravityStrengthCtlr"
        | "NiPSysInitialRotSpeedCtlr"
        | "NiPSysInitialRotSpeedVarCtlr"
        | "NiPSysInitialRotAngleCtlr"
        | "NiPSysInitialRotAngleVarCtlr"
        | "NiPSysEmitterPlanarAngleCtlr"
        | "NiPSysEmitterPlanarAngleVarCtlr"
        | "NiPSysFieldMagnitudeCtlr"
        | "NiPSysFieldAttenuationCtlr"
        | "NiPSysFieldMaxDistanceCtlr"
        | "NiPSysAirFieldAirFrictionCtlr"
        | "NiPSysAirFieldInheritVelocityCtlr"
        | "NiPSysAirFieldSpreadCtlr" => {
            Ok(Box::new(particle::parse_modifier_ctlr(stream, type_name)?))
        }
        // ── Havok collision blocks (fully parsed) ────────────────────
        "bhkCollisionObject" | "bhkSPCollisionObject" => {
            Ok(Box::new(BhkCollisionObject::parse(stream, false)?))
        }
        "bhkBlendCollisionObject" => Ok(Box::new(BhkCollisionObject::parse(stream, true)?)),
        "bhkRigidBody" | "bhkRigidBodyT" => Ok(Box::new(BhkRigidBody::parse(stream)?)),
        "bhkSimpleShapePhantom" => Ok(Box::new(BhkSimpleShapePhantom::parse(stream)?)),
        "bhkMoppBvTreeShape" => Ok(Box::new(BhkMoppBvTreeShape::parse(stream)?)),
        "bhkBoxShape" => Ok(Box::new(BhkBoxShape::parse(stream)?)),
        "bhkSphereShape" => Ok(Box::new(BhkSphereShape::parse(stream)?)),
        "bhkCapsuleShape" => Ok(Box::new(BhkCapsuleShape::parse(stream)?)),
        "bhkCylinderShape" => Ok(Box::new(BhkCylinderShape::parse(stream)?)),
        "bhkConvexVerticesShape" => Ok(Box::new(BhkConvexVerticesShape::parse(stream)?)),
        "bhkListShape" => Ok(Box::new(BhkListShape::parse(stream)?)),
        "bhkTransformShape" | "bhkConvexTransformShape" => {
            Ok(Box::new(BhkTransformShape::parse(stream)?))
        }
        "bhkNiTriStripsShape" => Ok(Box::new(BhkNiTriStripsShape::parse(stream)?)),
        "bhkPackedNiTriStripsShape" => Ok(Box::new(BhkPackedNiTriStripsShape::parse(stream)?)),
        "hkPackedNiTriStripsData" => Ok(Box::new(HkPackedNiTriStripsData::parse(stream)?)),
        "bhkCompressedMeshShape" => Ok(Box::new(BhkCompressedMeshShape::parse(stream)?)),
        "bhkCompressedMeshShapeData" => Ok(Box::new(BhkCompressedMeshShapeData::parse(stream)?)),
        // Havok blocks that remain skip-only (constraints, systems).
        // Constraints deferred to M28 (physics joints).
        "bhkMalleableConstraint"
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
            if let Some(size) = block_size {
                let data = stream.read_bytes(size as usize)?;
                Ok(Box::new(NiUnknown {
                    type_name: type_name.to_string(),
                    data,
                }))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Havok block '{}' requires block_size to skip", type_name),
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
                        "unknown block type '{}' and no block size available to skip \
                        (Oblivion v20.0.0.5 NIFs require dedicated parsers for every block type)",
                        type_name
                    ),
                ))
            }
        }
    }
}

#[cfg(test)]
mod dispatch_tests {
    //! Regression tests for `parse_block` type-name dispatch.
    //!
    //! These test that the dispatch table routes Oblivion-era shader
    //! variants through the right parser — see issue #145.
    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::NifVersion;
    use std::sync::Arc;

    /// Build an Oblivion (bsver=0) header with a single string slot.
    fn oblivion_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_0_0_5,
            little_endian: true,
            user_version: 11,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("SkyProp")],
            max_string_length: 8,
            num_groups: 0,
        }
    }

    /// Minimal Oblivion BSShaderPPLightingProperty-shaped payload: 22 bytes.
    /// Matches the no-extra-fields path (no refraction/parallax).
    fn oblivion_bsshader_bytes() -> Vec<u8> {
        let mut d = Vec::new();
        // NiObjectNET: name string index
        d.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        d.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1
        d.extend_from_slice(&(-1i32).to_le_bytes());
        // BSShaderProperty fields
        d.extend_from_slice(&0u16.to_le_bytes()); // shader_flags
        d.extend_from_slice(&1u32.to_le_bytes()); // shader_type
        d.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_1
        d.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_2
        d.extend_from_slice(&1.0f32.to_le_bytes()); // env_map_scale
        d.extend_from_slice(&3u32.to_le_bytes()); // texture_clamp_mode
        d.extend_from_slice(&5i32.to_le_bytes()); // texture_set_ref
        d
    }

    #[test]
    fn oblivion_shader_variants_route_to_bsshader_pp_lighting() {
        // Every specialized variant named in issue #145 must dispatch
        // through BSShaderPPLightingProperty::parse and produce a
        // downcastable block.
        let variants = [
            "BSShaderPPLightingProperty",
            "SkyShaderProperty",
            "WaterShaderProperty",
            "TallGrassShaderProperty",
            "Lighting30ShaderProperty",
            "TileShaderProperty",
            "HairShaderProperty",
            "VolumetricFogShaderProperty",
            "DistantLODShaderProperty",
            "BSDistantTreeShaderProperty",
            "BSSkyShaderProperty",
            "BSWaterShaderProperty",
        ];
        let header = oblivion_header();
        let bytes = oblivion_bsshader_bytes();

        for variant in variants {
            let mut stream = NifStream::new(&bytes, &header);
            let block = parse_block(variant, &mut stream, Some(bytes.len() as u32))
                .unwrap_or_else(|e| panic!("variant '{variant}' failed to parse: {e}"));
            let prop = block
                .as_any()
                .downcast_ref::<BSShaderPPLightingProperty>()
                .unwrap_or_else(|| {
                    panic!("variant '{variant}' did not downcast to BSShaderPPLightingProperty")
                });
            assert_eq!(
                prop.texture_set_ref.index(),
                Some(5),
                "variant '{variant}' parsed the wrong texture_set_ref"
            );
        }
    }

    /// Regression test for issue #144: Oblivion-era KF animation roots
    /// must dispatch through the right parsers.
    #[test]
    fn oblivion_kf_animation_blocks_route_correctly() {
        // NiKeyframeController: parses as NiSingleInterpController
        // (26-byte NiTimeControllerBase + 4-byte interpolator ref).
        let header = oblivion_header();
        let mut kf_bytes = Vec::new();
        // NiTimeControllerBase: next_controller, flags, frequency, phase,
        // start_time, stop_time, target_ref.
        kf_bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
        kf_bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
        kf_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
        kf_bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
        kf_bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
        kf_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // stop_time
        kf_bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref
        kf_bytes.extend_from_slice(&7i32.to_le_bytes()); // interpolator_ref
        let mut stream = NifStream::new(&kf_bytes, &header);
        let block = parse_block("NiKeyframeController", &mut stream, Some(kf_bytes.len() as u32))
            .expect("NiKeyframeController should dispatch through NiSingleInterpController");
        let ctrl = block
            .as_any()
            .downcast_ref::<crate::blocks::controller::NiSingleInterpController>()
            .expect("NiKeyframeController did not downcast to NiSingleInterpController");
        assert_eq!(ctrl.interpolator_ref.index(), Some(7));

        // NiSequenceStreamHelper: NiObjectNET with no extra fields.
        // name (string table index 0) + extra_data count (0) + controller ref (-1)
        let mut ssh_bytes = Vec::new();
        ssh_bytes.extend_from_slice(&0i32.to_le_bytes()); // name
        ssh_bytes.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
        ssh_bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller
        let mut stream = NifStream::new(&ssh_bytes, &header);
        let block =
            parse_block("NiSequenceStreamHelper", &mut stream, Some(ssh_bytes.len() as u32))
                .expect("NiSequenceStreamHelper should dispatch to its own parser");
        assert!(block
            .as_any()
            .downcast_ref::<crate::blocks::controller::NiSequenceStreamHelper>()
            .is_some());
    }
}
