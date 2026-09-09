//! Per-node attribute extractors (#3856, split from `walk/mod.rs`).
//!
//! Small, independent readers the two scene-graph walkers call as they
//! descend: tree bones, LOD/range data, the BS value/ordered node payloads,
//! billboard mode, and the editor-marker predicate.

use crate::blocks::node::{
    BsOrderedNode, BsRangeNode, BsTreeNode, NiBillboardNode, NiLODNode, NiRangeLODData,
};
use crate::blocks::NiObject;
use crate::scene::NifScene;

use super::super::coord::zup_point_to_yup;
use super::super::{LodGroupData, TreeBones};
use super::resolve_block_ref_names;
use crate::blocks::node::BsRangeKind;

/// Extract the [`crate::import::TreeBones`] payload when `block` is a
/// [`BSTreeNode`]. Returns `None` for any other block type (including
/// the regular `NiNode` and its non-tree subclasses). See #363.
pub(crate) fn extract_tree_bones(scene: &NifScene, block: &dyn NiObject) -> Option<TreeBones> {
    let tree = block.as_any().downcast_ref::<BsTreeNode>()?;
    let branch_roots = resolve_block_ref_names(scene, &tree.bones_1);
    let trunk = resolve_block_ref_names(scene, &tree.bones_2);
    if branch_roots.is_empty() && trunk.is_empty() {
        // No surviving bones — treat as if the wire data was absent so
        // the consumer doesn't have to filter out empty-payload tree
        // nodes downstream.
        None
    } else {
        Some(TreeBones {
            branch_roots,
            trunk,
        })
    }
}

/// Extract the [`BsRangeKind`] discriminator when `block` is a
/// [`BsRangeNode`] (or one of its dispatcher-aliased subclasses
/// `BSDamageStage` / `BSBlastNode` / `BSDebrisNode`). Returns `None`
/// for any other block type. See #364.
pub(crate) fn extract_range_kind(block: &dyn NiObject) -> Option<BsRangeKind> {
    block.as_any().downcast_ref::<BsRangeNode>().map(|n| n.kind)
}

/// Extract the [`LodGroupData`] (LOD center + per-level near/far ranges) when
/// `block` is a [`NiLODNode`] whose `lod_level_data` resolves to a
/// [`NiRangeLODData`]. Center is converted NIF-Z-up → engine-Y-up. Returns
/// `None` for any other node type, a NULL/legacy ref, or empty ranges.
/// In-cell-LOD foundation: surfaced for a future distance-switch consumer;
/// the walker still imports only child 0 (highest detail).
pub(crate) fn extract_lod_group(scene: &NifScene, block: &dyn NiObject) -> Option<LodGroupData> {
    let lod = block.as_any().downcast_ref::<NiLODNode>()?;
    let data_idx = lod.lod_level_data.index()?;
    let data = scene.get_as::<NiRangeLODData>(data_idx)?;
    if data.lod_levels.is_empty() {
        return None;
    }
    Some(LodGroupData {
        center: zup_point_to_yup(&data.lod_center),
        levels: data.lod_levels.clone(),
    })
}

/// Extract a `BSValueNode`'s `(value, value_flags)` pair. Pre-#625
/// `as_ni_node` unwrapped the wrapper to plain `NiNode`, dropping
/// these fields. Returns `None` for any block that isn't a
/// `BsValueNode`. See #625 (SK-D4-02).
pub(crate) fn extract_bs_value_node(block: &dyn NiObject) -> Option<super::super::BsValueNodeData> {
    block
        .as_any()
        .downcast_ref::<crate::blocks::node::BsValueNode>()
        .map(|n| super::super::BsValueNodeData {
            value: n.value,
            flags: n.value_flags,
        })
}

/// Extract a `BSOrderedNode`'s draw-order metadata. Pre-#625
/// `as_ni_node` unwrapped the wrapper to plain `NiNode`, dropping
/// `alpha_sort_bound` + `is_static_bound`. Returns `None` for any
/// block that isn't a `BsOrderedNode`. See #625 (SK-D4-03).
///
/// #2008 — `alpha_sort_bound`'s `[x, y, z]` center is a point (unlike
/// `BsBound.dimensions`, an unsigned half-extent), so it gets the full
/// Z-up → Y-up swap-and-negate like every other node-local position on
/// `ImportedNode` and its siblings, not the magnitude-only axis reorder
/// `BsBound.dimensions` uses. `radius` is a magnitude and is unaffected
/// by the rotation.
pub(crate) fn extract_bs_ordered_node(
    block: &dyn NiObject,
) -> Option<super::super::BsOrderedNodeData> {
    block.as_any().downcast_ref::<BsOrderedNode>().map(|n| {
        let [x, y, z, radius] = n.alpha_sort_bound;
        let center = byroredux_core::math::coord::zup_to_yup_pos([x, y, z]);
        super::super::BsOrderedNodeData {
            alpha_sort_bound: [center[0], center[1], center[2], radius],
            is_static_bound: n.is_static_bound,
        }
    })
}

/// Extract a NiBillboardNode mode from a block, if any.
///
/// From 10.1.0.0 onward (all Bethesda games) the mode is a trailing u16
/// field on the block. Pre-10.1.0.0 the mode is packed into NiAVObject
/// flags bits 5-6 — we translate that back out so the consumer always
/// sees the modern `BillboardMode` value regardless of source version.
///
/// Returns `None` for non-billboard nodes.
pub(crate) fn extract_billboard_mode(block: &dyn NiObject, av_flags: u32) -> Option<u16> {
    if let Some(bb) = block.as_any().downcast_ref::<NiBillboardNode>() {
        if bb.billboard_mode != 0 {
            return Some(bb.billboard_mode);
        }
        // 10.1.0.0+ NIF with mode 0 is still a valid "always face camera"
        // billboard — preserve the fact that this is a billboard.
        // Fall through to the legacy flags check in case the parser
        // defaulted to 0 for a pre-10.1.0.0 NIF.
        let legacy = (av_flags >> 5) & 0x3;
        return Some(legacy as u16);
    }
    None
}

/// Check if a node name is an editor marker that should be skipped.
///
/// Matches the NiNode name prefixes Bethesda uses for editor-only
/// geometry across Oblivion / FO3 / FNV / Skyrim / FO4 / FO76 /
/// Starfield:
///
/// - `EditorMarker*` — catch-all Bethesda placeholder (every game).
/// - `marker_*` / `marker:*` / `MarkerX` — Gamebryo editor pins
///   (quest / patrol / navmesh markers).
/// - `MapMarker` — exterior-cell world map pin. Skyrim+ ships one
///   of these per settlement / POI; without the match they render
///   as untextured pyramids scattered across the overworld
///   (audit N26-4-06 / #165).
pub(crate) fn is_editor_marker(name: Option<&str>) -> bool {
    let Some(name) = name else { return false };
    fn starts_with_ci(s: &str, prefix: &str) -> bool {
        s.len() >= prefix.len()
            && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    }
    starts_with_ci(name, "editormarker")
        || starts_with_ci(name, "marker_")
        || name.eq_ignore_ascii_case("markerx")
        || starts_with_ci(name, "marker:")
        || starts_with_ci(name, "mapmarker")
}
