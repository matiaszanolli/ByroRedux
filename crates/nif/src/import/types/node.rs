//! Imported collision, scene-graph nodes and the node-extra-data payloads
//! (LOD groups, value/ordered nodes, tree bones).

use super::*;

/// Collision data extracted from a NiNode, positioned in world space.
///
/// Used by the flat import path to return collision alongside geometry,
/// since the flat path doesn't produce ImportedNode hierarchy.
#[derive(Debug)]
pub struct ImportedCollision {
    /// World-space translation (Y-up).
    pub translation: [f32; 3],
    /// World-space rotation as quaternion [x, y, z, w] (Y-up).
    pub rotation: [f32; 4],
    pub scale: f32,
    pub shape: CollisionShape,
    pub body: RigidBodyData,
}

/// A scene graph node (NiNode) extracted from a NIF file.
#[derive(Debug, Clone)]
pub struct ImportedNode {
    /// Node name from the NIF (e.g., "Bip01 Head", "Scene Root").
    /// Uses `Arc<str>` to share the string table entry without heap allocation.
    pub name: Option<Arc<str>>,
    /// Local-space translation (Y-up), relative to parent.
    pub translation: [f32; 3],
    /// Local-space rotation as quaternion [x, y, z, w] (Y-up).
    pub rotation: [f32; 4],
    pub scale: f32,
    /// Index into `ImportedScene.nodes` for this node's parent, or None for root.
    pub parent_node: Option<usize>,
    /// Collision shape and rigid body data (from bhkCollisionObject chain).
    pub collision: Option<(CollisionShape, RigidBodyData)>,
    /// Raw `BillboardMode` value if this node was a `NiBillboardNode`.
    /// `None` for regular NiNode and its non-billboard subclasses.
    /// The consumer maps this to the `Billboard` ECS component. See #225.
    pub billboard_mode: Option<u16>,
    /// SpeedTree bone metadata when this node is a `BSTreeNode` —
    /// `(branch_root_bone_names, trunk_bone_names)` resolved via
    /// each `BlockRef`'s `NiObjectNET.name`. Pre-#363 the parser
    /// kept the lists but the walker stripped them down to plain
    /// NiNode, blocking any future SpeedTree wind / bend simulation
    /// from finding the bones it would animate. The geometry renders
    /// correctly today via the regular `NiNode.children` path; this
    /// field exists purely so downstream consumers can branch on it.
    /// Mirrors the [`ImportedLight::affected_node_names`] resolution
    /// pattern from #335 — names are looked up by the scene builder
    /// against its `node_by_name` index. See audit S4-05.
    pub tree_bones: Option<TreeBones>,
    /// Wire-type discriminator when this node is a `BSRangeNode` /
    /// `BSDamageStage` / `BSBlastNode` / `BSDebrisNode`. Pre-#364 all
    /// four collapsed into a single `BsRangeNode` with no surviving
    /// discriminator; gameplay-side systems (destructible-object
    /// switching, blast-effect spawning, debris ejection) couldn't
    /// tell them apart. `None` when the source block was a plain
    /// `NiNode` or one of its non-range subclasses. See audit S4-06.
    pub range_kind: Option<BsRangeKind>,
    /// Raw `NiAVObject.flags` value. Carried through so the scene
    /// builder can spawn a `SceneFlags` component per entity. APP_CULLED
    /// (bit 0) is already consumed by the import-time visibility filter
    /// in `walk.rs`; this field preserves every other bit
    /// (SELECTIVE_UPDATE, DISABLE_SORTING, IS_NODE, DISPLAY_OBJECT,
    /// etc.) so gameplay-side systems can branch on them instead of
    /// re-reading the source NIF. See #222.
    pub flags: u32,
    /// `BSValueNode` numeric metadata `(value, value_flags)` when this
    /// node is a `BSValueNode`. FO3/FNV used the value field for
    /// LOD-distance overrides + billboard-mode hints on subtree roots;
    /// persisted into Skyrim chains. Pre-#625 the walker called
    /// `as_ni_node` which dropped the trailing fields — this surfaces
    /// them so a future LOD/billboard consumer can read them off the
    /// imported node instead of re-walking the NIF. `None` on plain
    /// `NiNode` and other subclasses. See #625 (SK-D4-02).
    pub bs_value_node: Option<BsValueNodeData>,
    /// `BSOrderedNode` draw-order metadata when this node is a
    /// `BSOrderedNode`. Children of an ordered node SHOULD render in
    /// sibling-index order (alpha-sorted UI / HUD overlays, Dragonborn
    /// banner stacks, FO3/FNV transparent stacks); the depth-only sort
    /// in `byroredux/src/render.rs::build_render_data` ignores this,
    /// producing alpha bleed on banner stacks. Pre-#625 the walker
    /// dropped `alpha_sort_bound` + `is_static_bound` along with the
    /// type identity. Renderer consumption (a `RenderOrderHint`
    /// component on each child + a sort-key tweak) is deferred — the
    /// data plumbing lands here so the eventual fix has the source
    /// material to work from. `None` on plain `NiNode` and other
    /// subclasses. See #625 (SK-D4-03).
    pub bs_ordered_node: Option<BsOrderedNodeData>,
    /// LOD distance ranges when this node is a `NiLODNode` — its
    /// `NiRangeLODData` (`lod_center` + per-level `(near, far)` extents),
    /// converted to engine Y-up. The children (the LOD levels) are parallel
    /// to `lod_levels` (level `i` ↔ child `i`). Pre-foundation the walker
    /// imported child 0 only and dropped the ranges; this surfaces them so a
    /// future distance-switch system has the data without re-walking the NIF.
    /// **Import behaviour is unchanged** — child 0 (highest detail) is still
    /// the only child imported; nothing consumes `lod_group` at runtime yet.
    /// `None` on plain `NiNode` / `NiSwitchNode` and other subclasses.
    pub lod_group: Option<LodGroupData>,
}

/// LOD distance ranges from a [`NiLODNode`]'s `NiRangeLODData` — surfaced on
/// the matching [`ImportedNode`]. Parsed + carried for a future in-cell
/// distance-switch consumer (NIFAL §2 in-cell-LOD foundation); the renderer
/// does not yet read it.
#[derive(Debug, Clone)]
pub struct LodGroupData {
    /// LOD center in engine Y-up, node-local space (combine with the node's
    /// world transform at the consumer). Camera distance is measured from here.
    pub center: [f32; 3],
    /// `(near, far)` extent per LOD level, parallel to the host node's
    /// children (level `i` ↔ child `i`). The active level is the one whose
    /// `[near, far)` contains the camera-to-center distance.
    pub levels: Vec<(f32, f32)>,
}

/// `BSValueNode` numeric payload — surfaced on the matching
/// [`ImportedNode`] entry. Pre-#625 the walker dropped these fields
/// when it unwrapped the wrapper to plain `NiNode`. Future consumers:
/// LOD-distance override (FO3/FNV), billboard-mode hint on subtree
/// roots, gameplay-side cell-marker metadata.
#[derive(Debug, Clone, Copy)]
pub struct BsValueNodeData {
    /// Raw `BSValueNode.value` u32. Semantics depend on the parent
    /// subtree's gameplay role; the importer captures the wire value
    /// verbatim and lets consumers interpret.
    pub value: u32,
    /// `BSValueNode.value_flags` byte. Pre-#625 dropped along with
    /// `value`.
    pub flags: u8,
}

/// `BSOrderedNode` draw-order metadata — surfaced on the matching
/// [`ImportedNode`] entry. Pre-#625 the walker dropped both fields
/// when it unwrapped the wrapper to plain `NiNode`. Future renderer
/// consumption: tag children with `RenderOrderHint(sibling_index)` so
/// `build_render_data`'s sort prefers parent-supplied order over
/// `Transform.translation.z`. Carry `alpha_sort_bound` separately if
/// the renderer grows occlusion / culling on the bound.
#[derive(Debug, Clone, Copy)]
pub struct BsOrderedNodeData {
    /// Alpha-sort bounding sphere `[x, y, z, radius]` in node-local
    /// space, lifted from the BSOrderedNode wire format. The center is
    /// Y-up (Z-up → Y-up converted at extraction, #2008) like every
    /// other position field on `ImportedNode` and its siblings; `radius`
    /// is a magnitude and is unaffected by the conversion.
    pub alpha_sort_bound: [f32; 4],
    /// `true` when the bound is fixed (doesn't update with animation).
    /// Lets the renderer skip per-frame bound recomputation for static
    /// containers like a stack of inn-room banners.
    pub is_static_bound: bool,
}

/// SpeedTree bone metadata surfaced from a [`BSTreeNode`] — bone
/// references resolved to scene-graph node names. The SpeedTree tool
/// labels the lists as "branch roots" and "trunk bones"; the future
/// SpeedTree wind / bend simulation animates the associated entities
/// under wind loads. The scene builder resolves names → entities via
/// its `node_by_name` index (same pattern as
/// [`ImportedLight::affected_node_names`] from #335).
/// See audit S4-05 / #363.
#[derive(Debug, Clone, Default)]
pub struct TreeBones {
    /// Branch-root bone names — the entities the wind sim swings the
    /// outer canopy from. Null refs and refs that don't resolve to a
    /// named NiObjectNET-bearing block are dropped silently.
    pub branch_roots: Vec<Arc<str>>,
    /// Trunk bone names — the entities the wind sim bends the trunk
    /// across. Same drop rules as `branch_roots`.
    pub trunk: Vec<Arc<str>>,
}

/// Re-export the `BsRangeKind` discriminator from the parser side so
/// callers downstream of `ImportedNode` don't have to reach into
/// `crate::blocks::node` directly.
pub use crate::blocks::node::BsRangeKind;
