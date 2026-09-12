# SKY-2026-09-11-D6-01: .bto object-LOD spawner leaks a GPU texture handle + descriptor slot per non-atlas sub-mesh texture on every unload

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4253

**Severity**: HIGH
**Dimension**: 6 — Specialty Blocks + Real-Data Rendering
**Location**: `byroredux/src/cell_loader/object_lod.rs:65-79,348-431,473-481,490-505`
**Status**: NEW

**Description**: `.bto` object-LOD sub-mesh textures are acquired (`resolve_texture`, refcounted) but never released. `ObjectLodBlock` has exactly one texture field (`texture_handle`, the shared worldspace atlas), documented as the only handle dropped on unload, while #3412 added per-sub-mesh texture resolution for the ~34% of vanilla bindings that name a distinct (non-atlas) texture — none of those handles are ever released, in both `unload_object_lod_block` and the `entities.is_empty()` early return. This is the same leak shape #1537 and #2758 already closed on the sibling terrain-LOD and early-return paths; #3412 reopened it on the object-LOD path.

**Evidence**: Confirmed in current code — `ObjectLodBlock` (`object_lod.rs:69-78`) declares only `texture_handle: u32` (comment: "Dropped once on unload"). The per-sub-mesh loop (`object_lod.rs:348-431`) resolves potentially many distinct textures into a local `resolved: FxHashMap<String, u32>` (line 353), but neither `unload_object_lod_block` (`object_lod.rs:491-505`, releases only `block.texture_handle`) nor the `entities.is_empty()` early return (`object_lod.rs:473-481`, releases only `atlas`) touches any handle from `resolved` beyond the atlas entry.

**Impact**: Accrues on every quad load across the level-4/8/16 object-LOD ring during exterior traversal; never reclaimed; pins VRAM + bindless descriptor slots against `TextureRegistry` LRU eviction. Unbounded over a session's exterior traversal.

**Related**: #1537, #2758 (closed the same leak shape on sibling paths); #3412 (introduced the per-sub-mesh resolution that reopened it here).

**Suggested Fix**: Track every distinct non-atlas texture handle resolved per quad (e.g. store the `resolved` map's values, deduped, alongside `texture_handle` in `ObjectLodBlock`) and release all of them in both `unload_object_lod_block` and the `entities.is_empty()` early return, mirroring the `.btr` distant-terrain LOD path's correct dual-release (per the report's Dimension 6 checklist item 4).

## Completeness Checks
- [ ] **SIBLING**: Confirm the `.btr` distant-terrain LOD path's texture release pattern (which the report notes already does this correctly) is followed exactly
- [ ] **TESTS**: A regression test loads and unloads an object-LOD quad with ≥2 distinct non-atlas sub-mesh textures and asserts the registry refcount returns to its pre-load value
