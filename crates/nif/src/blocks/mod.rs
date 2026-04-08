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
pub mod legacy_particle;
pub mod light;
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
    NiSingleInterpController, NiTimeController, NiUVController,
};
use extra_data::{
    BsBehaviorGraphExtraData, BsBound, BsClothExtraData, BsConnectPointChildren,
    BsConnectPointParents, BsDecalPlacementVectorExtraData, BsInvMarker, NiExtraData,
};
use interpolator::{
    NiBSplineBasisData, NiBSplineCompTransformInterpolator, NiBSplineData,
    NiBlendBoolInterpolator, NiBlendFloatInterpolator, NiBlendPoint3Interpolator,
    NiBlendTransformInterpolator, NiBoolData, NiBoolInterpolator, NiFloatData, NiFloatInterpolator,
    NiPoint3Interpolator, NiPosData, NiTextKeyExtraData, NiTransformData, NiTransformInterpolator,
    NiUVData,
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
        // NiLight hierarchy — see issue #156. All four subtypes share the
        // NiDynamicEffect + NiLight base; NiSpotLight extends NiPointLight.
        "NiAmbientLight" => Ok(Box::new(light::NiAmbientLight::parse(stream)?)),
        "NiDirectionalLight" => Ok(Box::new(light::NiDirectionalLight::parse(stream)?)),
        "NiPointLight" => Ok(Box::new(light::NiPointLight::parse(stream)?)),
        "NiSpotLight" => Ok(Box::new(light::NiSpotLight::parse(stream)?)),
        // Plain NiNode alias targets — no extra serialized fields beyond the
        // NiNode base. AvoidNode / NiBSAnimationNode / NiBSParticleNode are
        // legacy Morrowind/Oblivion-era NiNode subclasses; the rest are
        // Bethesda node tags. See issue #142.
        "NiNode"
        | "BSFadeNode"
        | "BSLeafAnimNode"
        | "BSTreeNode"
        | "BSMultiBoundNode"
        | "RootCollisionNode"
        | "AvoidNode"
        | "NiBSAnimationNode"
        | "NiBSParticleNode" => Ok(Box::new(NiNode::parse(stream)?)),
        // NiNode subtypes with a small payload of trailing fields.
        "NiBillboardNode" => Ok(Box::new(node::NiBillboardNode::parse(stream)?)),
        "NiSwitchNode" => Ok(Box::new(node::NiSwitchNode::parse(stream)?)),
        "NiLODNode" => Ok(Box::new(node::NiLODNode::parse(stream)?)),
        "NiSortAdjustNode" => Ok(Box::new(node::NiSortAdjustNode::parse(stream)?)),
        // BSRangeNode + subclasses — all carry the same (min, max, current)
        // byte triple and are FO3+. BSBlastNode/BSDamageStage/BSDebrisNode
        // inherit BSRangeNode and add nothing on the wire.
        "BSRangeNode" | "BSBlastNode" | "BSDamageStage" | "BSDebrisNode" => {
            Ok(Box::new(node::BsRangeNode::parse(stream)?))
        }
        // NiCamera — embedded cinematic camera block. See issue #153.
        "NiCamera" => Ok(Box::new(node::NiCamera::parse(stream)?)),
        // NiTextureEffect — projected env-map / gobo / fog projector.
        // See issue #163.
        "NiTextureEffect" => Ok(Box::new(texture::NiTextureEffect::parse(stream)?)),
        // Legacy (pre-NiPSys) particle stack — Oblivion magic FX, fire,
        // dust, blood. See issue #143. NiBSPArrayController is an empty
        // NiParticleSystemController subclass (zero additional fields)
        // so it aliases to the same parser.
        "NiParticleSystemController" | "NiBSPArrayController" => Ok(Box::new(
            legacy_particle::NiParticleSystemController::parse(stream)?,
        )),
        "NiAutoNormalParticles" => Ok(Box::new(legacy_particle::NiLegacyParticles::parse(
            stream,
            "NiAutoNormalParticles",
        )?)),
        "NiRotatingParticles" => Ok(Box::new(legacy_particle::NiLegacyParticles::parse(
            stream,
            "NiRotatingParticles",
        )?)),
        "NiAutoNormalParticlesData" => Ok(Box::new(
            legacy_particle::NiLegacyParticlesData::parse(stream, "NiAutoNormalParticlesData")?,
        )),
        "NiRotatingParticlesData" => Ok(Box::new(legacy_particle::NiLegacyParticlesData::parse(
            stream,
            "NiRotatingParticlesData",
        )?)),
        "NiParticleColorModifier" => Ok(Box::new(
            legacy_particle::NiParticleColorModifier::parse(stream)?,
        )),
        "NiParticleGrowFade" => {
            Ok(Box::new(legacy_particle::NiParticleGrowFade::parse(stream)?))
        }
        "NiParticleRotation" => {
            Ok(Box::new(legacy_particle::NiParticleRotation::parse(stream)?))
        }
        "NiParticleBomb" => Ok(Box::new(legacy_particle::NiParticleBomb::parse(stream)?)),
        "NiGravity" => Ok(Box::new(legacy_particle::NiGravity::parse(stream)?)),
        "NiPlanarCollider" => {
            Ok(Box::new(legacy_particle::NiPlanarCollider::parse(stream)?))
        }
        "NiSphericalCollider" => Ok(Box::new(legacy_particle::NiSphericalCollider::parse(
            stream,
        )?)),
        "BSOrderedNode" => Ok(Box::new(BsOrderedNode::parse(stream)?)),
        "BSValueNode" => Ok(Box::new(BsValueNode::parse(stream)?)),
        // Multi-bound spatial volumes
        "BSMultiBound" => Ok(Box::new(BsMultiBound::parse(stream)?)),
        "BSMultiBoundAABB" => Ok(Box::new(BsMultiBoundAABB::parse(stream)?)),
        "BSMultiBoundOBB" => Ok(Box::new(BsMultiBoundOBB::parse(stream)?)),
        "NiTriShape" | "NiTriStrips" => Ok(Box::new(NiTriShape::parse(stream)?)),
        // BSSegmentedTriShape: FO3/FNV/SkyrimLE biped body-part segmentation.
        // Inherits NiTriShape and adds a trailing (u32 num_segments) +
        // (u8 flags + u32 index + u32 num_tris_in_segment)[num_segments]
        // array. Previously aliased to plain NiTriShape, leaving those
        // bytes unread and relying on block-loop realignment. See #146.
        "BSSegmentedTriShape" => Ok(Box::new(NiTriShape::parse_segmented(stream)?)),
        "BSTriShape" => Ok(Box::new(tri_shape::BsTriShape::parse(stream)?)),
        // BSMeshLODTriShape / BSLODTriShape: same 3-u32 LOD-size trailing
        // layout. BSMeshLODTriShape appears in Skyrim SE DLC and FO4 LOD;
        // BSLODTriShape is the FO4 distant-LOD variant. See issue #147, #157.
        "BSMeshLODTriShape" | "BSLODTriShape" => {
            Ok(Box::new(tri_shape::BsTriShape::parse_lod(stream)?))
        }
        // BSSubIndexTriShape: ubiquitous in Skyrim SE DLC and all FO4 actor
        // meshes (clothing segmentation for dismemberment). After the
        // BSTriShape body, FO4+ adds a variable-size segmentation block
        // (num primitives, segment table, optional shared sub-segment data
        // with SSF filename). The segmentation structure is used only for
        // gameplay damage subdivision — the renderer doesn't need it — so
        // we trust block_size to bound the skip rather than reimplementing
        // the full variable layout. See issue #147.
        "BSSubIndexTriShape" => {
            let start = stream.position();
            let shape = tri_shape::BsTriShape::parse(stream)?;
            if let Some(size) = block_size {
                let consumed = stream.position() - start;
                if consumed < size as u64 {
                    stream.skip(size as u64 - consumed);
                }
            }
            Ok(Box::new(shape))
        }
        // BSDynamicTriShape: Skyrim facegen head meshes — BSTriShape body
        // + CPU-mutable trailing Vector4 vertex array. Routing this to
        // NiUnknown caused invisible faces on every NPC. See issue #157.
        "BSDynamicTriShape" => Ok(Box::new(tri_shape::BsTriShape::parse_dynamic(stream)?)),
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
        "NiStringExtraData"
        | "NiBinaryExtraData"
        | "NiIntegerExtraData"
        | "BSXFlags"
        | "NiBooleanExtraData"
        | "NiStringsExtraData"
        | "NiIntegersExtraData" => Ok(Box::new(NiExtraData::parse(stream, type_name)?)),
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
        // NiBSpline* compressed animation (Skyrim / FO4 actor KF files).
        // See issue #155. Only the CompTransform variant is commonly used;
        // the data+basis blocks are shared across all bspline interpolator
        // subclasses. anim.rs evaluates the spline at 30 Hz into TQS keys.
        "NiBSplineCompTransformInterpolator" => {
            Ok(Box::new(NiBSplineCompTransformInterpolator::parse(stream)?))
        }
        "NiBSplineData" => Ok(Box::new(NiBSplineData::parse(stream)?)),
        "NiBSplineBasisData" => Ok(Box::new(NiBSplineBasisData::parse(stream)?)),
        "NiFloatInterpolator" => Ok(Box::new(NiFloatInterpolator::parse(stream)?)),
        "NiFloatData" => Ok(Box::new(NiFloatData::parse(stream)?)),
        // NiUVController + NiUVData — scrolling UV animation, deprecated
        // pre-10.1 and removed at 20.3. See issue #154.
        "NiUVController" => Ok(Box::new(NiUVController::parse(stream)?)),
        "NiUVData" => Ok(Box::new(NiUVData::parse(stream)?)),
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

    /// Helper: encode a pre-20.1 inline length-prefixed string (u32 len + bytes).
    fn inline_string(s: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(s.len() as u32).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
        out
    }

    /// Regression test for issue #164: array-form extra data.
    #[test]
    fn oblivion_strings_and_integers_extra_data_roundtrip() {
        use crate::blocks::extra_data::NiExtraData;

        let header = oblivion_header();

        // NiStringsExtraData: name(empty) + count(3) + 3 inline strings.
        let mut strings_bytes = Vec::new();
        strings_bytes.extend_from_slice(&0u32.to_le_bytes()); // name (empty inline str)
        strings_bytes.extend_from_slice(&3u32.to_le_bytes()); // count
        strings_bytes.extend_from_slice(&inline_string("alpha"));
        strings_bytes.extend_from_slice(&inline_string("beta"));
        strings_bytes.extend_from_slice(&inline_string("gamma"));
        let mut stream = NifStream::new(&strings_bytes, &header);
        let block = parse_block(
            "NiStringsExtraData",
            &mut stream,
            Some(strings_bytes.len() as u32),
        )
        .expect("NiStringsExtraData should dispatch");
        let ed = block
            .as_any()
            .downcast_ref::<NiExtraData>()
            .expect("downcast to NiExtraData");
        let arr = ed.strings_array.as_ref().expect("strings_array populated");
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_deref(), Some("alpha"));
        assert_eq!(arr[1].as_deref(), Some("beta"));
        assert_eq!(arr[2].as_deref(), Some("gamma"));

        // NiIntegersExtraData: name(empty) + count(2) + two u32s.
        let mut ints_bytes = Vec::new();
        ints_bytes.extend_from_slice(&0u32.to_le_bytes()); // name
        ints_bytes.extend_from_slice(&2u32.to_le_bytes()); // count
        ints_bytes.extend_from_slice(&42u32.to_le_bytes());
        ints_bytes.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
        let mut stream = NifStream::new(&ints_bytes, &header);
        let block = parse_block(
            "NiIntegersExtraData",
            &mut stream,
            Some(ints_bytes.len() as u32),
        )
        .expect("NiIntegersExtraData should dispatch");
        let ed = block
            .as_any()
            .downcast_ref::<NiExtraData>()
            .expect("downcast to NiExtraData");
        let arr = ed.integers_array.as_ref().expect("integers_array populated");
        assert_eq!(arr, &vec![42u32, 0xDEADBEEF]);
    }

    /// Oblivion-era empty NiNode body (no children, no effects, no
    /// properties, identity transform). Used as the base bytes for
    /// every NiNode subtype test in this module.
    fn oblivion_empty_ninode_bytes() -> Vec<u8> {
        let mut d = Vec::new();
        // NiObjectNET: name (empty inline) + empty extra data list + null controller
        d.extend_from_slice(&0u32.to_le_bytes()); // name len
        d.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
        d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
        // NiAVObject: flags (u16 for bsver<=26), identity transform (13 f32),
        // empty properties list, null collision ref.
        d.extend_from_slice(&0u16.to_le_bytes()); // flags
        // transform: translation (3 f32)
        d.extend_from_slice(&0.0f32.to_le_bytes());
        d.extend_from_slice(&0.0f32.to_le_bytes());
        d.extend_from_slice(&0.0f32.to_le_bytes());
        // transform: rotation 3x3 identity
        for (i, row) in (0..3).zip([[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]) {
            let _ = i;
            for v in row {
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        // transform: scale
        d.extend_from_slice(&1.0f32.to_le_bytes());
        // properties list: empty
        d.extend_from_slice(&0u32.to_le_bytes());
        // collision_ref: null
        d.extend_from_slice(&(-1i32).to_le_bytes());
        // NiNode children: empty
        d.extend_from_slice(&0u32.to_le_bytes());
        // NiNode effects: empty (Oblivion has_effects_list = true)
        d.extend_from_slice(&0u32.to_le_bytes());
        d
    }

    /// Regression test for issue #142: NiNode subtypes with trailing fields.
    #[test]
    fn oblivion_node_subtypes_dispatch_with_correct_payload() {
        use crate::blocks::node::{
            BsRangeNode, NiBillboardNode, NiLODNode, NiSortAdjustNode, NiSwitchNode,
        };

        let header = oblivion_header();
        let base = oblivion_empty_ninode_bytes();

        // NiBillboardNode: base + billboard_mode u16.
        let mut bb = base.clone();
        bb.extend_from_slice(&3u16.to_le_bytes()); // ALWAYS_FACE_CENTER
        let mut stream = NifStream::new(&bb, &header);
        let block = parse_block("NiBillboardNode", &mut stream, Some(bb.len() as u32))
            .expect("NiBillboardNode dispatch");
        let n = block.as_any().downcast_ref::<NiBillboardNode>().unwrap();
        assert_eq!(n.billboard_mode, 3);
        assert_eq!(stream.position(), bb.len() as u64);

        // NiSwitchNode: base + switch_flags u16 + index u32.
        let mut sw = base.clone();
        sw.extend_from_slice(&0x0003u16.to_le_bytes()); // UpdateOnlyActiveChild | UpdateControllers
        sw.extend_from_slice(&7u32.to_le_bytes());
        let mut stream = NifStream::new(&sw, &header);
        let block = parse_block("NiSwitchNode", &mut stream, Some(sw.len() as u32))
            .expect("NiSwitchNode dispatch");
        let n = block.as_any().downcast_ref::<NiSwitchNode>().unwrap();
        assert_eq!(n.switch_flags, 0x0003);
        assert_eq!(n.index, 7);
        assert_eq!(stream.position(), sw.len() as u64);

        // NiLODNode: NiSwitchNode body + lod_level_data ref i32.
        let mut lod = base.clone();
        lod.extend_from_slice(&0u16.to_le_bytes()); // switch_flags
        lod.extend_from_slice(&0u32.to_le_bytes()); // index
        lod.extend_from_slice(&42i32.to_le_bytes()); // lod_level_data
        let mut stream = NifStream::new(&lod, &header);
        let block = parse_block("NiLODNode", &mut stream, Some(lod.len() as u32))
            .expect("NiLODNode dispatch");
        let n = block.as_any().downcast_ref::<NiLODNode>().unwrap();
        assert_eq!(n.lod_level_data.index(), Some(42));
        assert_eq!(stream.position(), lod.len() as u64);

        // NiSortAdjustNode: base + sorting_mode u32 (v20.0.0.5 > 20.0.0.3 → no
        // trailing accumulator ref).
        let mut sa = base.clone();
        sa.extend_from_slice(&1u32.to_le_bytes()); // SORTING_OFF
        let mut stream = NifStream::new(&sa, &header);
        let block = parse_block("NiSortAdjustNode", &mut stream, Some(sa.len() as u32))
            .expect("NiSortAdjustNode dispatch");
        let n = block.as_any().downcast_ref::<NiSortAdjustNode>().unwrap();
        assert_eq!(n.sorting_mode, 1);
        assert_eq!(stream.position(), sa.len() as u64);

        // BSRangeNode (and its subclasses) — base + 3 bytes.
        for type_name in ["BSRangeNode", "BSBlastNode", "BSDamageStage", "BSDebrisNode"] {
            let mut r = base.clone();
            r.push(5); // min
            r.push(10); // max
            r.push(7); // current
            let mut stream = NifStream::new(&r, &header);
            let block = parse_block(type_name, &mut stream, Some(r.len() as u32))
                .unwrap_or_else(|e| panic!("{type_name} dispatch: {e}"));
            let n = block.as_any().downcast_ref::<BsRangeNode>().unwrap();
            assert_eq!(n.min, 5);
            assert_eq!(n.max, 10);
            assert_eq!(n.current, 7);
            assert_eq!(stream.position(), r.len() as u64);
        }

        // Pure-alias variants — parse as plain NiNode with no trailing bytes.
        for type_name in ["AvoidNode", "NiBSAnimationNode", "NiBSParticleNode"] {
            let mut stream = NifStream::new(&base, &header);
            let block = parse_block(type_name, &mut stream, Some(base.len() as u32))
                .unwrap_or_else(|e| panic!("{type_name} dispatch: {e}"));
            assert!(block.as_any().downcast_ref::<crate::blocks::NiNode>().is_some());
            assert_eq!(stream.position(), base.len() as u64);
        }
    }

    /// Build an "empty NiAVObject" body sized for Oblivion. Same prefix
    /// as the NiNode helper, minus the NiNode-specific children+effects
    /// trailers. Used for NiLight bodies.
    fn oblivion_niavobject_bytes() -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&0u32.to_le_bytes()); // name len (empty inline)
        d.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
        d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
        d.extend_from_slice(&0u16.to_le_bytes()); // flags
        for _ in 0..3 {
            d.extend_from_slice(&0.0f32.to_le_bytes()); // translation
        }
        for row in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
            for v in row {
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
        d.extend_from_slice(&0u32.to_le_bytes()); // empty properties list
        d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
        d
    }

    /// Regression test for issue #156: NiLight hierarchy dispatch + payload.
    #[test]
    fn oblivion_lights_parse_with_attenuation_and_color() {
        use crate::blocks::light::{NiAmbientLight, NiPointLight, NiSpotLight};

        let header = oblivion_header();
        let av = oblivion_niavobject_bytes();

        // Common NiDynamicEffect + NiLight tail for an Oblivion torch:
        //   switch_state:u8=1, num_affected_nodes:u32=0,
        //   dimmer:f32=1.0,
        //   ambient:(0,0,0), diffuse:(1.0, 0.6, 0.2), specular:(0,0,0)
        fn dynamic_light_tail() -> Vec<u8> {
            let mut d = Vec::new();
            d.push(1u8); // switch_state
            d.extend_from_slice(&0u32.to_le_bytes()); // affected nodes count
            d.extend_from_slice(&1.0f32.to_le_bytes()); // dimmer
            for _ in 0..3 {
                d.extend_from_slice(&0.0f32.to_le_bytes()); // ambient color
            }
            for &c in &[1.0f32, 0.6, 0.2] {
                d.extend_from_slice(&c.to_le_bytes()); // diffuse color
            }
            for _ in 0..3 {
                d.extend_from_slice(&0.0f32.to_le_bytes()); // specular color
            }
            d
        }

        // NiAmbientLight: base + dynamic_light_tail, nothing else.
        let mut amb = av.clone();
        amb.extend_from_slice(&dynamic_light_tail());
        let mut stream = NifStream::new(&amb, &header);
        let block = parse_block("NiAmbientLight", &mut stream, Some(amb.len() as u32))
            .expect("NiAmbientLight dispatch");
        let light = block.as_any().downcast_ref::<NiAmbientLight>().unwrap();
        assert_eq!(light.base.dimmer, 1.0);
        assert!((light.base.diffuse_color.g - 0.6).abs() < 1e-6);
        assert_eq!(stream.position(), amb.len() as u64);

        // NiPointLight: base + tail + (const=1.0, lin=0.01, quad=0.0).
        let mut pl = av.clone();
        pl.extend_from_slice(&dynamic_light_tail());
        pl.extend_from_slice(&1.0f32.to_le_bytes()); // constant
        pl.extend_from_slice(&0.01f32.to_le_bytes()); // linear
        pl.extend_from_slice(&0.0f32.to_le_bytes()); // quadratic
        let mut stream = NifStream::new(&pl, &header);
        let block = parse_block("NiPointLight", &mut stream, Some(pl.len() as u32))
            .expect("NiPointLight dispatch");
        let p = block.as_any().downcast_ref::<NiPointLight>().unwrap();
        assert_eq!(p.constant_attenuation, 1.0);
        assert!((p.linear_attenuation - 0.01).abs() < 1e-6);
        assert_eq!(stream.position(), pl.len() as u64);

        // NiSpotLight: NiPointLight body + outer + exponent (Oblivion
        // v20.0.0.5 < 20.2.0.5, so no inner_spot_angle).
        let mut sl = av.clone();
        sl.extend_from_slice(&dynamic_light_tail());
        sl.extend_from_slice(&1.0f32.to_le_bytes()); // constant
        sl.extend_from_slice(&0.01f32.to_le_bytes()); // linear
        sl.extend_from_slice(&0.0f32.to_le_bytes()); // quadratic
        sl.extend_from_slice(&(std::f32::consts::FRAC_PI_4).to_le_bytes()); // outer
        sl.extend_from_slice(&2.0f32.to_le_bytes()); // exponent
        let mut stream = NifStream::new(&sl, &header);
        let block = parse_block("NiSpotLight", &mut stream, Some(sl.len() as u32))
            .expect("NiSpotLight dispatch");
        let s = block.as_any().downcast_ref::<NiSpotLight>().unwrap();
        assert!((s.outer_spot_angle - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
        assert_eq!(s.inner_spot_angle, 0.0); // not in this version
        assert_eq!(s.exponent, 2.0);
        assert_eq!(stream.position(), sl.len() as u64);
    }

    /// Regression test for issue #154: NiUVController + NiUVData.
    #[test]
    fn oblivion_uv_controller_and_data_roundtrip() {
        use crate::blocks::controller::NiUVController;
        use crate::blocks::interpolator::NiUVData;

        let header = oblivion_header();

        // NiUVController: NiTimeControllerBase (26 bytes) + u16 target + i32 data ref.
        let mut uvc = Vec::new();
        uvc.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
        uvc.extend_from_slice(&0u16.to_le_bytes()); // flags
        uvc.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
        uvc.extend_from_slice(&0.0f32.to_le_bytes()); // phase
        uvc.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
        uvc.extend_from_slice(&2.5f32.to_le_bytes()); // stop_time
        uvc.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref
        uvc.extend_from_slice(&0u16.to_le_bytes()); // target_attribute
        uvc.extend_from_slice(&42i32.to_le_bytes()); // data ref
        let mut stream = NifStream::new(&uvc, &header);
        let block = parse_block("NiUVController", &mut stream, Some(uvc.len() as u32))
            .expect("NiUVController dispatch");
        let c = block.as_any().downcast_ref::<NiUVController>().unwrap();
        assert_eq!(c.target_attribute, 0);
        assert_eq!(c.data_ref.index(), Some(42));
        assert!((c.base.stop_time - 2.5).abs() < 1e-6);
        assert_eq!(stream.position(), uvc.len() as u64);

        // NiUVData: four KeyGroup<FloatKey>. First group has 2 linear
        // keys scrolling U from 0→1; the rest are empty.
        let mut uvd = Vec::new();
        // Group 0: num_keys=2, key_type=Linear(1), key (time, value)×2
        uvd.extend_from_slice(&2u32.to_le_bytes());
        uvd.extend_from_slice(&1u32.to_le_bytes()); // KeyType::Linear
        uvd.extend_from_slice(&0.0f32.to_le_bytes()); // t=0
        uvd.extend_from_slice(&0.0f32.to_le_bytes()); // v=0
        uvd.extend_from_slice(&1.0f32.to_le_bytes()); // t=1
        uvd.extend_from_slice(&1.0f32.to_le_bytes()); // v=1
        // Groups 1-3: num_keys=0 (no key_type field when empty).
        for _ in 0..3 {
            uvd.extend_from_slice(&0u32.to_le_bytes());
        }
        let mut stream = NifStream::new(&uvd, &header);
        let block = parse_block("NiUVData", &mut stream, Some(uvd.len() as u32))
            .expect("NiUVData dispatch");
        let d = block.as_any().downcast_ref::<NiUVData>().unwrap();
        assert_eq!(d.groups[0].keys.len(), 2);
        assert_eq!(d.groups[0].keys[1].value, 1.0);
        assert!(d.groups[1].keys.is_empty());
        assert!(d.groups[2].keys.is_empty());
        assert!(d.groups[3].keys.is_empty());
        assert_eq!(stream.position(), uvd.len() as u64);
    }

    /// Regression test for issue #153: NiCamera parsing.
    #[test]
    fn oblivion_ni_camera_roundtrip() {
        use crate::blocks::node::NiCamera;

        let header = oblivion_header();
        let mut bytes = oblivion_niavobject_bytes();
        // camera_flags u16
        bytes.extend_from_slice(&0u16.to_le_bytes());
        // frustum left/right/top/bottom
        bytes.extend_from_slice(&(-0.5f32).to_le_bytes());
        bytes.extend_from_slice(&0.5f32.to_le_bytes());
        bytes.extend_from_slice(&0.3f32.to_le_bytes());
        bytes.extend_from_slice(&(-0.3f32).to_le_bytes());
        // frustum near / far
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&5000.0f32.to_le_bytes());
        // use_orthographic byte bool = 0
        bytes.push(0u8);
        // viewport left/right/top/bottom
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
        // lod_adjust
        bytes.extend_from_slice(&1.5f32.to_le_bytes());
        // scene_ref
        bytes.extend_from_slice(&9i32.to_le_bytes());
        // num_screen_polygons, num_screen_textures (both u32, both 0 on disk)
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());

        let mut stream = NifStream::new(&bytes, &header);
        let block = parse_block("NiCamera", &mut stream, Some(bytes.len() as u32))
            .expect("NiCamera dispatch");
        let c = block.as_any().downcast_ref::<NiCamera>().unwrap();
        assert!((c.frustum_right - 0.5).abs() < 1e-6);
        assert!((c.frustum_far - 5000.0).abs() < 1e-6);
        assert!(!c.use_orthographic);
        assert!((c.lod_adjust - 1.5).abs() < 1e-6);
        assert_eq!(c.scene_ref.index(), Some(9));
        assert_eq!(c.num_screen_polygons, 0);
        assert_eq!(c.num_screen_textures, 0);
        assert_eq!(stream.position(), bytes.len() as u64);
    }

    /// Regression test for issue #163: NiTextureEffect.
    #[test]
    fn oblivion_ni_texture_effect_roundtrip() {
        use crate::blocks::texture::NiTextureEffect;

        let header = oblivion_header();
        let mut bytes = oblivion_niavobject_bytes();
        // NiDynamicEffect base: switch_state=1, num_affected_nodes=0
        bytes.push(1u8);
        bytes.extend_from_slice(&0u32.to_le_bytes());
        // model_projection_matrix: 3x3 identity
        for row in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
            for v in row {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        // model_projection_translation: (0, 0, 0)
        for _ in 0..3 {
            bytes.extend_from_slice(&0.0f32.to_le_bytes());
        }
        // texture_filtering = 2 (trilerp)
        bytes.extend_from_slice(&2u32.to_le_bytes());
        // NO max_anisotropy at 20.0.0.5 (< 20.5.0.4)
        // texture_clamping = 0
        bytes.extend_from_slice(&0u32.to_le_bytes());
        // texture_type = 4 (env map)
        bytes.extend_from_slice(&4u32.to_le_bytes());
        // coordinate_generation_type = 0 (sphere map)
        bytes.extend_from_slice(&0u32.to_le_bytes());
        // source_texture_ref = 17
        bytes.extend_from_slice(&17i32.to_le_bytes());
        // enable_plane = 0
        bytes.push(0u8);
        // plane: normal (0, 1, 0), constant 0.5
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
        bytes.extend_from_slice(&0.5f32.to_le_bytes());
        // NO ps2_l / ps2_k at 20.0.0.5 (> 10.2.0.0)

        let mut stream = NifStream::new(&bytes, &header);
        let block = parse_block("NiTextureEffect", &mut stream, Some(bytes.len() as u32))
            .expect("NiTextureEffect dispatch");
        let e = block.as_any().downcast_ref::<NiTextureEffect>().unwrap();
        assert_eq!(e.texture_filtering, 2);
        assert_eq!(e.texture_type, 4);
        assert_eq!(e.coordinate_generation_type, 0);
        assert_eq!(e.source_texture_ref.index(), Some(17));
        assert!(!e.enable_plane);
        assert!((e.plane[1] - 1.0).abs() < 1e-6);
        assert!((e.plane[3] - 0.5).abs() < 1e-6);
        assert_eq!(e.max_anisotropy, 0); // absent for Oblivion
        assert_eq!(e.ps2_l, 0); // absent for Oblivion
        assert_eq!(stream.position(), bytes.len() as u64);
    }

    /// Regression test for issue #143: legacy particle modifier chain
    /// and NiParticleSystemController. These types ship in every
    /// Oblivion magic FX / fire / dust / blood mesh and hard-fail the
    /// whole file when one is missing (no block_sizes fallback).
    #[test]
    fn oblivion_legacy_particle_modifier_chain_roundtrip() {
        use crate::blocks::legacy_particle::{
            NiGravity, NiParticleBomb, NiParticleColorModifier, NiParticleGrowFade,
            NiParticleRotation, NiPlanarCollider, NiSphericalCollider,
        };

        let header = oblivion_header();

        // Helpers.
        fn niptr_modifier_prefix() -> Vec<u8> {
            // next_modifier = -1, controller = -1
            let mut d = Vec::new();
            d.extend_from_slice(&(-1i32).to_le_bytes());
            d.extend_from_slice(&(-1i32).to_le_bytes());
            d
        }
        fn collider_prefix() -> Vec<u8> {
            let mut d = niptr_modifier_prefix();
            d.extend_from_slice(&0.5f32.to_le_bytes()); // bounce
            d.push(0u8); // spawn_on_collide
            d.push(1u8); // die_on_collide
            d
        }

        // NiParticleColorModifier: base + color_data_ref.
        let mut bytes = niptr_modifier_prefix();
        bytes.extend_from_slice(&7i32.to_le_bytes());
        let mut s = NifStream::new(&bytes, &header);
        let b = parse_block("NiParticleColorModifier", &mut s, Some(bytes.len() as u32)).unwrap();
        let m = b.as_any().downcast_ref::<NiParticleColorModifier>().unwrap();
        assert_eq!(m.color_data_ref.index(), Some(7));
        assert_eq!(s.position(), bytes.len() as u64);

        // NiParticleGrowFade: base + grow + fade.
        let mut bytes = niptr_modifier_prefix();
        bytes.extend_from_slice(&0.25f32.to_le_bytes());
        bytes.extend_from_slice(&0.75f32.to_le_bytes());
        let mut s = NifStream::new(&bytes, &header);
        let b = parse_block("NiParticleGrowFade", &mut s, Some(bytes.len() as u32)).unwrap();
        let m = b.as_any().downcast_ref::<NiParticleGrowFade>().unwrap();
        assert!((m.grow - 0.25).abs() < 1e-6);
        assert!((m.fade - 0.75).abs() < 1e-6);
        assert_eq!(s.position(), bytes.len() as u64);

        // NiParticleRotation: base + random_initial_axis + Vec3 axis + speed.
        let mut bytes = niptr_modifier_prefix();
        bytes.push(1u8);
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
        bytes.extend_from_slice(&2.5f32.to_le_bytes());
        let mut s = NifStream::new(&bytes, &header);
        let b = parse_block("NiParticleRotation", &mut s, Some(bytes.len() as u32)).unwrap();
        let m = b.as_any().downcast_ref::<NiParticleRotation>().unwrap();
        assert!(m.random_initial_axis);
        assert_eq!(m.initial_axis, [0.0, 1.0, 0.0]);
        assert!((m.rotation_speed - 2.5).abs() < 1e-6);
        assert_eq!(s.position(), bytes.len() as u64);

        // NiParticleBomb: base + decay + duration + delta_v + start +
        // decay_type + symmetry_type + position + direction.
        let mut bytes = niptr_modifier_prefix();
        for v in [0.1f32, 1.0, 2.0, 0.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.extend_from_slice(&1u32.to_le_bytes()); // decay_type
        bytes.extend_from_slice(&0u32.to_le_bytes()); // symmetry_type
        for v in [0.0f32, 0.0, 0.0, 0.0, 0.0, 1.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let mut s = NifStream::new(&bytes, &header);
        let b = parse_block("NiParticleBomb", &mut s, Some(bytes.len() as u32)).unwrap();
        let m = b.as_any().downcast_ref::<NiParticleBomb>().unwrap();
        assert_eq!(m.decay_type, 1);
        assert_eq!(m.direction, [0.0, 0.0, 1.0]);
        assert_eq!(s.position(), bytes.len() as u64);

        // NiGravity: base + decay + force + field_type + position + direction.
        let mut bytes = niptr_modifier_prefix();
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // decay
        bytes.extend_from_slice(&9.81f32.to_le_bytes()); // force
        bytes.extend_from_slice(&1u32.to_le_bytes()); // planar field
        for v in [0.0f32, 0.0, 0.0, 0.0, -1.0, 0.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let mut s = NifStream::new(&bytes, &header);
        let b = parse_block("NiGravity", &mut s, Some(bytes.len() as u32)).unwrap();
        let m = b.as_any().downcast_ref::<NiGravity>().unwrap();
        assert!((m.force - 9.81).abs() < 1e-6);
        assert_eq!(m.field_type, 1);
        assert_eq!(m.direction[1], -1.0);
        assert_eq!(s.position(), bytes.len() as u64);

        // NiPlanarCollider: collider_prefix + height + width + position +
        // x_vector + y_vector + NiPlane (vec3 normal + f32 constant).
        let mut bytes = collider_prefix();
        bytes.extend_from_slice(&10.0f32.to_le_bytes()); // height
        bytes.extend_from_slice(&5.0f32.to_le_bytes()); // width
        for v in [0.0f32; 3] {
            bytes.extend_from_slice(&v.to_le_bytes());
        } // position
        for v in [1.0f32, 0.0, 0.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        } // x_vector
        for v in [0.0f32, 0.0, 1.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        } // y_vector
        for v in [0.0f32, 1.0, 0.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        } // plane normal
        bytes.extend_from_slice(&0.25f32.to_le_bytes()); // plane constant
        let mut s = NifStream::new(&bytes, &header);
        let b = parse_block("NiPlanarCollider", &mut s, Some(bytes.len() as u32)).unwrap();
        let m = b.as_any().downcast_ref::<NiPlanarCollider>().unwrap();
        assert!(m.die_on_collide);
        assert!((m.height - 10.0).abs() < 1e-6);
        assert_eq!(m.plane, [0.0, 1.0, 0.0, 0.25]);
        assert_eq!(s.position(), bytes.len() as u64);

        // NiSphericalCollider: collider_prefix + radius + position.
        let mut bytes = collider_prefix();
        bytes.extend_from_slice(&3.5f32.to_le_bytes()); // radius
        for v in [1.0f32, 2.0, 3.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let mut s = NifStream::new(&bytes, &header);
        let b = parse_block("NiSphericalCollider", &mut s, Some(bytes.len() as u32)).unwrap();
        let m = b.as_any().downcast_ref::<NiSphericalCollider>().unwrap();
        assert!((m.radius - 3.5).abs() < 1e-6);
        assert_eq!(m.position, [1.0, 2.0, 3.0]);
        assert_eq!(s.position(), bytes.len() as u64);
    }

    /// Regression test for issue #143: NiParticleSystemController with
    /// zero particles. Verifies the huge scalar field chain consumes
    /// the expected byte count.
    #[test]
    fn oblivion_legacy_particle_system_controller_roundtrip() {
        use crate::blocks::legacy_particle::NiParticleSystemController;

        let header = oblivion_header();

        // NiTimeControllerBase: 26 bytes.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
        bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
        bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
        bytes.extend_from_slice(&3.0f32.to_le_bytes()); // stop_time
        bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref

        // Controller body scalar soup — mostly zeros, non-zero marker
        // values to verify specific field offsets.
        for v in [
            50.0f32, // speed
            5.0,     // speed_variation
            0.0,     // declination
            0.5,     // declination_variation
            0.0,     // planar_angle
            6.28,    // planar_angle_variation
        ] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        // initial_normal (vec3)
        for v in [0.0f32, 0.0, 1.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        // initial_color (RGBA)
        for v in [1.0f32, 0.5, 0.25, 1.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.extend_from_slice(&1.5f32.to_le_bytes()); // initial_size
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // emit_start_time
        bytes.extend_from_slice(&10.0f32.to_le_bytes()); // emit_stop_time
        bytes.push(0u8); // reset_particle_system
        bytes.extend_from_slice(&25.0f32.to_le_bytes()); // birth_rate
        bytes.extend_from_slice(&2.0f32.to_le_bytes()); // lifetime
        bytes.extend_from_slice(&0.5f32.to_le_bytes()); // lifetime_variation
        bytes.push(1u8); // use_birth_rate
        bytes.push(0u8); // spawn_on_death
        for v in [0.0f32; 3] {
            bytes.extend_from_slice(&v.to_le_bytes());
        } // emitter_dimensions
        bytes.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // emitter ptr hash
        bytes.extend_from_slice(&1u16.to_le_bytes()); // num_spawn_generations
        bytes.extend_from_slice(&1.0f32.to_le_bytes()); // percentage_spawned
        bytes.extend_from_slice(&1u16.to_le_bytes()); // spawn_multiplier
        bytes.extend_from_slice(&0.1f32.to_le_bytes()); // spawn_speed_chaos
        bytes.extend_from_slice(&0.1f32.to_le_bytes()); // spawn_dir_chaos

        bytes.extend_from_slice(&0u16.to_le_bytes()); // num_particles
        bytes.extend_from_slice(&0u16.to_le_bytes()); // num_valid
        // No particle records.
        bytes.extend_from_slice(&(-1i32).to_le_bytes()); // unknown_ref
        bytes.extend_from_slice(&0u32.to_le_bytes()); // num_emitter_points
        bytes.extend_from_slice(&0u32.to_le_bytes()); // trailer_emitter_type
        bytes.extend_from_slice(&0.0f32.to_le_bytes()); // unknown_trailer_float
        bytes.extend_from_slice(&(-1i32).to_le_bytes()); // trailer_emitter_modifier

        let mut s = NifStream::new(&bytes, &header);
        let b = parse_block(
            "NiParticleSystemController",
            &mut s,
            Some(bytes.len() as u32),
        )
        .expect("NiParticleSystemController dispatch");
        let c = b
            .as_any()
            .downcast_ref::<NiParticleSystemController>()
            .unwrap();
        assert!((c.speed - 50.0).abs() < 1e-6);
        assert!((c.birth_rate - 25.0).abs() < 1e-6);
        assert!((c.lifetime - 2.0).abs() < 1e-6);
        assert_eq!(c.emitter, 0xDEADBEEF);
        assert_eq!(c.num_particles, 0);
        assert_eq!(s.position(), bytes.len() as u64);

        // NiBSPArrayController aliases to the same parser with the
        // identical payload — verify it dispatches.
        let mut s = NifStream::new(&bytes, &header);
        let b = parse_block("NiBSPArrayController", &mut s, Some(bytes.len() as u32))
            .expect("NiBSPArrayController dispatch");
        assert!(b
            .as_any()
            .downcast_ref::<NiParticleSystemController>()
            .is_some());
    }
}
