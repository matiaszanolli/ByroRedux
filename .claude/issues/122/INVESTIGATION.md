# Investigation: NIF-402 Cell loader flat import discards collision

## Problem
`cell_loader.rs:368` uses `import_nif()` (flat path) which calls `walk_node_flat()`.
`walk_node_flat()` does NOT call `extract_collision()` — only `walk_node_hierarchical()` does.
Result: all collision data from cell-loaded NIFs is silently dropped.

## Approach
The cleanest fix is to switch the cell loader from `import_nif()` to `import_nif_scene()` (hierarchical),
then iterate `imported.nodes` for collision data alongside `imported.meshes` for geometry.

The cell loader already composes REFR transforms with NIF-internal transforms manually.
With `import_nif_scene()`, the meshes have local transforms (not world-space), so the cell loader
would need to compose the full parent chain. However, since it already applies REFR transforms,
the simplest approach is: keep the existing mesh handling using the flat path for geometry,
and add a SEPARATE hierarchical pass just for collision extraction.

Actually, the even simpler approach: keep `import_nif()` for meshes (it works fine),
and also call `import_nif_scene()` to get collision data from nodes. This is slightly wasteful
(parses twice conceptually) but the NIF is already parsed — both functions just walk the
already-parsed NifScene. The overhead is negligible.

Simplest fix: after `import_nif()`, also call `import_nif_scene()` and iterate nodes
for collision data. Attach collision components to a dedicated entity per node-with-collision,
positioned at the REFR transform (collision is already in node-local space after the
hierarchical walk does Z→Y conversion).

Wait — the hierarchical walk stores collision per-node with LOCAL transforms. For cells,
we need WORLD transforms. The flat walk composes world transforms for meshes. We need
equivalent for collision.

**Best approach**: Add collision extraction directly to `walk_node_flat()`. Each NiNode already
has its world_transform composed. When a node has a collision_ref, extract it and return it
alongside the meshes. This requires extending the flat import to also return collision data.

## Plan
1. Add `ImportedCollision` struct (world-space position/rotation + shape + body data)
2. Extend `import_nif()` return type to include collision: `(Vec<ImportedMesh>, Vec<ImportedCollision>)`
3. Add collision extraction to `walk_node_flat()` 
4. Update cell_loader to attach collision components
5. Update all other callers of `import_nif()` (tests + examples just ignore the new field)

Actually, changing the return type of `import_nif()` would break 20+ test call sites.
Better: add a new function `import_nif_with_collision()` or add the collision vec as an
out parameter. Or even simpler: return a struct.

Simplest with minimum breakage: return a struct from import_nif().
