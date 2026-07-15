# DIM2-01: Additive-blend sort key orders mesh before wireframe bit, unlike the opaque branch

**Filed**: 2026-07-15 · **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-15.md` (Dimension 2: Draw & Instancing) · **Labels**: `low,renderer,performance,bug`

## Description

In the alpha-blend branch of `draw_sort_key`, the wireframe pipeline-bind bit (packed into `pack_depth_state`, bit 2) lands at sort-key slot 7. For additive blend (`dst_blend == GAMEBRYO_BLEND_ONE`), the mesh handle sorts at slot 6 — ahead of the wireframe bit. The opaque branch orders these the other way round (`pack_depth_state` at slot 6, `mesh_handle` at slot 7), which is the correct order since `wireframe` is a `PipelineKey` axis (`Opaque { wireframe }` / `Blended { .., wireframe }`, #869) and must sort ahead of any axis that isn't itself a pipeline-bind boundary.

A scene containing multiple distinct additive-blend meshes, each present in both wireframe and fill variants, would sort as `meshA-fill, meshA-wire, meshB-fill, meshB-wire` — forcing 4 pipeline binds instead of the optimal `fill,fill,wire,wire` (2 binds).

## Evidence

```rust
// additive-blend branch (mod.rs:225-234)
let (slot6, slot8) = if cmd.dst_blend == GAMEBRYO_BLEND_ONE {
    (cmd.mesh_handle, cmd.sort_depth) // mesh dominates → contiguous
} else {
    (!cmd.sort_depth, cmd.mesh_handle)
};
(..., slot6, pack_depth_state(cmd) as u32, slot8, cmd.entity_id)
//          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wireframe bit lands at slot 7, AFTER mesh (slot 6)

// opaque branch (mod.rs:247-256), correct ordering:
(..., pack_depth_state(cmd) as u32, cmd.mesh_handle, cmd.sort_depth, cmd.entity_id)
//    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wireframe bit at slot 6, BEFORE mesh (slot 7)
```

## Impact

`NiWireframeProperty` combined with additive-blend `NiAlphaProperty` is effectively absent from Bethesda content — real-world blast radius is ~zero. No correctness impact; additive blend is order-independent for the final composited color. Latent pipeline-bind inefficiency only.

## Related

- #1806 (D2-NEW-05) — the wireframe-into-`pack_depth_state` packing this finding stems from.
- #1377, #1804 — prior draw/instancing regression guards in the same sort-key/batch-merge machinery (both re-verified intact during this audit pass).

## Suggested Fix

Not worth the sort-key-width cost given the ~zero real-world blast radius. If ever measurable, give the transparent branch a dedicated wireframe slot ahead of the mesh slot, mirroring the opaque branch.

## Completeness Checks
- [ ] **SIBLING**: Confirm the true alpha-over (non-additive) transparent branch doesn't have the same axis-ordering issue
- [ ] **TESTS**: If fixed, a regression test pins the fill/wireframe cluster ordering for the additive-blend sort-key branch specifically

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/1994
