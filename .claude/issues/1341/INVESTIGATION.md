# Investigation — #1341 (D3-05): GreyscaleLutHandle leaked on cell unload

## Root cause (confirmed)
`spawn_placed_instances` resolves a BSEffectShaderProperty greyscale LUT via
`resolve_texture` (refcount bump) and attaches `GreyscaleLutHandle` gated on
`!= fallback()` (`byroredux/src/cell_loader/spawn.rs:908-910`) — exactly mirroring the
`DarkMapHandle` path two lines above. But the `unload_cell` victim walk queried
`MeshHandle`/`TextureHandle`/`NormalMapHandle`/`DarkMapHandle`/`ExtraTextureMaps`/
`TerrainTileSlot` and **not** `GreyscaleLutHandle`, so the LUT texture was never handed to
`drop_texture` → one leaked texture + bindless slot per distinct LUT per unloaded cell.

## Design decision: extract for a real regression test
A minimal fix is one `gq` arm in the walk. But the walk lives inside `unload_cell`, which
needs a `VulkanContext` (no headless ctor), so a test that adds the arm couldn't actually
*verify* it — and this is the SECOND victim-walk omission this audit found (the first was the
water normal map, #1338), so the spot is regression-prone.

The codebase already has the right pattern: `release_victim_item_instances` (#896) is a pure,
GPU-free fn extracted from `unload_cell` and unit-tested in `inventory_release_tests.rs`.
Following it, I extracted the per-victim handle collection into
`collect_victim_gpu_handles(world, victims, fallback_tex) -> (mesh_drops, texture_drops,
terrain_tile_slots)` (pure over the `World`), added the `GreyscaleLutHandle` sweep there, and
wrote tests that call that fn directly — so dropping the LUT arm again fails the test.

## Behaviour-preserving notes
- Lock semantics unchanged: the fn holds the same per-component read guards across the same
  single fan-out walk and drops them at fn return (before the caller's GPU mutations) —
  identical to the old explicit `drop((mq, …))`.
- The terrain-slot layer-index draining stays in `unload_cell` (it needs
  `ctx.free_terrain_tile`); its inline `push_tex_drop` was replaced with the same `idx != 0 &&
  idx != fallback_tex` predicate.
- `texture_drops` is now `mut` in `unload_cell` so the terrain drain can still append.

## Fix surface
- `byroredux/src/cell_loader/unload.rs` — extract `collect_victim_gpu_handles` (+ `gq` sweep),
  import `GreyscaleLutHandle`, call the fn from `unload_cell`.
- `byroredux/src/cell_loader.rs` — `#[cfg(test)]` re-export + `mod unload_greyscale_lut_tests`.
- `byroredux/src/cell_loader/unload_greyscale_lut_tests.rs` — 3 regression tests.
