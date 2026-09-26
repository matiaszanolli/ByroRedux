# REN-D5-2026-09-26-03: `StagingPool` capacity labels decay on reuse, so retained host-visible memory is not bounded by `DEFAULT_STAGING_BUDGET_BYTES`

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4881

**Labels**: medium,renderer,memory,bug

**Regression of #1921 (closed 2026-07-11, `a0b5539c4`).** #1921 fixed the ledger under-count by releasing at `allocation.size()`; #4512 (`80813026a`) had to abandon that because the allocation footprint can exceed the `VkBuffer` create size (an over-labelled entry can hand out a buffer smaller than a later request). The remedy (release at the *requested* size) reintroduced the under-count. No open issue tracks it. The correct label is the buffer's create size, which neither fix records.

- **Severity**: MEDIUM (impact ceiling HIGH if a soak confirms; live measurement owed)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/buffer.rs` (`StagingPool::acquire` / `release` / `total_capacity` / `trim_to`, `select_evictions`, `StagingGuard::release_to`, `GpuBuffer::create_device_local_buffer`, `create_device_local_buffers_batched`, `copy_bytes_range`), `crates/renderer/src/vulkan/texture.rs` (`record_dds_upload`, `overwrite_rgba_pixels`), `crates/renderer/src/texture_registry/upload.rs` (`flush_upload_batch`), `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`upload_terrain_tiles`)
- **Status**: **Regression of #1921** (closed 2026-07-11, `a0b5539c4`; text in `.claude/issues/1921/ISSUE.md`). Knowingly reintroduced by #4512 (`80813026a`, its own comment says "#1921's ledger concern returns") and extended to the mesh and terrain sites by #4593 (`59c4a01d4`) and #4790 (`78a98ed87`). No open issue.
- **Description / Evidence** (mechanism confirmed by reading `acquire` / `release` / `release_to`):
  - `acquire(size)` returns `(vk::Buffer, Allocation)` only, so the entry's capacity is lost. Every call site releases with the *current request's* `size`.
  - Best-fit hands an entry only to a request ≤ its label, and release re-labels it to that request. An entry's label is therefore monotonically non-increasing while its `VkBuffer` create size is fixed.
  - `total_capacity()` under-counts retained bytes, `release`'s over-budget `trim_to` never fires, and `select_evictions` evicts the largest *label* first, so honest entries go while decayed large buffers stay. There is no production `trim_to(0)` caller and no production `total_capacity()` reader.
  - Deterministic sequence from the code: a 64 MiB request creates B1 (create 64, label 64). A 3 MiB request reuses B1 and relabels it 3. A later 64 MiB request finds no label ≥ 64 and creates B2. A 4 MiB request takes B2 (label 4). Real bytes grow 64 MiB per cycle while labels stay a few MiB.
  - A Python model of exactly these rules (sub-scope B; not a measurement of the live engine) gave declared 125–128 MiB versus real 0.88–1.45 GiB (discrete BC1/BC3 sizes) and 3.19 GiB (log-uniform 4 KiB–22 MiB). The same model with the label set to the true create size gave real == declared ≈ 124 MiB.
  - Three pools carry the same rule (`TextureRegistry::staging_pool`, `MeshRegistry::geometry_staging_pool`, `SceneBuffers::terrain_tile_staging_pool`). It is more acute now because `geometry_staging_pool` holds the two 64 MiB `advance_geometry_rebuild` chunk buffers *and* is fed by the small per-group precombine uploads (`upload_scene_meshes_batched`) that relabel those entries small.
- **Impact**: CpuToGpu (BAR / VRAM under ReBAR) memory retained above the documented bound during texture-heavy or mesh-heavy streaming. Growth is per upload, not per frame. The magnitude depends on the real request-size mix and is not measured live.
- **Related**: #4512, #4593, #4790, #1921. memory-budget.md "Not yet ledgered: StagingPool retained capacity" (the "retention bound" it implies is false under this defect).
- **Suggested Fix**: Record the `VkBuffer` create size per entry inside `StagingPool` (return it from `acquire` and carry it in `StagingGuard`) and release at that. This satisfies both #4512 (label ≤ create size, never the allocation footprint) and #1921 (label == real). Add a `real_bytes()` gauge to the `rt.integrity` / scratch telemetry and a pure test over the extracted best-fit and relabel logic. The three tests that pin the request-size rule (`staging_release_capacity_tests::pooled_staging_releases_the_requested_size_not_the_allocation_footprint`, `::terrain_ring_releases_staging_at_the_requested_size`, `dds_upload_guard_tests::staging_release_capacity_is_requested_size_not_allocation_size`) must move with the fix. **Confirm first with a grid-soak** comparing the sum of real create sizes to `total_capacity()` per pool.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl and teardown ordering are still correct
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
