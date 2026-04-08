//! NiNode — scene graph parent node.
//!
//! NiNode is the fundamental grouping object: it has a transform,
//! a list of children (NiAVObject refs), and a list of properties.

use super::base::NiAVObjectData;
use super::traits::{HasAVObject, HasObjectNET};
use super::NiObject;
use crate::stream::NifStream;
use crate::types::{BlockRef, NiTransform};
use std::any::Any;
use std::io;

/// Scene graph node (NiNode, BSFadeNode, etc.).
#[derive(Debug)]
pub struct NiNode {
    /// NiObjectNET + NiAVObject base fields.
    pub av: NiAVObjectData,
    /// NiNode-specific: child node/geometry references.
    pub children: Vec<BlockRef>,
    /// NiNode-specific: dynamic effect references (removed in FO4+).
    pub effects: Vec<BlockRef>,
    // Public accessors for backward compatibility with existing code
    // that accesses fields directly. These will be removed once all
    // consumers migrate to trait-based access.
}

// Convenience accessors for direct field access (backward compat).
impl NiNode {
    pub fn name(&self) -> Option<&str> {
        self.av.net.name.as_deref()
    }
    pub fn flags(&self) -> u32 {
        self.av.flags
    }
    pub fn transform(&self) -> &NiTransform {
        &self.av.transform
    }
    pub fn collision_ref(&self) -> BlockRef {
        self.av.collision_ref
    }
    pub fn properties(&self) -> &[BlockRef] {
        &self.av.properties
    }
    pub fn extra_data_refs(&self) -> &[BlockRef] {
        &self.av.net.extra_data_refs
    }
    pub fn controller_ref(&self) -> BlockRef {
        self.av.net.controller_ref
    }
}

impl NiObject for NiNode {
    fn block_type_name(&self) -> &'static str {
        "NiNode"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
        Some(self)
    }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> {
        Some(self)
    }
}

impl HasObjectNET for NiNode {
    fn name(&self) -> Option<&str> {
        self.av.net.name.as_deref()
    }
    fn extra_data_refs(&self) -> &[BlockRef] {
        &self.av.net.extra_data_refs
    }
    fn controller_ref(&self) -> BlockRef {
        self.av.net.controller_ref
    }
}

impl HasAVObject for NiNode {
    fn flags(&self) -> u32 {
        self.av.flags
    }
    fn transform(&self) -> &NiTransform {
        &self.av.transform
    }
    fn properties(&self) -> &[BlockRef] {
        &self.av.properties
    }
    fn collision_ref(&self) -> BlockRef {
        self.av.collision_ref
    }
}

impl NiNode {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let av = NiAVObjectData::parse(stream)?;

        // NiNode-specific fields
        let children = stream.read_block_ref_list()?;
        // FO4+ removes the effects list from NiNode (BSVER >= 130).
        let effects = if stream.variant().has_effects_list() {
            stream.read_block_ref_list()?
        } else {
            Vec::new()
        };

        Ok(Self {
            av,
            children,
            effects,
        })
    }
}

// ── BSOrderedNode ──────────────────────────────────────────────────

/// BSOrderedNode — NiNode subclass with alpha sort bound for draw ordering.
///
/// Used by FO3/FNV for transparent object sorting within a node hierarchy.
#[derive(Debug)]
pub struct BsOrderedNode {
    pub base: NiNode,
    /// Alpha sort bounding sphere: [x, y, z, radius].
    pub alpha_sort_bound: [f32; 4],
    /// Whether the bound is static (doesn't update with animation).
    pub is_static_bound: bool,
}

impl NiObject for BsOrderedNode {
    fn block_type_name(&self) -> &'static str {
        "BSOrderedNode"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
        Some(&self.base)
    }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> {
        Some(&self.base)
    }
}

impl BsOrderedNode {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiNode::parse(stream)?;
        let alpha_sort_bound = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let is_static_bound = stream.read_u8()? != 0;
        Ok(Self {
            base,
            alpha_sort_bound,
            is_static_bound,
        })
    }
}

// ── BSValueNode ────────────────────────────────────────────────────

/// BSValueNode — NiNode subclass with an integer value and flags.
///
/// Used by FO3/FNV for attaching numeric metadata to scene graph nodes.
#[derive(Debug)]
pub struct BsValueNode {
    pub base: NiNode,
    pub value: u32,
    pub value_flags: u8,
}

impl NiObject for BsValueNode {
    fn block_type_name(&self) -> &'static str {
        "BSValueNode"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
        Some(&self.base)
    }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> {
        Some(&self.base)
    }
}

impl BsValueNode {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiNode::parse(stream)?;
        let value = stream.read_u32_le()?;
        let value_flags = stream.read_u8()?;
        Ok(Self {
            base,
            value,
            value_flags,
        })
    }
}

// ── NiBillboardNode ────────────────────────────────────────────────────
//
// Pre-10.1.0.0 the billboard mode was packed into the parent NiAVObject
// flags (bits 5-6). From 10.1.0.0 onward — including Oblivion v20.0.0.5
// — it becomes a trailing u16.

/// NiBillboardNode — children face the camera at rendering time.
#[derive(Debug)]
pub struct NiBillboardNode {
    pub base: NiNode,
    /// 0 = ALWAYS_FACE_CAMERA, 1 = ROTATE_ABOUT_UP,
    /// 2 = RIGID_FACE_CAMERA, 3 = ALWAYS_FACE_CENTER, 4 = RIGID_FACE_CENTER.
    pub billboard_mode: u16,
}

impl NiObject for NiBillboardNode {
    fn block_type_name(&self) -> &'static str {
        "NiBillboardNode"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
        Some(&self.base)
    }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> {
        Some(&self.base)
    }
}

impl NiBillboardNode {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiNode::parse(stream)?;
        // Mode field was introduced in 10.1.0.0. Earlier NIFs pack the
        // mode into NiAVObject flags (bits 5-6) and have no trailing
        // field — see nif.xml.
        let billboard_mode = if stream.version() >= crate::version::NifVersion(0x0A010000) {
            stream.read_u16_le()?
        } else {
            0
        };
        Ok(Self {
            base,
            billboard_mode,
        })
    }
}

// ── NiSwitchNode ───────────────────────────────────────────────────────
//
// Groups multiple scenegraph subtrees and exposes a single "active child"
// index. The flags field was added in 10.1.0.0 — at Oblivion-era versions
// we always read it.

/// NiSwitchNode — scenegraph node with a single active child.
#[derive(Debug)]
pub struct NiSwitchNode {
    pub base: NiNode,
    /// Bit 0: update only active child. Bit 1: update controllers.
    pub switch_flags: u16,
    /// Active child index into `base.children`.
    pub index: u32,
}

impl NiObject for NiSwitchNode {
    fn block_type_name(&self) -> &'static str {
        "NiSwitchNode"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
        Some(&self.base)
    }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> {
        Some(&self.base)
    }
}

impl NiSwitchNode {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiNode::parse(stream)?;
        let switch_flags = if stream.version() >= crate::version::NifVersion(0x0A010000) {
            stream.read_u16_le()?
        } else {
            0
        };
        let index = stream.read_u32_le()?;
        Ok(Self {
            base,
            switch_flags,
            index,
        })
    }
}

// ── NiLODNode ──────────────────────────────────────────────────────────
//
// Distance-based LOD selector. Inherits NiSwitchNode; from 10.1.0.0 onward
// it stores a ref to NiLODData. Before that it held an inline (LOD center +
// num levels + level array) block — the legacy path is not exercised by
// any Bethesda game we target (Oblivion is already 20.0.0.5).

/// NiLODNode — distance-based level-of-detail selector.
#[derive(Debug)]
pub struct NiLODNode {
    pub base: NiSwitchNode,
    /// Ref to NiLODData (since 10.1.0.0). `NULL` for the legacy path.
    pub lod_level_data: BlockRef,
}

impl NiObject for NiLODNode {
    fn block_type_name(&self) -> &'static str {
        "NiLODNode"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
        Some(&self.base.base)
    }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> {
        Some(&self.base.base)
    }
}

impl NiLODNode {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiSwitchNode::parse(stream)?;
        let lod_level_data = if stream.version() >= crate::version::NifVersion(0x0A010000) {
            stream.read_block_ref()?
        } else {
            // Legacy path (Vector3 center + num levels + N × LODRange)
            // is not exercised by any currently-targeted game. Leave
            // unread; callers that hit this should switch to the
            // legacy branch if it ever becomes needed.
            BlockRef::NULL
        };
        Ok(Self {
            base,
            lod_level_data,
        })
    }
}

// ── NiSortAdjustNode ───────────────────────────────────────────────────
//
// Overrides the transparency sorter for a subtree. Oblivion v20.0.0.5 is
// > 20.0.0.3, so the trailing `accumulator` ref is absent.

/// NiSortAdjustNode — alpha sort override for a scenegraph subtree.
#[derive(Debug)]
pub struct NiSortAdjustNode {
    pub base: NiNode,
    /// SortingMode enum (u32). Typical values: 0 = inherit, 1 = off, 2 = sub-sort.
    pub sorting_mode: u32,
}

impl NiObject for NiSortAdjustNode {
    fn block_type_name(&self) -> &'static str {
        "NiSortAdjustNode"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
        Some(&self.base)
    }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> {
        Some(&self.base)
    }
}

impl NiSortAdjustNode {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiNode::parse(stream)?;
        let sorting_mode = stream.read_u32_le()?;
        // Legacy accumulator ref (until 20.0.0.3) — Oblivion and later
        // don't serialize it.
        if stream.version() <= crate::version::NifVersion(0x14000003) {
            let _accumulator = stream.read_block_ref()?;
        }
        Ok(Self { base, sorting_mode })
    }
}

// ── BSRangeNode ────────────────────────────────────────────────────────
//
// Bethesda node with (min, max, current) byte range values. FO3 and later.
// Its subclasses BSBlastNode, BSDamageStage, BSDebrisNode add no extra
// fields and share the exact same layout.

/// BSRangeNode — Bethesda-specific node carrying min/max/current bytes.
#[derive(Debug)]
pub struct BsRangeNode {
    pub base: NiNode,
    pub min: u8,
    pub max: u8,
    pub current: u8,
}

impl NiObject for BsRangeNode {
    fn block_type_name(&self) -> &'static str {
        "BSRangeNode"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
        Some(&self.base)
    }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> {
        Some(&self.base)
    }
}

impl BsRangeNode {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiNode::parse(stream)?;
        let min = stream.read_u8()?;
        let max = stream.read_u8()?;
        let current = stream.read_u8()?;
        Ok(Self {
            base,
            min,
            max,
            current,
        })
    }
}
