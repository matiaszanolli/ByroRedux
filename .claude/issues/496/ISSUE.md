# Issue #496: drain_terrain_tile_uploads allocates 32 KB Vec per dirty frame

Severity: MEDIUM | Labels: bug, renderer, vulkan, memory, performance

## Problem
`drain_terrain_tile_uploads.collect()` freshly allocates a 32 KB Vec every
dirty frame. MAX_FRAMES_IN_FLIGHT per cell transition × 32 KB = ~128 KB
heap churn per cell load.

## Fix
Added persistent `terrain_tile_scratch: Vec<GpuTerrainTile>` on
VulkanContext. `drain_*` became `fill_terrain_tile_scratch_if_dirty(&mut
dest) -> bool` delegating to a free `fill_terrain_tiles` helper. Call
site uses the same `mem::take` dance as `gpu_instances_scratch`.
