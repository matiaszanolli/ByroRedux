# Investigation

Introduced by #470 (terrain splat). Pattern already established by
`gpu_instances_scratch` and `batches_scratch` in draw.rs:463/466 —
extended that discipline to the new terrain slab.

## Changes
- `mod.rs`: added `terrain_tile_scratch` field + Default init.
- `resources.rs`: lifted the fill into a pure `fill_terrain_tiles`
  free function (testable without Vulkan); the method delegates.
- `draw.rs`: `mem::take` scratch → call method → consume slice for
  upload → restore scratch. Added `GpuTerrainTile` import.

## Tests
3 unit tests on `fill_terrain_tiles`:
- capacity amortizes across dirty frames (core regression)
- zero counter short-circuits without underflow
- empty slots fill with zero default (fragment-shader guard contract)

byroredux-renderer: 68 → 71 tests.
