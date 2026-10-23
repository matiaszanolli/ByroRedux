//! NIF block type dispatch.
//!
//! Each serialized block in a NIF file has an RTTI class name from the
//! header's block type table. This module maps those names to parsers
//! and provides the NiObject trait that all parsed blocks implement.

pub mod base;
mod dispatch;
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
pub mod strip;
pub mod texture;
pub mod traits;
pub mod tri_shape;

use crate::stream::NifStream;
use collision::{
    BhkAabbPhantom, BhkBoxShape, BhkBreakableConstraint, BhkCapsuleShape, BhkCollisionObject,
    BhkCompressedMeshShape, BhkCompressedMeshShapeData, BhkConstraint, BhkConvexListShape,
    BhkConvexSweepShape, BhkConvexVerticesShape, BhkCylinderShape, BhkLiquidAction, BhkListShape,
    BhkMeshShape, BhkMoppBvTreeShape, BhkMultiSphereShape, BhkNiTriStripsShape,
    BhkOrientHingedBodyAction, BhkPCollisionObject, BhkPackedNiTriStripsShape, BhkPlaneShape,
    BhkPoseArray, BhkRagdollTemplate, BhkRagdollTemplateData, BhkRigidBody, BhkSimpleShapePhantom,
    BhkSphereShape, BhkTransformShape, HkPackedNiTriStripsData, NiCollisionObjectBase,
};
use controller::{
    BhkBlendController, BsNiAlphaPropertyTestRefController, BsRefractionFirePeriodController,
    NiBsBoneLodController, NiControllerManager, NiControllerSequence, NiFlipController,
    NiFloatExtraDataController, NiGeomMorpherController, NiLightColorController,
    NiLightFloatController, NiLookAtController, NiMaterialColorController, NiMorphData,
    NiMultiTargetTransformController, NiPathController, NiSequenceStreamHelper, NiTimeController,
    NiUVController,
};
use extra_data::{
    BsAnimNote, BsAnimNotes, BsBehaviorGraphExtraData, BsBound, BsClothExtraData,
    BsCollisionQueryProxyExtraData, BsConnectPointChildren, BsConnectPointParents,
    BsDecalPlacementVectorExtraData, BsDistantObjectExtraData, BsDistantObjectLargeRefExtraData,
    BsEyeCenterExtraData, BsFurnitureMarker, BsInvMarker, BsPositionData, BsWArray, NiExtraData,
};
use interpolator::{
    BsTreadTransfInterpolator, NiBSplineBasisData, NiBSplineCompFloatInterpolator,
    NiBSplineCompPoint3Interpolator, NiBSplineCompTransformInterpolator, NiBSplineData,
    NiBSplineFloatInterpolator, NiBSplinePoint3Interpolator, NiBSplineTransformInterpolator,
    NiBlendBoolInterpolator, NiBlendFloatInterpolator, NiBlendPoint3Interpolator,
    NiBlendTransformInterpolator, NiBoolData, NiBoolInterpolator, NiColorData, NiColorInterpolator,
    NiFloatData, NiFloatInterpolator, NiLookAtInterpolator, NiPathInterpolator,
    NiPoint3Interpolator, NiPosData, NiTextKeyExtraData, NiTransformData, NiTransformInterpolator,
    NiUVData,
};
use multibound::{BsMultiBound, BsMultiBoundAABB, BsMultiBoundOBB, BsMultiBoundSphere};
use node::{BsDistantObjectInstancedNode, BsOrderedNode, BsValueNode, BsWeakReferenceNode, NiNode};
use properties::{
    NiAlphaProperty, NiFlagProperty, NiFogProperty, NiMaterialProperty, NiStencilProperty,
    NiStringPalette, NiTexturingProperty, NiVertexColorProperty, NiZBufferProperty,
};
use shader::{
    BSEffectShaderProperty, BSLightingShaderProperty, BSShaderNoLightingProperty,
    BSShaderPPLightingProperty, BSShaderPropertyBaseOnly, BSShaderTextureSet, BSSkyShaderProperty,
    BSWaterShaderProperty, SkyShaderProperty, TallGrassShaderProperty, TileShaderProperty,
    WaterShaderProperty,
};
use skin::{
    BsDismemberSkinInstance, BsSkinBoneData, BsSkinInstance, NiSkinData, NiSkinInstance,
    NiSkinPartition,
};
use std::any::Any;
use std::fmt::Debug;
use std::io;
use std::sync::Arc;
use texture::{NiPersistentSrcTextureRendererData, NiPixelData, NiSourceTexture};
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

    /// Length of the opaque trailing bytes this block CAPTURED rather than
    /// parsed — the Starfield `starfield_tail` pattern. `None` means the block
    /// has no such capture at all.
    ///
    /// #2625 — this exists because capturing a tail destroys the signal that
    /// would otherwise report it. The tail read consumes `block_size -
    /// consumed` *inside* the block parser, so by the time `parse_nif` compares
    /// consumed against `block_size` the two agree exactly and drift is zero.
    /// A Starfield shader under-read therefore never reached
    /// `drift_histogram`: measured, shader-block drift was `{}` while tail
    /// lengths were simultaneously bimodal `{38: 1868, 42: 11}` — precisely the
    /// anomaly a drift histogram exists to raise, and the reason SF-D6-01 went
    /// undetected by telemetry that was nominally watching for it.
    ///
    /// Reporting the length keeps the capture (it is deliberate — the bytes are
    /// preserved rather than skipped) while restoring the observability it
    /// removed. `Some(0)` is meaningful and distinct from `None`: the block has
    /// a tail slot and consumed the whole declared size without leftovers.
    fn opaque_tail_len(&self) -> Option<usize> {
        None
    }
}

/// Implements the trivial 2-method `NiObject` boilerplate for block types
/// that don't override `as_object_net`, `as_av_object`, or `as_shader_refs`.
///
/// Two forms:
/// - `impl_ni_object!(TypeA, TypeB)` — uses `stringify!(TypeName)` as the block type name.
/// - `impl_ni_object!(TypeA => "ActualName", TypeB => "Other")` — explicit name string.
///
/// Mix forms freely: `impl_ni_object!(Foo, Bar => "bhkBar", Baz);`
#[macro_export]
macro_rules! impl_ni_object {
    // Base case: empty
    () => {};
    // Explicit name form: Type => "name", rest...
    ($t:ident => $name:literal, $($rest:tt)*) => {
        impl $crate::blocks::NiObject for $t {
            fn block_type_name(&self) -> &'static str { $name }
            fn as_any(&self) -> &dyn ::std::any::Any { self }
        }
        $crate::impl_ni_object!($($rest)*);
    };
    // Explicit name form (trailing, no comma): Type => "name"
    ($t:ident => $name:literal) => {
        impl $crate::blocks::NiObject for $t {
            fn block_type_name(&self) -> &'static str { $name }
            fn as_any(&self) -> &dyn ::std::any::Any { self }
        }
    };
    // Implicit name form: Type, rest...
    ($t:ident, $($rest:tt)*) => {
        impl $crate::blocks::NiObject for $t {
            fn block_type_name(&self) -> &'static str { stringify!($t) }
            fn as_any(&self) -> &dyn ::std::any::Any { self }
        }
        $crate::impl_ni_object!($($rest)*);
    };
    // Implicit name form (trailing, no comma): Type
    ($t:ident) => {
        impl $crate::blocks::NiObject for $t {
            fn block_type_name(&self) -> &'static str { stringify!($t) }
            fn as_any(&self) -> &dyn ::std::any::Any { self }
        }
    };
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
///
/// This is the test-friendly entry point — pass `&str` and an `Arc<str>` is
/// allocated lazily only if the unknown-block-type fallback fires. The hot
/// path (`lib.rs::parse_nif`) calls [`parse_block_with_name_arc`] instead,
/// passing the already-built `Arc<str>` from the header table so the
/// fallback can refcount-clone it rather than allocating fresh. See #1261.
pub fn parse_block(
    type_name: &str,
    stream: &mut NifStream,
    block_size: Option<u32>,
) -> io::Result<Box<dyn NiObject>> {
    parse_block_inner(type_name, stream, block_size, None)
}

/// Hot-path entry point used by `lib.rs::parse_nif`. The caller already has
/// an `Arc<str>` for the block type name (built once per block from the
/// header table, then refcount-shared across every NiUnknown fallback);
/// this function avoids a fresh `Arc::from(type_name)` in the fallback by
/// cloning the caller's refcount instead. See #1261 / NIF-D5-NEW-01 — the
/// `_ =>` fallback was missed by #834's original sweep across `lib.rs`.
pub fn parse_block_with_name_arc(
    type_name_arc: &Arc<str>,
    stream: &mut NifStream,
    block_size: Option<u32>,
) -> io::Result<Box<dyn NiObject>> {
    parse_block_inner(
        type_name_arc.as_ref(),
        stream,
        block_size,
        Some(type_name_arc),
    )
}

/// Probe whether `type_name` has a dedicated parser arm in [`parse_block`]
/// (as opposed to falling through to the [`NiUnknown`] catch-all). This is
/// the LIVE source of truth for dispatch coverage: coverage tooling
/// (`examples/d5_coverage.rs`) calls this instead of carrying a hand-written
/// dispatch-key snapshot that silently over-reports as new arms land (#1335 /
/// NIF-2026-05-29-07).
///
/// Mechanism: only the `_ =>` fallback constructs a [`NiUnknown`], so a name
/// is dispatched iff parsing it against an empty payload does NOT yield
/// `Ok(NiUnknown)`. A dispatched arm either parses an empty struct
/// (`Ok(other)`) or — far more often — errors on the truncated input
/// (`Err`); both verdicts mean "has a parser". `block_size = Some(0)` makes
/// the fallback skip 0 bytes and return `NiUnknown` cleanly, so the
/// undispatched case is unambiguous. Dispatch routes only on the type name,
/// so the empty payload + minimal modern header never change the verdict.
pub fn is_block_type_dispatched(type_name: &str) -> bool {
    let header = crate::header::NifHeader::detached(crate::version::NifVersion::V20_2_0_7, 0, 0);
    let data: [u8; 0] = [];
    let mut stream = NifStream::new(&data, &header);
    match parse_block(type_name, &mut stream, Some(0)) {
        Ok(block) => block.block_type_name() != "NiUnknown",
        Err(_) => true,
    }
}

/// Is `type_name` a Havok *serializable* (the `bhkRefObject` subtree:
/// shapes, rigid bodies, constraints, phantoms, ragdoll, `hkPackedNiTriStripsData`)?
///
/// These skip the v10.0.x NiObject `groupID` (see the consume site in
/// [`parse_block_inner`]). The `NiCollisionObject` family
/// (`bhkCollisionObject` / `bhkNPCollisionObject` / `bhkPCollisionObject`
/// / `bhkSPCollisionObject` / `bhkBlendCollisionObject`) is deliberately
/// excluded — those descend from `NiObject` (nifly `bhk.hpp`:
/// `NiCollisionObject : NiObject`), not `bhkRefObject`, so they DO carry
/// the groupID. They are the only `bhk*` types ending in `CollisionObject`.
fn is_havok_serializable(type_name: &str) -> bool {
    (type_name.starts_with("bhk") || type_name.starts_with("hk"))
        && !type_name.ends_with("CollisionObject")
}

fn parse_block_inner(
    type_name: &str,
    stream: &mut NifStream,
    block_size: Option<u32>,
    type_name_arc_hint: Option<&Arc<str>>,
) -> io::Result<Box<dyn NiObject>> {
    // O5-3 / #688 — early-Gamebryo NIFs in the file-version range
    // [10.0.0.0, 10.1.0.114) ship a 4-byte `groupID` field on every
    // `NiObject` that nifly's `NiObject::Get` consumes before any
    // subclass payload. Pre-#688 this byte was misread as the first
    // u32 of the block — usually the `NiObjectNET.Name` SizedString
    // length — and 154 / 8032 Oblivion-era files (39 at v10.0.1.0 +
    // 12+9 at v10.0.1.2 + 8 at v10.1.0.101 + 77 at v10.1.0.106 + 9
    // others) truncated at root with "failed to fill whole buffer".
    // The version gate is the exact pair niflib uses
    // (`crates/reference/nifly/include/BasicTypes.hpp:972`).
    //
    // The field is named `groupID` after nifly's name; vanilla
    // Bethesda content always ships zero. We read it and discard —
    // the value isn't consumed by any downstream parser.
    //
    // Defence-in-depth note (NIF-D3-NEW-07, audit 2026-05-12): on a
    // v10.0.x dispatch-fallback where the block-sizes table is
    // unavailable, the unconditional consume here could theoretically
    // over-read by 4 bytes — but `block_sizes` is gated at
    // `>= 20.2.0.5` everywhere it's consumed, so the combination is
    // structurally unreachable on shipping content. Moving this
    // consumption into per-subclass `NiObject`-base parsers would
    // make it defensible against future format quirks, deferred until
    // content surfaces.
    //
    // #1329 / #1337 — the v10.0.x `groupID` is a NiObject field, but the
    // `bhkRefObject` Havok-serializable subtree (shapes / bodies /
    // constraints / phantoms / `hkPackedNiTriStripsData`) skips it: its
    // Get-chain never reaches `NiObject::Get` and openmw's `bhk*::read`
    // bodies read no groupID. The `NiCollisionObject` family
    // (`*CollisionObject`) is NOT in that subtree — it descends from
    // `NiObject` directly (nifly `bhk.hpp`: `NiCollisionObject : NiObject`)
    // — so it DOES carry the groupID. `is_havok_serializable` encodes that
    // split; the version band is `NifVersion::has_object_group_id`.
    // Consuming the groupID for the wrong set shifts the whole v10.0.1.0
    // collision chain by 4 bytes/block and truncates these sizeless files
    // (handscythe01 / oar01 / ungrdltraphingedoor) — verified byte-exact.
    if stream.version().has_object_group_id() && !is_havok_serializable(type_name) {
        let _group_id = stream.read_u32_le()?;
    }

    for family in dispatch::FAMILIES {
        if let Some(parsed) = family(type_name, stream, block_size)? {
            return Ok(parsed);
        }
    }
    // An RTTI name no family owns.
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
        // #1261 — refcount-clone the caller's existing `Arc<str>`
        // (built once per block by `parse_nif`) instead of
        // allocating fresh on every unknown-block fallback. Tests
        // calling `parse_block` directly pass `None` and fall back
        // to the original `Arc::from(type_name)` allocation; only
        // production hot-path callers via `parse_block_with_name_arc`
        // get the refcount-share win.
        let arc = match type_name_arc_hint {
            Some(a) => Arc::clone(a),
            None => Arc::from(type_name),
        };
        Ok(Box::new(NiUnknown {
            type_name: arc,
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

#[cfg(test)]
mod dispatch_tests;

#[cfg(test)]
mod group_id_tests {
    use super::is_havok_serializable;

    /// #1337 — the groupID split. The `bhkRefObject` subtree (shapes,
    /// bodies, constraints, data) skips the v10.0.x NiObject groupID; the
    /// `NiCollisionObject` family (`*CollisionObject`) and all Ni* blocks
    /// keep it. Regressing this (e.g. a blanket `starts_with("bhk")`)
    /// strips bhkCollisionObject's groupID and cascade-truncates
    /// sizeless v10.0.1.x meshes.
    #[test]
    fn havok_serializable_excludes_collision_object_family() {
        // bhkRefObject subtree → serializable (no groupID).
        for t in [
            "bhkRigidBody",
            "bhkRigidBodyT",
            "bhkBoxShape",
            "bhkConvexSweepShape",
            "bhkMeshShape",
            "bhkMoppBvTreeShape",
            "bhkNiTriStripsShape",
            "bhkListShape",
            "hkPackedNiTriStripsData",
        ] {
            assert!(is_havok_serializable(t), "{t} must be a Havok serializable");
        }
        // NiCollisionObject family → NOT serializable (keeps groupID).
        for t in [
            "bhkCollisionObject",
            "bhkNPCollisionObject",
            "bhkPCollisionObject",
            "bhkSPCollisionObject",
            "bhkBlendCollisionObject",
        ] {
            assert!(
                !is_havok_serializable(t),
                "{t} descends from NiObject and MUST keep the groupID"
            );
        }
        // Ordinary Ni* blocks → NOT serializable.
        for t in ["NiNode", "NiTriStrips", "NiSkinData", "NiKeyframeData"] {
            assert!(!is_havok_serializable(t), "{t} is not a Havok serializable");
        }
    }
}

#[cfg(test)]
mod dispatch_probe_tests {
    use super::is_block_type_dispatched;

    /// #1335 — the five types `d5_coverage`'s stale snapshot mis-flagged as
    /// UNCOVERED despite having live dispatch arms. The probe must report them
    /// covered so the coverage tool stops over-reporting. A regression here
    /// (an arm deleted) flips the tool's count, which is exactly what this
    /// guards.
    #[test]
    fn previously_misreported_types_are_dispatched() {
        for t in [
            "NiAlphaController",
            "BSKeyframeController",
            "BSBoneLODExtraData",
            "NiIntegersExtraData",
            "NiFloatsExtraData",
            // A few core types as a sanity floor.
            "NiNode",
            "NiTriShape",
            "bhkRigidBody",
        ] {
            assert!(
                is_block_type_dispatched(t),
                "{t} has a live parse_block arm but the probe reports it uncovered"
            );
        }
    }

    /// A name with no parser arm must fall through to the `NiUnknown`
    /// fallback and be reported uncovered — the negative half of the probe
    /// contract.
    #[test]
    fn unknown_type_is_not_dispatched() {
        assert!(!is_block_type_dispatched("DefinitelyNotARealNifBlockType"));
        assert!(!is_block_type_dispatched(""));
    }
}
