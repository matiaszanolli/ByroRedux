# FNV-D3-02: Terrain splat-layer texture refcounts never released — ~150 leaks per 7×7 reload

## Finding: FNV-D3-02

- **Severity**: HIGH
- **Source**: `docs/audits/AUDIT_FNV_2026-04-24.md`
- **Game Affected**: All games with terrain splat layers (FNV/FO3 LAND, Skyrim landscape, FO4 distant terrain)
- **Locations**:
  - Acquire: `build_cell_splat_layers` at [byroredux/src/cell_loader.rs:780](byroredux/src/cell_loader.rs#L780) → `resolve_texture` → `acquire_by_path` (refcount bumped per layer)
  - Allocate: [cell_loader.rs:1024-1032](byroredux/src/cell_loader.rs#L1024-L1032) — `ctx.allocate_terrain_tile(indices_arr)` with up to 8 texture indices
  - Release path: `unload_cell` → [cell_loader.rs:271-273](byroredux/src/cell_loader.rs#L271-L273) → `ctx.free_terrain_tile(slot)`
  - Bug site: `free_terrain_tile` at [crates/renderer/src/vulkan/context/resources.rs:51-60](crates/renderer/src/vulkan/context/resources.rs#L51-L60) — only resets the slot to `None`; does **not** drop the per-layer texture refcounts

## Description

Each terrain tile's 8 layer textures are acquired through `resolve_texture` → `acquire_by_path`, which bumps the registry refcount by one per layer. `allocate_terrain_tile` stores the indices but doesn't track them as ECS `TextureHandle` components, so `unload_cell`'s ECS-component sweep cannot release them.

`free_terrain_tile` (at resources.rs:51-60) is the natural release site, but it only manipulates the slot's free-list state. The per-layer texture refcounts persist forever — those textures stay pinned in the bindless slab and will eventually saturate `check_slot_available()`.

A 7×7 WastelandNV grid with avg 3 splat layers per LAND leaks **~150 texture refcounts per reload**. The leak is invisible today because cells don't unload during normal play; M40 doorwalking will surface it.

## Suggested Fix

Make `free_terrain_tile` return (or expose via a peek) the previous `[u32; 8]` so `unload_cell` can drop each non-zero layer:

```rust
// crates/renderer/src/vulkan/context/resources.rs
pub fn free_terrain_tile(&mut self, slot: u32) -> Option<[u32; 8]> {
    let idx = slot as usize;
    if idx >= self.terrain_tiles.len() { return None; }
    let tile = self.terrain_tiles[idx].take()?;
    self.terrain_tile_free_list.push(slot);
    self.terrain_tiles_dirty = true;
    Some(tile.layer_texture_index)
}
```

Then in `unload_cell`:

```rust
for &slot in &terrain_tile_slots {
    if let Some(layer_indices) = ctx.free_terrain_tile(slot) {
        for idx in layer_indices {
            if idx != 0 {
                ctx.texture_registry.drop_texture(&ctx.device, idx);
            }
        }
    }
}
```

Alternative: pass `&mut TextureRegistry` into `free_terrain_tile` and do the drops inline. The two-step API is cleaner because it lets the caller batch / dedup if needed.

## Related

- FNV-D3-01 (companion) — SkyParamsRes textures leak in the same way.
- M40 doorwalking — once that lands, this becomes a per-transition regression.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify any other slot-allocator on `VulkanContext` follows the same pattern (none currently, but check before extending).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Leak-counter regression — load 7×7 WastelandNV, unload, load again, assert `TextureRegistry.live_count` returned to baseline (within fallback-tex tolerance).

_Filed from audit `docs/audits/AUDIT_FNV_2026-04-24.md`._
