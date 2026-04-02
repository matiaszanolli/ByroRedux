# Investigation: Issue #2 — Scene Graph Hierarchy

## Architecture Decision
**Option (a): LocalTransform + GlobalTransform** (two components)

`Transform` already says "local-space" in its docstring but currently stores world-space.
Rather than rename, we keep `Transform` as the local-space component (it already is semantically)
and add `GlobalTransform` as the computed world-space equivalent.

- `Transform` = local-space (written by animation, import, user code)
- `GlobalTransform` = world-space (written only by `transform_propagation_system`)
- For root entities (no Parent), propagation copies Transform → GlobalTransform
- Renderer reads `GlobalTransform` for model matrix

## What Changes

### New components (crates/core)
1. `Parent(EntityId)` — sparse, points to parent entity
2. `Children(Vec<EntityId>)` — sparse, maintained alongside Parent
3. `GlobalTransform` — packed (same as Transform), computed each frame

### Import pipeline (crates/nif/src/import.rs)
Current: `walk_node()` accumulates world transforms, only emits leaf geometry.
New: Also emit `ImportedNode` for each NiNode with name + local transform.
New return type: `ImportedScene { nodes: Vec<ImportedNode>, meshes: Vec<ImportedMesh> }`
Each ImportedMesh/ImportedNode carries a `parent_index: Option<usize>` pointing into nodes array.
ImportedMesh transform becomes LOCAL (relative to parent), not world-space.

### Entity spawning (byroredux/src/main.rs)
`load_nif_bytes()`: spawn node entities first, then mesh entities.
Set Parent/Children for each. Set Transform to local. GlobalTransform set by propagation.

### Renderer (byroredux/src/main.rs)
`collect_draw_commands()` at line 1149: query GlobalTransform instead of Transform for model matrix.

### Animation system (byroredux/src/main.rs)
Already writes to Transform (local) — no change needed.

### Cell loader (byroredux/src/cell_loader.rs)
Cell-placed objects (REFR) don't have internal animation hierarchy — they're single meshes
placed at world positions. These get Transform + GlobalTransform but no Parent.
The cell loader composes ref_transform * nif_transform into a world-space Transform.
Since these have no Parent, propagation copies Transform → GlobalTransform. No change needed
in the composing logic — but we do need to add GlobalTransform to cell-spawned entities.

### Fly camera (main.rs:108)
Writes to Transform directly — camera has no parent, propagation handles it.

### Spin system (main.rs:770)
Writes to Transform — demo entities have no parent, propagation handles it.

## Files Touched
1. `crates/core/src/ecs/components/hierarchy.rs` — NEW: Parent, Children
2. `crates/core/src/ecs/components/global_transform.rs` — NEW: GlobalTransform
3. `crates/core/src/ecs/components/mod.rs` — register new modules
4. `crates/core/src/ecs/mod.rs` — export new components
5. `crates/nif/src/import.rs` — emit ImportedNode, local transforms, parent indices
6. `byroredux/src/main.rs` — spawn hierarchy, propagation system, renderer uses GlobalTransform
7. `byroredux/src/cell_loader.rs` — add GlobalTransform to spawned entities

**7 files total — above 5-file threshold. Must confirm with user.**
