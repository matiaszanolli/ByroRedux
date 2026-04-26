# SK-D4-LOW: Specialty NiNode subclasses lose extra fields — BsValueNode value/flags + BsOrderedNode alpha_sort_bound

## Finding: SK-D4-LOW (bundle of SK-D4-02 + SK-D4-03)

- **Severity**: LOW (both items)
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`

## SK-D4-02: BsValueNode value + value_flags discarded by importer

**Location**:
- Parser populates them: [crates/nif/src/blocks/node.rs:175-205](crates/nif/src/blocks/node.rs#L175-L205) — `value: u32`, `value_flags: u8`
- Walker drops them: [crates/nif/src/import/walk.rs:45-46](crates/nif/src/import/walk.rs#L45-L46) — `as_ni_node` returns just `&n.base`, forgetting the trailing fields

`BsValueNode` carries numeric metadata (FO3/FNV used it for LOD-distance overrides + billboard-mode hints on subtree roots; persisted in Skyrim chains). Today the importer only walks the embedded `NiNode`, dropping the value+flags pair.

#150 closed when subtree dispatch landed (so children are walked correctly), but did not surface the extra fields.

**Fix**: surface as `ImportedNode::extras.bs_value_node = Some((value, flags))`; consume in scene setup. Cross-check nif.xml `BSValueNode` for any version-gated fields.

## SK-D4-03: BsOrderedNode alpha_sort_bound + is_static_bound discarded; depth-only sort

**Location**:
- Parser populates them: [crates/nif/src/blocks/node.rs:128-167](crates/nif/src/blocks/node.rs#L128-L167) — `alpha_sort_bound: [f32; 4]`, `is_static_bound: bool`
- Walker drops them: same `as_ni_node` path
- Render path: [byroredux/src/render.rs](byroredux/src/render.rs) `build_render_data` sorts purely by `Transform.translation.z`

`BsOrderedNode` exists specifically to declare a fixed draw order for its children (alpha-sorted UI / HUD overlays, certain Dragonborn banner meshes, FO3/FNV transparent stacks). Sort by `Transform.translation.z` ignores parent-supplied ordering; alpha bleed on banner stacks.

**Fix**: tag children of a BsOrderedNode with a `RenderOrderHint(u16)` component derived from sibling index in the scene graph; back-to-front sorter checks that component first and falls back to depth otherwise. Carry `alpha_sort_bound` as a separate optional component if the renderer uses it for occlusion.

## Related

- #150 (closed): BSOrderedNode + BSValueNode subtrees silently dropped — closed via dispatch fix; this finding is the EXTRA-fields gap that survives that fix.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit other NiNode subclasses for parsed-but-unread fields (e.g. `BsRangeNode.range_start/range_end`, `BsTreeNode.bone_lists` — #363 was closed for the latter).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic NIF with a BsOrderedNode children chain; assert renderer respects sibling-index order before depth.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
