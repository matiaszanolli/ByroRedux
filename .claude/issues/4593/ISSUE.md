# SAFE-D2-2026-09-21-01: Three staging-pool release sites still record `allocation.size()` as capacity — the #4512 `vkCmdCopyBuffer` overrun class, unfixed on the mesh/terrain paths

**Labels**: high, safety, renderer, vulkan, memory, bug

Filed via /audit-publish from docs/audits/AUDIT_SAFETY_2026-09-21.md.

**Severity**: HIGH (Vulkan spec violation; #4512 was filed HIGH) · **Dimension**: 2 — Memory corruption / UB / 5 — Vulkan spec
**Location**:
- `crates/renderer/src/vulkan/buffer.rs`: `GpuBuffer::create_device_local_buffer`, pooled-release arm (~:1599-1605)
- `crates/renderer/src/vulkan/buffer.rs`: `GpuBuffer::copy_bytes_range` (~:1758-1764)
- `crates/renderer/src/vulkan/scene_buffer/upload.rs`: `upload_terrain_tiles`, previous-slot release (~:1033-1040)

**Status**: NEW. These are unfixed siblings of closed #4512: its fix was incomplete, and this is not a regression.
**Verified against**: HEAD `f97775ca8`

## Description

- `80813026a` (the fix for #4512) traced the live `+8 B` `vkCmdCopyBufferToImage` overrun to pooled staging being released at `allocation.size()`. That value is the driver-rounded footprint, which sits above the `VkBuffer` create size. `StagingPool::acquire`'s best fit (`e.capacity >= size`) trusts the recorded capacity as the buffer's usable size, so it can hand a later request a `VkBuffer` smaller than the request.
- That commit fixed `record_dds_upload` and `create_device_local_buffers_batched`. It also rewrote `StagingGuard::release_to`'s contract: "`capacity` must be the buffer's *requested* size … never the allocation footprint."
- Three callers still pass the footprint:
  - `create_device_local_buffer` and `copy_bytes_range` both feed `MeshRegistry::geometry_staging_pool` (`crates/renderer/src/mesh.rs`). The first serves the global vertex/index SSBO build (two calls in `crates/renderer/src/mesh/geometry_ssbo.rs`); the second serves the #3298 chunked rebuild (two calls in the same file).
  - The per-cell batched upload (`create_device_local_buffers_batched`, called from `mesh.rs`) draws from the same pool, so it can be handed an inflated entry left by either of the other two.
  - The terrain-tile ring releases the previous frame slot's guard at `allocation.size()`.
- `Vertex` is 104 B, which is 8 mod 16. A `GEOMETRY_REBUILD_CHUNK_BYTES` (64 MiB) vertex chunk is 645,277 vertices = 67,108,808 B, also 8 mod 16. So odd-sized vertex uploads leave pool entries whose recorded capacity is 8 B above their `VkBuffer`.

## Evidence

```rust
// buffer.rs: create_device_local_buffer (pooled arm) and copy_bytes_range
let capacity = staging.allocation.as_ref().map(|a| a.size()).unwrap_or(size);
staging.release_to(pool, capacity);
// scene_buffer/upload.rs: upload_terrain_tiles
let capacity = previous.allocation.as_ref().map(|allocation| allocation.size()).unwrap_or(byte_size);
previous.release_to(&mut self.terrain_tile_staging_pool, capacity);
```

- The guard `pooled_staging_releases_the_requested_size_not_the_allocation_footprint` (`buffer.rs`, `staging_release_capacity_tests`) cannot see any of the three. It forbids only the exact spelling `.map(|allocation| allocation.size())` and scans only `buffer.rs`. The two `buffer.rs` sites spell it `.map(|a| a.size())`.
- `release_to`'s `debug_assert!(capacity <= alloc.size())` is always satisfied by the footprint itself.
- #4512 was closed with its SIBLING completeness box unchecked.
- Publish-time check: the other two `release_to` callers, `vulkan/texture.rs` and `texture_registry/upload.rs`, already release at the requested `image_size` via `record_dds_upload`.

## Impact

- Suppose a later batched or chunked request falls inside the slack, between the `VkBuffer` size and the footprint. It is handed the smaller buffer, and its `vkCmdCopyBuffer` region then exceeds `srcBuffer`'s size (VUID-vkCmdCopyBuffer-srcOffset-00113 / size family). That is the #4512 defect on the mesh path.
- The CPU write stays in bounds, because the mapped slice is allocation-sized. The GPU reads slack bytes of the same allocation. The result is a validation error and spec-level undefined behaviour, not a crash.
- The collision window is narrow, so this is rarer than the DDS case, where sizes cluster. The terrain ring uses 160 B `GpuTerrainTile`s, always a multiple of 16, so it is exposed only on drivers that round buffer requirements above 16 B.

## Related

- #4512 (closed): the texture-path fix this completes.
- #4187 (closed): the same `StagingPool` on the terrain ring.
- #1921 / #1954 (closed): an earlier change in the other direction, which made texture-flush releases record the allocation size so the 128 MB budget ledger would not under-count. #4512 then settled on the requested size for correctness.
- SAFE-D3-2026-09-21-01 (#4599): the same `StagingGuard` / `StagingPool` free paths.
- `docs/audits/AUDIT_PERFORMANCE_2026-09-21.md` and `docs/audits/AUDIT_CONCURRENCY_2026-09-21.md` cite this finding without re-reporting it.

## Suggested Fix

- At all three sites, release at the size each guard was acquired with. The terrain site needs care: its fallback `unwrap_or(byte_size)` is the *current* frame's size, not the size `previous` was acquired with, so a bare substitution would be wrong there.
- Better: have `StagingGuard` record its acquired size, and remove the caller-chosen `capacity` argument from `release_to`, so no caller can pick the footprint.
- Widen the pin to every `release_to` caller in the crate (both files, any closure spelling), or make it unnecessary through the API change above.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D2-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every `release_to` / `StagingPool::release` caller in `crates/renderer` re-checked (the three sites here plus `vulkan/texture.rs` and `texture_registry/upload.rs`)
- [ ] **DROP**: if the `StagingGuard` API changes, `release_to` / `Drop` still free or return each buffer exactly once
- [ ] **TESTS**: a pin or unit test fails if any release records more than the acquired size
