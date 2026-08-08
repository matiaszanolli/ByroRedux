# NIF-D4-2026-08-07-01: Hierarchical import never propagates NiBillboardNode mode onto child mesh entities -- billboard content renders in rest pose

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2527
**Finding ID**: NIF-D4-2026-08-07-01

**Severity**: MEDIUM
**Dimension**: Geometry Handoff
**Game Affected**: All games — `NiBillboardNode` is a cross-game NIF block with no `bsver` gating (Oblivion through Starfield)
**Location**: `crates/nif/src/import/walk/mod.rs:426-530` (the four geometry-push arms of `walk_node_hierarchical`); `crates/nif/src/import/mesh/{ni_tri_shape.rs:207,bs_tri_shape.rs:212,bs_geometry.rs:327}` (`billboard_mode: None` hardcoded); `byroredux/src/scene/nif_loader.rs:466-471`; `crates/core/src/ecs/systems.rs:41-253`; `byroredux/src/systems/billboard.rs:60-71`
**Status**: NEW

## Description
The **flat** import path (`walk_node_flat`, used by the cell loader) correctly threads billboard mode via `FlatWalkCtx.inherited_billboard` — every geometry-push arm sets `mesh.billboard_mode = ctx.inherited_billboard` after extraction (the `#2206` fix, still correct). The **hierarchical** path (`walk_node_hierarchical`, used by `import_nif_scene` — which is not just the loose-NIF debug viewer, it's also called from real object/terrain LOD spawn paths: `cell_loader/object_lod.rs:262`, `cell_loader/placement_lod.rs:469`, `cell_loader/terrain_lod_btr.rs:137`) has no equivalent mechanism: `HierWalkCtx` has no `inherited_billboard` field, the mesh extractors hardcode `billboard_mode: None`, and none of the four hierarchical push-arms overwrite it afterward. Even if it were populated, the consumer (`scene/nif_loader.rs`) only reads `ImportedNode.billboard_mode` to attach a `Billboard` component to the `NiBillboardNode`'s own (typically empty container) entity — a separate ECS entity from the actual geometry, linked via `Parent`/`Children`. `make_billboard_system` writes `GlobalTransform.rotation` directly on the billboard entity, bypassing `Transform`; `make_transform_propagation_system` reseeds its walk from the `Transform`-dirty set, which the billboard entity's write never touches — so the rotation never propagates to the child mesh's `GlobalTransform`.

## Evidence
Confirmed directly: `FlatWalkCtx` (`walk/mod.rs:896`) has `inherited_billboard: Option<u16>` and threads it through all four flat push-arms (lines 1080/1112/1141/1165). `HierWalkCtx` (`walk/mod.rs:197-207`) has no such field. `billboard_mode: None` is hardcoded at all three mesh extractors (`bs_tri_shape.rs:212`, `bs_geometry.rs:327`, `ni_tri_shape.rs:207`).

## Impact
Any NIF loaded through the hierarchical import path that authors a `NiBillboardNode` wrapping child geometry (sprite/impostor content, distant tree/grass billboards, camera-facing FX cards) spawns but never rotates to face the camera — it renders frozen in its authored rest orientation. Affects the standalone loose-NIF viewer and the real object/terrain LOD spawn paths. The primary in-game rendering path (cell-loader's `walk_node_flat`) is unaffected — it already has the `#2206` fix. Visual-only; no data corruption, no crash.

## Related
Sibling of but distinct from `#994` (cell-loader flat-path's `placement_root_billboard` gap for SpeedTree/.spt content, already known/deferred) — this is the hierarchical path's per-mesh gap, untracked, different root cause.

## Suggested Fix
Add `inherited_billboard: Option<u16>` to `HierWalkCtx` mirroring `FlatWalkCtx`, populate it in the `as_ni_node` branch (save/restore around recursion, matching the flat walker's pattern), and set `mesh.billboard_mode` in all four hierarchical push-arms before pushing. On the consumer side, attach `Billboard` directly to the mesh entity when `mesh.billboard_mode.is_some()` (mirroring `cell_loader/spawn.rs:1473-1475`) rather than relying on parent→child propagation, which the current system ordering can't do in one frame.

## Completeness Checks
- [ ] **TESTS**: A regression test loads a hierarchical-path NIF with a `NiBillboardNode` and confirms the child mesh's `Billboard` component / rotation-facing behavior
- [ ] **SIBLING**: Confirm `#994`'s separate SpeedTree gap isn't conflated with this fix
