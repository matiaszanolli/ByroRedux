# D1-02: Terrain LOD ring leaks entirely on worldspace-state drain

**Issue**: #1536 · **Severity**: HIGH · **Labels**: high, memory, bug
**Source**: AUDIT_FNV_2026-06-14 (D1-02) · **Status when filed**: NEW, CONFIRMED

## Location
- `byroredux/src/streaming_helpers.rs:19-40` (`drain_streaming_state`)
- `byroredux/src/streaming.rs:177,183` (`lod_blocks` / `object_lod_blocks`)
- reclaim fn `byroredux/src/cell_loader/terrain_lod.rs:280-283`

## Description
`drain_streaming_state` drains only `state.loaded` (per-cell roots, via `unload_cell`). It never iterates `state.lod_blocks` or `state.object_lod_blocks`. LOD blocks deliberately carry no `CellRoot`, so `unload_cell`'s `CellRootIndex` victim walk can't reach them either. The only path that ever calls `unload_lod_block` is `stream_lod_blocks`'s own steady-state retain/regen loop. A worldspace-state drain (exterior→interior door-walk mid-session) frees zero LOD blocks.

## Evidence
`drain_streaming_state` body has no LOD iteration (`state.loaded.drain()` only). `unload_lod_block` callers exclusively `terrain_lod.rs:209/226/237`; `unload_object_lod_block` only `object_lod.rs:136`. Drain invoked on real mid-session transitions (`debug_load.rs`, `main.rs`). Terrain LOD not game-gated.

## Impact
Every exterior→interior transition while the LOD ring is resident leaks the whole ring (hundreds of blocks at `LOD_RADIUS_BLOCKS`): SSBO range (`MeshHandle`), base ground `TextureHandle` refcount (#1537), ECS row. Unbounded across a session.

## Suggested Fix
In `drain_streaming_state`, before joining the worker, iterate `state.lod_blocks.drain()` → `terrain_lod::unload_lod_block` and `state.object_lod_blocks.drain()` → `object_lod::unload_object_lod_block`.
