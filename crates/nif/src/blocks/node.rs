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
        // Use raw bsver rather than `variant().has_effects_list()` so
        // non-Bethesda pre-FO4 Gamebryo files (Unknown variant, bsver=0)
        // still read the list correctly. Same pattern + rationale as
        // the `has_properties_list` fix in base.rs. See issue #160.
        let effects = if stream.bsver() < 130 {
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

// ── BSMultiBoundNode ────────────────────────────────────────────────

/// BSMultiBoundNode — NiNode subclass with a pre-computed multi-bound
/// culling volume used for fast rejection of large interior cells.
///
/// Wire layout (niflib nif.xml):
/// ```text
/// NiNode body
/// BlockRef multi_bound_ref      ; → BSMultiBound
/// uint culling_mode             ; only for BSVER >= 83 (Skyrim+)
/// ```
///
/// `culling_mode` values: 0 = normal, 1 = all bounds visible, 2 = all
/// bounds hidden, 3 = force culled. Only present on Skyrim+ — the FO3/FNV
/// variant stops at `multi_bound_ref`. See issue #148.
#[derive(Debug)]
pub struct BsMultiBoundNode {
    pub base: NiNode,
    /// Reference to the associated BSMultiBound block.
    pub multi_bound_ref: BlockRef,
    /// Culling mode (Skyrim+ only; FO3/FNV leaves this as 0).
    pub culling_mode: u32,
}

impl NiObject for BsMultiBoundNode {
    fn block_type_name(&self) -> &'static str {
        "BSMultiBoundNode"
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

impl BsMultiBoundNode {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiNode::parse(stream)?;
        let multi_bound_ref = stream.read_block_ref()?;
        // culling_mode is Skyrim+ only (BSVER >= 83). FO3/FNV (bsver=34)
        // stops after the multi_bound_ref.
        let culling_mode = if stream.bsver() >= 83 {
            stream.read_u32_le()?
        } else {
            0
        };
        Ok(Self {
            base,
            multi_bound_ref,
            culling_mode,
        })
    }
}

// ── BSTreeNode ───────────────────────────────────────────────────────

/// BSTreeNode — Skyrim SpeedTree root with two bone lists (branches /
/// trunk) that the engine's tree simulation uses to animate bending
/// under wind loads. The bones are references to existing `NiNode`
/// blocks, so the scene walker still descends through the regular
/// `NiNode.children` path — these extra ref lists are just metadata
/// for the future SpeedTree runtime.
///
/// Wire layout (niflib nif.xml):
/// ```text
/// NiNode body
/// uint num_bones_1
/// BlockRef[num_bones_1] bones_1
/// uint num_bones_2
/// BlockRef[num_bones_2] bones_2
/// ```
///
/// See issue #159.
#[derive(Debug)]
pub struct BsTreeNode {
    pub base: NiNode,
    /// First bone list — the SpeedTree tool labels this "branch roots".
    pub bones_1: Vec<BlockRef>,
    /// Second bone list — the SpeedTree tool labels this "trunk bones".
    pub bones_2: Vec<BlockRef>,
}

impl NiObject for BsTreeNode {
    fn block_type_name(&self) -> &'static str {
        "BSTreeNode"
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

impl BsTreeNode {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiNode::parse(stream)?;
        let bones_1 = stream.read_block_ref_list()?;
        let bones_2 = stream.read_block_ref_list()?;
        Ok(Self {
            base,
            bones_1,
            bones_2,
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

// ── NiCamera ───────────────────────────────────────────────────────────
//
// Animated cinematic NIFs ship an embedded NiCamera describing a view
// frustum + viewport + LOD bias. Used to drive cutscene cameras from
// an animated scene graph. See issue #153.
//
// Layout (from nif.xml):
//
//   NiAVObject base
//   camera_flags: u16 (since 10.1.0.0)
//   frustum_left/right/top/bottom: f32
//   frustum_near, frustum_far: f32
//   use_orthographic: bool (since 10.1.0.0)
//   viewport_left/right/top/bottom: f32
//   lod_adjust: f32
//   scene_ref: Ref (NiAVObject)
//   num_screen_polygons: u32 (always 0 on disk)
//   num_screen_textures: u32 (since 4.2.1.0, always 0 on disk)

/// NiCamera — embedded camera block with frustum, viewport, LOD bias.
#[derive(Debug)]
pub struct NiCamera {
    pub av: NiAVObjectData,
    pub camera_flags: u16,
    pub frustum_left: f32,
    pub frustum_right: f32,
    pub frustum_top: f32,
    pub frustum_bottom: f32,
    pub frustum_near: f32,
    pub frustum_far: f32,
    pub use_orthographic: bool,
    pub viewport_left: f32,
    pub viewport_right: f32,
    pub viewport_top: f32,
    pub viewport_bottom: f32,
    pub lod_adjust: f32,
    pub scene_ref: BlockRef,
    /// Legacy — always zero on disk.
    pub num_screen_polygons: u32,
    /// Legacy — always zero on disk.
    pub num_screen_textures: u32,
}

impl NiObject for NiCamera {
    fn block_type_name(&self) -> &'static str {
        "NiCamera"
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

impl HasObjectNET for NiCamera {
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

impl HasAVObject for NiCamera {
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

impl NiCamera {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let av = NiAVObjectData::parse(stream)?;

        // camera_flags added at 10.1.0.0. Oblivion (20.0.0.5) has it.
        let camera_flags = if stream.version() >= crate::version::NifVersion(0x0A010000) {
            stream.read_u16_le()?
        } else {
            0
        };

        let frustum_left = stream.read_f32_le()?;
        let frustum_right = stream.read_f32_le()?;
        let frustum_top = stream.read_f32_le()?;
        let frustum_bottom = stream.read_f32_le()?;
        let frustum_near = stream.read_f32_le()?;
        let frustum_far = stream.read_f32_le()?;

        // use_orthographic added at 10.1.0.0. Per nif.xml, `bool` is
        // 8-bit from 4.1.0.1 onward — all games we target (Oblivion+)
        // sit in that window, so read a single byte.
        let use_orthographic = if stream.version() >= crate::version::NifVersion(0x0A010000) {
            stream.read_byte_bool()?
        } else {
            false
        };

        let viewport_left = stream.read_f32_le()?;
        let viewport_right = stream.read_f32_le()?;
        let viewport_top = stream.read_f32_le()?;
        let viewport_bottom = stream.read_f32_le()?;
        let lod_adjust = stream.read_f32_le()?;
        let scene_ref = stream.read_block_ref()?;
        let num_screen_polygons = stream.read_u32_le()?;

        // num_screen_textures added at 4.2.1.0 — always present for our targets.
        let num_screen_textures = if stream.version() >= crate::version::NifVersion(0x04020100) {
            stream.read_u32_le()?
        } else {
            0
        };

        Ok(Self {
            av,
            camera_flags,
            frustum_left,
            frustum_right,
            frustum_top,
            frustum_bottom,
            frustum_near,
            frustum_far,
            use_orthographic,
            viewport_left,
            viewport_right,
            viewport_top,
            viewport_bottom,
            lod_adjust,
            scene_ref,
            num_screen_polygons,
            num_screen_textures,
        })
    }
}
