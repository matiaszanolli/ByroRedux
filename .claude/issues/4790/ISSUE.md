# #4790: PERF-D4-2026-09-23b-01: The #4593 fix labels the terrain ring's previous staging buffer with the *new* upload's size; when the terrain prefix grows, the pool hands that too-small buffer straight back and the copy overruns it (regression of #4593)

**Severity**: CRITICAL
**Labels**: critical, renderer, vulkan, memory, safety, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D4-2026-09-23b-01)

- **Severity**: CRITICAL (host-side memory corruption; Vulkan spec violation)
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:1026-1072` (`upload_terrain_tiles`; the release is at `:1039`); the guard that pins the bug is `crates/renderer/src/vulkan/buffer.rs:2402`.
- **Status**: Regression of #4593. This is a new defect on the same line, introduced by its fix `59c4a01d4`, not the old footprint mislabel returning.
- **Description**:
  - `byte_size` is computed from the **current** call's `tiles.len()` (`:1026`).
  - `previous` is this frame slot's guard from an **earlier** upload, acquired at that upload's size.
  - `previous.release_to(&mut self.terrain_tile_staging_pool, byte_size)` records the old buffer in the pool with `capacity = byte_size`.
  - `StagingPool::acquire` (`buffer.rs:218-223`) returns the first free entry with `capacity >= size`. `release` inserts at `partition_point(capacity < size)`, which is ahead of any equal-capacity entry. So the next line (`:1041`, `acquire(byte_size)`) deterministically takes back the mislabelled buffer.
  - `copy_nonoverlapping(…, byte_size)` (`:1061-1065`) then writes past it:
    - if `byte_size` > the allocation footprint, the write goes into neighbouring sub-allocations of the shared CpuToGpu block, via a pointer derived from a slice of the allocation's length (UB in Rust as well);
    - the `vk::BufferCopy { size: byte_size }` exceeds `srcBuffer`'s create size in either case (the VUID-vkCmdCopyBuffer-srcOffset-00113 family #4593 set out to close);
    - debug builds panic first, on `release_to`'s `debug_assert!(capacity <= alloc.size())` (`buffer.rs:626-631`).
  - Before `59c4a01d4`, the release used `allocation.size()`: the old buffer's real footprint. That was never smaller than the buffer, only over by the rounding slack.
  - Every other `release_to` call site releases in the same scope that acquired, at the size it acquired (`buffer.rs:909,1605,1761`, `texture.rs:248,321`, `texture_registry/upload.rs:613`). Terrain is the only deferred release.
- **Evidence**: Uploads by frame slot with a growing tile prefix (`fill_terrain_tiles` uploads the live high-water prefix; dirty is set at `context/resources.rs:168,185`):
  - upload 1 → slot 0, N tiles;
  - upload 2 → slot 1, N+k tiles;
  - upload 3 → slot 0, N+2k tiles. It releases the N-tile buffer labelled `(N+2k)·160 B`, re-acquires it, and writes `(N+2k)·160 B` into an `N·160 B` (+ alignment) buffer.
- **Impact**:
  - Trigger: any multi-frame exterior stream that allocates terrain slots past the prefix (`--grid … --radius ≥ 1`, the W1 Lake Mead cell-boundary walk).
  - Release builds: silent corruption of other in-flight staging data (texture and mesh uploads sharing the block), a possible fault at the end of a mapped block, and a copy-range spec violation.
  - Debug builds: a panic on the first qualifying stream step.
- **Related**: #4593, #4512 (the overrun class), #3664 (the high-water prefix), SAFE-D2-2026-09-21-01.
- **Suggested Fix**:
  - Store the requested size next to the guard (`terrain_tile_staging_buffers: Vec<Option<(StagingGuard, vk::DeviceSize)>>`) and release the previous guard at *its own* recorded size.
  - Change the `buffer.rs:2402` pin so it forbids releasing at the current call's `byte_size`.
  - Add a unit test that drives two growing uploads into one slot against the pool's best-fit.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **DROP**: If the staging-guard slot type changes, teardown still releases every guard exactly once (reverse-order correct)
- [ ] **TESTS**: A regression test pins this specific fix
