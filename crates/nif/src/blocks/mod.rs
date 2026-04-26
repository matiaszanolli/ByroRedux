//! NIF block type dispatch.
//!
//! Each serialized block in a NIF file has an RTTI class name from the
//! header's block type table. This module maps those names to parsers
//! and provides the NiObject trait that all parsed blocks implement.

pub mod base;
pub mod bs_geometry;
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
    BhkAabbPhantom, BhkBoxShape, BhkBreakableConstraint, BhkCapsuleShape, BhkCollisionObject,
    BhkCompressedMeshShape, BhkCompressedMeshShapeData, BhkConstraint, BhkConvexListShape,
    BhkConvexVerticesShape, BhkCylinderShape, BhkLiquidAction, BhkListShape, BhkMoppBvTreeShape,
    BhkMultiSphereShape, BhkNiTriStripsShape, BhkOrientHingedBodyAction, BhkPCollisionObject,
    BhkPackedNiTriStripsShape, BhkRigidBody, BhkSimpleShapePhantom, BhkSphereShape,
    BhkTransformShape, HkPackedNiTriStripsData, NiCollisionObjectBase,
};
use controller::{
    BhkBlendController, BsNiAlphaPropertyTestRefController, BsRefractionFirePeriodController,
    NiBsBoneLodController, NiControllerManager, NiControllerSequence, NiFlipController,
    NiFloatExtraDataController, NiGeomMorpherController, NiLightColorController,
    NiLightFloatController, NiLookAtController, NiMaterialColorController, NiMorphData,
    NiMultiTargetTransformController, NiPathController, NiSequenceStreamHelper,
    NiSingleInterpController, NiTimeController, NiUVController,
};
use extra_data::{
    BsAnimNote, BsAnimNotes, BsBehaviorGraphExtraData, BsBound, BsClothExtraData,
    BsConnectPointChildren, BsConnectPointParents, BsDecalPlacementVectorExtraData,
    BsFurnitureMarker, BsInvMarker, BsWArray, NiExtraData,
};
use interpolator::{
    NiBSplineBasisData, NiBSplineCompTransformInterpolator, NiBSplineData, NiBlendBoolInterpolator,
    NiBlendFloatInterpolator, NiBlendPoint3Interpolator, NiBlendTransformInterpolator, NiBoolData,
    NiBoolInterpolator, NiColorData, NiColorInterpolator, NiFloatData, NiFloatInterpolator,
    NiLookAtInterpolator, NiPathInterpolator, NiPoint3Interpolator, NiPosData, NiTextKeyExtraData,
    NiTransformData, NiTransformInterpolator, NiUVData,
};
use multibound::{BsMultiBound, BsMultiBoundAABB, BsMultiBoundOBB, BsMultiBoundSphere};
use node::{BsOrderedNode, BsValueNode, NiNode};
use properties::{
    NiAlphaProperty, NiFlagProperty, NiFogProperty, NiMaterialProperty, NiStencilProperty,
    NiStringPalette, NiTexturingProperty, NiVertexColorProperty, NiZBufferProperty,
};
use shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderNoLightingProperty,
    BSShaderPPLightingProperty, BSShaderTextureSet, SkyShaderProperty, TallGrassShaderProperty,
    TileShaderProperty, WaterShaderProperty,
};
use skin::{
    BsDismemberSkinInstance, BsSkinBoneData, BsSkinInstance, NiSkinData, NiSkinInstance,
    NiSkinPartition,
};
use std::any::Any;
use std::fmt::Debug;
use std::io;
use std::sync::Arc;
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
    /// Original block type name from the NIF header. `Arc<str>` instead
    /// of `String` to avoid a per-unknown-block allocation — many unknown
    /// blocks share the same type name. See #248.
    pub type_name: Arc<str>,
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
        | "RootCollisionNode"
        | "AvoidNode"
        | "NiBSAnimationNode"
        | "NiBSParticleNode" => Ok(Box::new(NiNode::parse(stream)?)),
        // BSMultiBoundNode: NiNode + multi_bound_ref + (Skyrim+) culling_mode.
        // See issue #148. Previously aliased to plain NiNode, dropping the
        // multi_bound linkage to BSMultiBoundAABB volumes.
        "BSMultiBoundNode" => Ok(Box::new(node::BsMultiBoundNode::parse(stream)?)),
        // BSTreeNode: Skyrim SpeedTree root with two trailing NiNode ref lists
        // (branch roots + trunk bones) for wind simulation. Previously aliased
        // to plain NiNode, silently dropping both ref lists. See #159.
        "BSTreeNode" => Ok(Box::new(node::BsTreeNode::parse(stream)?)),
        // NiNode subtypes with a small payload of trailing fields.
        "NiBillboardNode" => Ok(Box::new(node::NiBillboardNode::parse(stream)?)),
        "NiSwitchNode" => Ok(Box::new(node::NiSwitchNode::parse(stream)?)),
        "NiLODNode" => Ok(Box::new(node::NiLODNode::parse(stream)?)),
        "NiSortAdjustNode" => Ok(Box::new(node::NiSortAdjustNode::parse(stream)?)),
        // BSRangeNode + subclasses — all carry the same (min, max, current)
        // byte triple and are FO3+. BSBlastNode / BSDamageStage / BSDebrisNode
        // inherit BSRangeNode and add nothing on the wire — but the
        // discriminator matters to gameplay-side systems (destruction
        // sequence vs blast effect vs debris ejection root). Stamp the
        // wire type name onto `BsRangeNode.kind` so consumers can branch
        // without re-running the dispatch from `original_type`. See #364.
        "BSRangeNode" => Ok(Box::new(node::BsRangeNode::parse(stream)?)),
        "BSDamageStage" => Ok(Box::new(
            node::BsRangeNode::parse(stream)?.with_kind(node::BsRangeKind::DamageStage),
        )),
        "BSBlastNode" => Ok(Box::new(
            node::BsRangeNode::parse(stream)?.with_kind(node::BsRangeKind::Blast),
        )),
        "BSDebrisNode" => Ok(Box::new(
            node::BsRangeNode::parse(stream)?.with_kind(node::BsRangeKind::Debris),
        )),
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
        "BSMultiBoundSphere" => Ok(Box::new(BsMultiBoundSphere::parse(stream)?)),
        "NiTriShape" | "NiTriStrips" => Ok(Box::new(NiTriShape::parse(stream)?)),
        // BSSegmentedTriShape: FO3/FNV/SkyrimLE biped body-part segmentation.
        // Inherits NiTriShape and adds a trailing (u32 num_segments) +
        // (u8 flags + u32 index + u32 num_tris_in_segment)[num_segments]
        // array. Previously aliased to plain NiTriShape, leaving those
        // bytes unread and relying on block-loop realignment. See #146.
        "BSSegmentedTriShape" => Ok(Box::new(NiTriShape::parse_segmented(stream)?)),
        "BSTriShape" => Ok(Box::new(tri_shape::BsTriShape::parse(stream)?)),
        // BSMeshLODTriShape / BSLODTriShape: same 3-u32 LOD-size trailing
        // layout. BSLODTriShape is the FO4 distant-LOD variant;
        // BSMeshLODTriShape appears in Skyrim SE DLC. `parse_lod` stamps
        // `LOD { lod0, lod1, lod2 }`; for BSMeshLODTriShape we overwrite
        // via `with_kind(MeshLOD)` so downstream importers can tell the
        // two wire subclasses apart. See issue #147, #157, #560.
        "BSLODTriShape" => Ok(Box::new(tri_shape::BsTriShape::parse_lod(stream)?)),
        "BSMeshLODTriShape" => Ok(Box::new(
            tri_shape::BsTriShape::parse_lod(stream)?.with_kind(tri_shape::BsTriShapeKind::MeshLOD),
        )),
        // BSSubIndexTriShape: ubiquitous in Skyrim SE DLC and all FO4 actor
        // meshes (clothing segmentation for dismemberment). #404 replaced
        // the previous `block_size`-driven skip with a structured decode
        // — the per-segment bone-slot flags + SSF filename + cut offsets
        // are now recovered into `BsTriShapeKind::SubIndex(_)` so the
        // M-series combat / locational-damage roadmap has the data it
        // needs. The full layout differs between SSE (`bsver == 100`,
        // pre-FO4 single-byte flags) and FO4+/FO76 (`bsver >= 130`,
        // sub-segment lists + optional shared-data trailer with .ssf
        // filename). See `BsTriShape::parse_sub_index` doc-comment.
        "BSSubIndexTriShape" => {
            // Pass `block_size` through so parse_sub_index can locally
            // swallow segmentation-decode errors and skip past the
            // remaining block bytes — we never want a malformed
            // segmentation trailer to take down the BSTriShape body
            // (which is what the renderer actually consumes). Pre-#404
            // this whole block was a wholesale skip, so any
            // post-#404 segmentation parse failure must degrade to at
            // least that level of robustness, never worse.
            Ok(Box::new(tri_shape::BsTriShape::parse_sub_index(
                stream, block_size,
            )?))
        }
        // BSDynamicTriShape: Skyrim facegen head meshes — BSTriShape body
        // + CPU-mutable trailing Vector4 vertex array. Routing this to
        // NiUnknown caused invisible faces on every NPC. See issue #157.
        "BSDynamicTriShape" => Ok(Box::new(tri_shape::BsTriShape::parse_dynamic(stream)?)),
        // BSGeometry: Starfield-era replacement for BSTriShape /
        // BSSubIndexTriShape. The .nif holds bounds + skin/shader/alpha
        // refs + up to 4 mesh-LOD slots; each slot carries either an
        // external `.mesh` filename (the 99% Starfield case) or — when
        // bit 0x200 of the parent's flags is set — an inline mesh body
        // (UDEC3 packed normals/tangents, half-float UVs, meshlets, cull
        // data). Pre-#708 every Starfield mesh fell into NiUnknown:
        // 190,549 hits in `Starfield - Meshes01.ba2` (24.74% of every
        // block). Wire layout sourced from nifly's BSGeometry::Sync
        // (Geometry.cpp:1769); nif.xml has no <niobject> for this block.
        "BSGeometry" => Ok(Box::new(bs_geometry::BSGeometry::parse(stream)?)),
        "NiTriShapeData" => Ok(Box::new(NiTriShapeData::parse(stream)?)),
        "NiTriStripsData" => Ok(Box::new(NiTriStripsData::parse(stream)?)),
        // NiAdditionalGeometryData + BSPackedAdditionalGeometryData: per-vertex
        // aux channels (tangents / bitangents / blend weights) referenced by
        // NiGeometryData.additional_data_ref. 4,039 FO3+FNV vanilla blocks
        // fell into NiUnknown pre-fix. The packed variant appears only in
        // older FNV DLC (nvdlc01vaultposter01.nif) and serializes two extra
        // u32s per data block. See issue #547.
        "NiAdditionalGeometryData" => Ok(Box::new(tri_shape::NiAdditionalGeometryData::parse(
            stream,
        )?)),
        "BSPackedAdditionalGeometryData" => Ok(Box::new(
            tri_shape::NiAdditionalGeometryData::parse_packed(stream)?,
        )),
        // All Oblivion-era BSShaderLightingProperty specializations share the
        // base texture-set + flags layout, so alias them to BSShaderPPLighting.
        // Specializing the differences (e.g. sky scroll, water reflection) can
        // come later — for now this unblocks Oblivion exterior cells, which
        // hard-failed on any of these.
        // Blocks that genuinely inherit BSShaderPPLightingProperty (or
        // haven't been split out yet — SkyShaderProperty/HairShaderProperty/
        // VolumetricFogShaderProperty/DistantLODShaderProperty/
        // BSDistantTreeShaderProperty/BSSkyShaderProperty/BSWaterShaderProperty
        // are still aliased pending follow-up per-game validation). Pre-#474
        // `WaterShaderProperty` and `TallGrassShaderProperty` were here too
        // but over-read 20+ bytes — split out below.
        "BSShaderPPLightingProperty"
        | "Lighting30ShaderProperty"
        | "HairShaderProperty"
        | "VolumetricFogShaderProperty"
        | "DistantLODShaderProperty"
        | "BSDistantTreeShaderProperty"
        | "BSSkyShaderProperty"
        | "BSWaterShaderProperty" => Ok(Box::new(BSShaderPPLightingProperty::parse(stream)?)),
        // FO3/FNV `SkyShaderProperty` — inherits `BSShaderLightingProperty`
        // + `File Name: SizedString` + `Sky Object Type: u32` (nif.xml
        // line 6335). Pre-#550 aliased to BSShaderPPLightingProperty::parse
        // which over-read 20-28 bytes (texture_set_ref + refraction +
        // parallax) and dropped the actual sky filename + object type on
        // the floor — every sky NIF rendered with default cloud scroll
        // and horizon fade. block_sizes masked the drift at parse time.
        // Recurring warning bucket `consumed 54, expected 42-82`.
        "SkyShaderProperty" => Ok(Box::new(SkyShaderProperty::parse(stream)?)),
        // FO3/FNV WaterShaderProperty inherits BSShaderProperty directly
        // (no texture_clamp_mode, no texture_set, no refraction/parallax)
        // — see nif.xml line 6322 and issue #474.
        "WaterShaderProperty" => Ok(Box::new(WaterShaderProperty::parse(stream)?)),
        // FO3/FNV TallGrassShaderProperty inherits BSShaderProperty + adds
        // a SizedString File Name — see nif.xml line 6354 and issue #474.
        "TallGrassShaderProperty" => Ok(Box::new(TallGrassShaderProperty::parse(stream)?)),
        // FO3-only `TileShaderProperty` — inherits `BSShaderLightingProperty`
        // + File Name SizedString. Pre-#455 was aliased to
        // BSShaderPPLightingProperty::parse, which over-read 20-28
        // bytes (texture_set_ref + refraction + parallax) and dropped
        // the actual filename on the floor. HUD overlays (stealth
        // meter, airtimer, quest markers) rendered without their
        // texture paths bound. block_sizes recovery kept the stream
        // aligned so the defect was silent at parse time.
        "TileShaderProperty" => Ok(Box::new(TileShaderProperty::parse(stream)?)),
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
        "NiFogProperty" => Ok(Box::new(NiFogProperty::parse(stream)?)),
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
        | "NiIntegersExtraData"
        // NiFloatExtraData / NiFloatsExtraData: single-float + float-array
        // metadata tags (nif.xml lines 4264, 4269). Pre-#553 absent from
        // dispatch — 1,492 SE + 156 FO3/FNV blocks fell into NiUnknown
        // and every tool-authored FOV/scale/wetness knob was lost.
        | "NiFloatExtraData"
        | "NiFloatsExtraData"
        // BSBoneLODExtraData (Skyrim+): bone-LOD distance thresholds for
        // skeleton mesh swapping. Pre-#614 every Skyrim SE skeleton.nif
        // (52 files in vanilla Meshes0.bsa) fell into NiUnknown because
        // the type name had no dispatch arm; the fallback consumed the
        // bytes via block_size recovery but recorded the file as
        // truncated, dropping the parse rate from 100% to ~99.7%.
        | "BSBoneLODExtraData"
        // SkinAttach / BoneTranslations (Starfield, #708 / NIF-D5-02 +
        // NIF-D5-08). Pair with BSGeometry to attach the mesh to a
        // skeleton and supply per-bone translation deltas at LOD
        // boundaries. Wire layouts sourced from nifly's
        // SkinAttach::Sync and BoneTranslations::Sync (ExtraData.cpp:436/441);
        // nif.xml does not define either block.
        | "SkinAttach"
        | "BoneTranslations" => Ok(Box::new(NiExtraData::parse(stream, type_name)?)),
        "BSWArray" => Ok(Box::new(BsWArray::parse(stream)?)),
        "BSBound" => Ok(Box::new(BsBound::parse(stream)?)),
        "BSDecalPlacementVectorExtraData" => {
            Ok(Box::new(BsDecalPlacementVectorExtraData::parse(stream)?))
        }
        "BSBehaviorGraphExtraData" => Ok(Box::new(BsBehaviorGraphExtraData::parse(stream)?)),
        // BSAnimNote / BSAnimNotes — IK event hints on FO3+ animation
        // sequences (grab-IK arm picks, look-IK target tracking). Before
        // #432 these landed on `NiUnknown`, silently dropping the hint
        // data after `block_size` recovery consumed the bytes.
        "BSAnimNote" => Ok(Box::new(BsAnimNote::parse(stream)?)),
        "BSAnimNotes" => Ok(Box::new(BsAnimNotes::parse(stream)?)),
        "BSInvMarker" => Ok(Box::new(BsInvMarker::parse(stream)?)),
        // BSFurnitureMarker / BSFurnitureMarkerNode — sitting/sleeping/leaning
        // positions attached to furniture meshes (chairs, beds, leaning spots).
        // BSVER ≤ 34 (Oblivion/FO3/FNV) uses orientation+refs; BSVER > 34
        // (Skyrim+) uses heading+animation type+entry properties. BSFurnitureMarkerNode
        // (Skyrim+) shares the BSFurnitureMarker wire layout.
        "BSFurnitureMarker" => Ok(Box::new(BsFurnitureMarker::parse(stream, "BSFurnitureMarker")?)),
        "BSFurnitureMarkerNode" => Ok(Box::new(BsFurnitureMarker::parse(
            stream,
            "BSFurnitureMarkerNode",
        )?)),
        "BSClothExtraData" => Ok(Box::new(BsClothExtraData::parse(stream)?)),
        "BSConnectPoint::Parents" => Ok(Box::new(BsConnectPointParents::parse(stream)?)),
        "BSConnectPoint::Children" => Ok(Box::new(BsConnectPointChildren::parse(stream)?)),
        // BSPackedCombined[Shared]GeomDataExtra — FO4+ distant-LOD
        // merged geometry batches attached to BSMultiBoundNode roots in
        // cell LOD NIFs. The fixed-layout header is parsed; the
        // variable-size per-object data + vertex/triangle pools are
        // skipped via block_size until a downstream LOD importer picks
        // them up (terrain streaming milestone). See issue #158.
        "BSPackedCombinedGeomDataExtra" | "BSPackedCombinedSharedGeomDataExtra" => {
            let start = stream.position();
            let type_name_static: &'static str = match type_name {
                "BSPackedCombinedGeomDataExtra" => "BSPackedCombinedGeomDataExtra",
                "BSPackedCombinedSharedGeomDataExtra" => "BSPackedCombinedSharedGeomDataExtra",
                _ => unreachable!(),
            };
            let block = extra_data::BsPackedCombinedGeomDataExtra::parse(stream, type_name_static)?;
            if let Some(size) = block_size {
                let consumed = stream.position() - start;
                if consumed < size as u64 {
                    stream.skip(size as u64 - consumed)?;
                }
            }
            Ok(Box::new(block))
        }
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
        // Pure NiFloatInterpController subclasses (no extra fields beyond
        // NiSingleInterpController). FO3+ era — block_size recovery catches
        // any stream drift. See issue #235.
        | "BSMaterialEmittanceMultController"
        | "BSRefractionStrengthController"
        | "BSFrustumFOVController" => {
            Ok(Box::new(NiSingleInterpController::parse(stream)?))
        }
        // Inherits NiTimeController directly with one explicit interpolator ref.
        // Not NiSingleInterpController (different base class per nif.xml line 6830).
        "BSRefractionFirePeriodController" => {
            Ok(Box::new(BsRefractionFirePeriodController::parse(stream)?))
        }
        // BSNiAlphaPropertyTestRefController: animates NiAlphaProperty's
        // alpha-test threshold for dissolve / fade / ghost-reveal VFX.
        // Inherits NiFloatInterpController → NiSingleInterpController
        // with no additional fields (nif.xml line 6279). Wrapped in a
        // dedicated newtype so telemetry preserves the RTTI name.
        // 751 Skyrim SE vanilla blocks pre-#552.
        "BSNiAlphaPropertyTestRefController" => Ok(Box::new(
            BsNiAlphaPropertyTestRefController::parse(stream)?,
        )),
        // NiFloatExtraDataController: animates a NiFloatExtraData tag
        // (FOV multipliers, scale overrides, wetness levels). nif.xml
        // line 3797 — NiTimeController base + interpolator_ref +
        // extra_data_name. 1,312 data + 180 controller blocks on SE
        // plus smaller FO3/FNV counts pre-#553.
        "NiFloatExtraDataController" => {
            Ok(Box::new(NiFloatExtraDataController::parse(stream)?))
        }
        // NiLightColorController (nif.xml line 3776): NiPoint3InterpController
        // + `Target Color: LightColor (u16)`. Animates the ambient /
        // diffuse color slot of an NiLight. See #433.
        "NiLightColorController" => Ok(Box::new(NiLightColorController::parse(stream)?)),
        // NiLightDimmerController / NiLightIntensityController /
        // NiLightRadiusController: NiFloatInterpController with no
        // additional fields (nif.xml lines 3750 / 5025 / 8444). All
        // three drive NiLight float slots — lantern flicker, campfire
        // pulse, torch flicker, plasma-weapon glow, etc. See #433.
        "NiLightDimmerController" => Ok(Box::new(NiLightFloatController::parse(
            stream,
            "NiLightDimmerController",
        )?)),
        "NiLightIntensityController" => Ok(Box::new(NiLightFloatController::parse(
            stream,
            "NiLightIntensityController",
        )?)),
        "NiLightRadiusController" => Ok(Box::new(NiLightFloatController::parse(
            stream,
            "NiLightRadiusController",
        )?)),
        // BSEffectShader / BSLightingShader property-controller family —
        // each adds a single trailing `controlled_variable: u32` enum to
        // NiSingleInterpController per nif.xml line 6253-6276. Before
        // #407 this u32 was unconsumed, so every block over-read by 4
        // bytes and `block_size` recovery seeked past — 5,264 occurrences
        // in vanilla `Meshes.ba2` alone (the largest single source of
        // drift in the FO4 corpus). The wrapper preserves the original
        // type name in telemetry while consuming the extra u32.
        "BSEffectShaderPropertyFloatController"
        | "BSLightingShaderPropertyFloatController"
        | "BSEffectShaderPropertyColorController"
        | "BSLightingShaderPropertyColorController"
        | "BSLightingShaderPropertyUShortController" => {
            let type_name_static: &'static str = match type_name {
                "BSEffectShaderPropertyFloatController" => {
                    "BSEffectShaderPropertyFloatController"
                }
                "BSLightingShaderPropertyFloatController" => {
                    "BSLightingShaderPropertyFloatController"
                }
                "BSEffectShaderPropertyColorController" => {
                    "BSEffectShaderPropertyColorController"
                }
                "BSLightingShaderPropertyColorController" => {
                    "BSLightingShaderPropertyColorController"
                }
                "BSLightingShaderPropertyUShortController" => {
                    "BSLightingShaderPropertyUShortController"
                }
                _ => unreachable!(),
            };
            Ok(Box::new(controller::BsShaderController::parse(
                stream,
                type_name_static,
            )?))
        }
        // Bethesda / Fallout controller types that extend NiTimeController
        // or NiInterpController with additional fields we don't model yet.
        // Dispatch to the NiTimeController base-parse stub so the RTTI name
        // is preserved in telemetry; block_size recovery on FO3+ seeks past
        // any trailing extra data. Oblivion-era files never reference these
        // types. See issues #234, #235.
        "BSLagBoneController"                        // base + 3 floats
        | "BSKeyframeController"                     // NiSingleInterpController + Data2 ref
        | "BSProceduralLightningController"          // base + 3 interp refs + strip data
        | "NiMorpherController"                      // base + NiMorphData ref
        | "NiMorphController"                        // base (no extra fields in nif.xml)
        | "NiMorphWeightsController" => {            // base + interpolator / target arrays
            Ok(Box::new(NiTimeController::parse(stream)?))
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
        // NiLookAtController + NiPathController — legacy NiTimeController
        // subclasses for look-at constraints and spline path following.
        // DEPRECATED (10.2), REMOVED (20.5) — appear in Oblivion/FO3/FNV/
        // Skyrim-LE cutscenes and environmental animations. Post-Skyrim-LE
        // content replaced them with NiTransformController + NiLookAt/
        // NiPathInterpolator. Parsed so the blocks land in `NifScene`
        // intact — ECS-side constraint systems are a later follow-up.
        // See issue #228.
        "NiLookAtController" => Ok(Box::new(NiLookAtController::parse(stream)?)),
        "NiPathController" => Ok(Box::new(NiPathController::parse(stream)?)),
        "NiPoint3Interpolator" => Ok(Box::new(NiPoint3Interpolator::parse(stream)?)),
        "NiPosData" => Ok(Box::new(NiPosData::parse(stream)?)),
        // NiColorInterpolator + NiColorData — RGBA key-based animation
        // used by every BSEffectShaderPropertyColorController /
        // BSLightingShaderPropertyColorController and historical
        // NiMaterialColorController authored with a color interpolator.
        // Pre-#431 both landed as NiUnknown and every animated emissive
        // silently played with a default color. See #431.
        "NiColorInterpolator" => Ok(Box::new(NiColorInterpolator::parse(stream)?)),
        "NiColorData" => Ok(Box::new(NiColorData::parse(stream)?)),
        // NiPathInterpolator — spline-path motion driver (door hinges,
        // pendulums, wind-turbine blades). Parsed so the
        // block_sizes-less Oblivion loader doesn't truncate the
        // rest of the NIF. See #394 / audit OBL-D5-H2.
        "NiPathInterpolator" => Ok(Box::new(NiPathInterpolator::parse(stream)?)),
        // NiLookAtInterpolator — replaces the deprecated NiLookAtController
        // from 10.2 onwards; drives a plain NiTransformController to keep
        // an axis tracking a target NiNode. 18 instances per FNV mesh
        // sweep landed in NiUnknown pre-fix — surfaced by the R3 per-
        // block histogram.
        "NiLookAtInterpolator" => Ok(Box::new(NiLookAtInterpolator::parse(stream)?)),
        // NiFlipController — flipbook / texture-cycle animation
        // (water ripples, fire flicker, caustics). Oblivion-era
        // content via the #394 sweep. See audit OBL-D5-H2.
        "NiFlipController" => Ok(Box::new(NiFlipController::parse(stream)?)),
        // NiBSBoneLODController — Bethesda creature-skeleton LOD
        // switcher. 34 vanilla Oblivion creature NIFs trip this
        // block; pre-#394 every block after it was discarded
        // because Oblivion has no block_sizes recovery. See audit
        // OBL-D5-H2.
        "NiBSBoneLODController" => Ok(Box::new(NiBsBoneLodController::parse(stream)?)),
        "NiBoolInterpolator" => Ok(Box::new(NiBoolInterpolator::parse(stream)?)),
        // NiBoolTimelineInterpolator — same wire layout as NiBoolInterpolator
        // (nif.xml line 3287 adds no fields); only the semantics differ
        // (ensures no key is missed between updates). 8,450 blocks across
        // FO3 + FNV + Skyrim SE fell into NiUnknown pre-fix. #548.
        "NiBoolTimelineInterpolator" => {
            Ok(Box::new(NiBoolInterpolator::parse_timeline(stream)?))
        }
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
        // NiCollisionObject (non-Havok base): occasionally appears as a
        // direct block on Oblivion scenes. Reading even the 4-byte base
        // keeps the parse loop alive on Oblivion (no block_sizes). #125.
        "NiCollisionObject" => Ok(Box::new(NiCollisionObjectBase::parse(stream)?)),
        "bhkCollisionObject" | "bhkSPCollisionObject" => {
            Ok(Box::new(BhkCollisionObject::parse(stream, false)?))
        }
        "bhkBlendCollisionObject" => Ok(Box::new(BhkCollisionObject::parse(stream, true)?)),
        // bhkBlendController: Havok ragdoll blend-weight controller on
        // FO3 + FNV skeletons. nif.xml line 3927 — NiTimeController base
        // + trailing `Keys: uint`. 1,427 vanilla blocks pre-#551. See #551.
        "bhkBlendController" => Ok(Box::new(BhkBlendController::parse(stream)?)),
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
        // Havok constraints (#117) — minimal stub parse so Oblivion
        // no longer cascades on these. The constraint CInfo layouts
        // are skipped by a hand-computed byte size on Oblivion and
        // by block_size recovery on FO3+. See BhkConstraint::parse.
        "bhkBallAndSocketConstraint" => {
            Ok(Box::new(BhkConstraint::parse(stream, "bhkBallAndSocketConstraint")?))
        }
        "bhkHingeConstraint" => {
            Ok(Box::new(BhkConstraint::parse(stream, "bhkHingeConstraint")?))
        }
        "bhkLimitedHingeConstraint" => {
            Ok(Box::new(BhkConstraint::parse(stream, "bhkLimitedHingeConstraint")?))
        }
        "bhkRagdollConstraint" => {
            Ok(Box::new(BhkConstraint::parse(stream, "bhkRagdollConstraint")?))
        }
        "bhkPrismaticConstraint" => {
            Ok(Box::new(BhkConstraint::parse(stream, "bhkPrismaticConstraint")?))
        }
        "bhkStiffSpringConstraint" => {
            Ok(Box::new(BhkConstraint::parse(stream, "bhkStiffSpringConstraint")?))
        }
        "bhkMalleableConstraint" => {
            Ok(Box::new(BhkConstraint::parse(stream, "bhkMalleableConstraint")?))
        }
        // Havok sphere-cluster collision (#394 / OBL-D5-H2). Oblivion
        // creature ragdolls ship these as compact bounding-volume
        // approximations. Without a parser the block_sizes-less
        // Oblivion loader couldn't skip past this block and truncated
        // the rest of the NIF.
        "bhkMultiSphereShape" => Ok(Box::new(BhkMultiSphereShape::parse(stream)?)),
        // Havok tail types (#557 / NIF-12). Low-volume leaf types that
        // landed in NiUnknown across all four games pre-fix. Each
        // parser is byte-exact where the type is Oblivion-reachable
        // (no block_sizes recovery there) and short-stub on FO3+.
        "bhkAabbPhantom" => Ok(Box::new(BhkAabbPhantom::parse(stream)?)),
        "bhkPCollisionObject" => Ok(Box::new(BhkPCollisionObject::parse(stream)?)),
        "bhkLiquidAction" => Ok(Box::new(BhkLiquidAction::parse(stream)?)),
        "bhkConvexListShape" => Ok(Box::new(BhkConvexListShape::parse(stream)?)),
        "bhkBreakableConstraint" => Ok(Box::new(BhkBreakableConstraint::parse(stream)?)),
        "bhkOrientHingedBodyAction" => Ok(Box::new(BhkOrientHingedBodyAction::parse(stream)?)),
        // FO4 / FO76 NP physics family (#124 / audit NIF-513). Parsing
        // the outer NIF shells so downstream systems can resolve the
        // references without the full Havok-serialised body inside
        // `ByteArray`. The binary blob is kept verbatim in the parsed
        // struct for eventual hand-off to a Havok parser.
        "bhkNPCollisionObject" => Ok(Box::new(collision::BhkNPCollisionObject::parse(stream)?)),
        "bhkPhysicsSystem" => Ok(Box::new(collision::BhkSystemBinary::parse(
            stream,
            "bhkPhysicsSystem",
        )?)),
        "bhkRagdollSystem" => Ok(Box::new(collision::BhkSystemBinary::parse(
            stream,
            "bhkRagdollSystem",
        )?)),
        _ => {
            // Unknown block type — skip it if we know the size
            if let Some(size) = block_size {
                let start = stream.position();
                stream.skip(size as u64)?;
                log::debug!(
                    "Skipping unknown block type '{}' ({} bytes at offset {})",
                    type_name,
                    size,
                    start
                );
                Ok(Box::new(NiUnknown {
                    type_name: Arc::from(type_name),
                    data: Vec::new(),
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
#[path = "dispatch_tests.rs"]
mod dispatch_tests;
