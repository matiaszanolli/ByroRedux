# REN-D5-2026-09-26-01: A cancelled streaming apply orphans its queued-but-unflushed textures — `flush_upload_batch` installs a texture into a slot whose last reference was already released

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4879

**Labels**: high,renderer,vulkan,memory,bug

- **Severity**: HIGH (unbounded, session-long GPU memory leak on a normal-operation path; provisional. Downgrade to MEDIUM only if telemetry shows cancels with a non-empty upload queue are rare.)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/texture_registry/upload.rs` (`flush_upload_batch` install block, `queue_or_hit_for_view`), `crates/renderer/src/texture_registry/release.rs` (`drop_released_texture`, `release_ref`, `release_refs_batch`, `decrement_ref`), `crates/renderer/src/texture_registry/mod.rs` (`pending_dds_uploads`, the `TextureEntry` invariant "`texture.is_some()` iff `ref_count > 0`"), caller chain `byroredux/src/cell_loader/exterior.rs` (`ExteriorCellApplyJob::cancel`) → `unload_cell` → `drop_textures`
- **Status**: NEW
- **Description / Evidence**:
  - `enqueue_*` reserves a slot (`texture: None`, `ref_count: 1`, a `path_map` entry) and pushes a `PendingDdsUpload`.
  - `drop_released_texture` on such an entry does `entry.texture.take()` → `None` and returns, so the queued upload survives. `pending_dds_uploads` is only ever pushed to (`upload.rs` push) and `mem::take`n (`flush_pending_uploads`). There is no `retain` and no purge anywhere.
  - At the next flush, the record loop and the install block in `flush_upload_batch` never read `ref_count`. The install sets `entry.texture = Some(texture)` on a slot with `ref_count == 0` and no `path_map` key.
  - Nothing frees it before `TextureRegistry::destroy`: `tick_deferred_destroy` walks only `pending_destroy`, and `decrement_ref` on that slot returns "already-released".
  - Reachability, read in code: `ExteriorCellApplyJob::cancel` calls `references.cancel(world)` and then `unload_cell` directly, with no texture flush.
    - `flush_pending_cell_textures_on_yield` flushes only at ≥ 64 uploads or ≥ `MAX_UPLOAD_BATCH_BYTES` (`YIELDED_TEXTURE_UPLOAD_BATCH_MIN`, `should_flush_pending_cell_textures`).
    - So any cell with fewer than 64 fresh textures holds its whole reservation set unflushed until completion.
    - Failure arms that release just-resolved unflushed handles (`terrain.rs` `release_splat_layer_textures`, the `terrain_lod_btr.rs` failed-upload release) reach the same state.
  - Sibling of the same broken invariant: `update_rgba`'s extent-change arm quietly revives a `ref_count == 0` entry.
- **Impact**: Each such event uploads the queued textures and orphans them for the process lifetime. Re-entering the cell re-reserves fresh slots and uploads them a second time. `live_slot_count()` counts the orphans as dead, so the leak is invisible in telemetry. memory-budget.md's claim that "GPU image memory itself IS correctly reclaimed" is false for this path. Per-event size is bounded by the unflushed queue (< 64 textures or < 128 MiB). Frequency is unmeasured. The orphans also accelerate slot exhaustion (#2030).
- **Related**: #1922 (dead handle after a failed flush — a different mechanism), #2030, #524.
- **Suggested Fix**:
  1. In `flush_upload_batch`, skip any upload whose `textures[handle].ref_count == 0` before parsing or staging.
  2. Retain-filter `pending_dds_uploads` for freed handles once per release batch.
  3. Make `update_rgba` refuse `ref_count == 0` rather than revive.
  4. Pin it device-free: `queue_or_hit` → `release_ref` → assert the queue no longer names the handle. Those functions are already exercised without a device by the existing registry tests.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
