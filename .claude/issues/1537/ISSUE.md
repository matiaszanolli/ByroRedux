# D1-03: unload_lod_block never releases the base ground texture refcount

**Issue**: #1537 · **Severity**: MEDIUM · **Labels**: medium, memory, bug
**Source**: AUDIT_FNV_2026-06-14 (D1-03) · **Status when filed**: NEW, CONFIRMED

## Location
- `byroredux/src/cell_loader/terrain_lod.rs:280-283` (reclaim)
- `LodBlock` struct `byroredux/src/streaming.rs:65-69`
- acquire at `terrain_lod.rs:431-435,462`

## Description
`unload_lod_block` calls `drop_mesh(block.mesh_handle)` + `world.despawn(block.entity)`, but `LodBlock` stores only `{entity, mesh_handle, hole_mask}` — no texture handle. The base ground texture is acquired via `resolve_texture` (refcount bump) and attached as a `TextureHandle`, but `World::despawn` has no GPU side effects, so the refcount is never dropped.

## Evidence
`LodBlock` field list confirmed `{entity, mesh_handle, hole_mask}`. `unload_lod_block` body: only `drop_mesh` + `despawn`. Same class as #1338/#1341 handle-leak fixes, missed in LOD path. Unlike D1-02, fires during normal play (boundary-block regen, `terrain_lod.rs:209/227/237`).

## Impact
One ground-texture refcount leaked per boundary-block regen; few distinct landscape textures never reach refcount 0 → VkImages + bindless slots pinned for the session. Self-caps at distinct-texture count (hence MEDIUM).

## Suggested Fix
Add `texture_handle: u32` to `LodBlock`, populate at spawn, have `unload_lod_block` call `texture_registry.drop_texture` (skip `0`/fallback). `object_lod.rs:309-320` has the identical omission — fix in lockstep.
