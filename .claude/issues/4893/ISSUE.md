# REN-D5-2026-09-26-15: Doc and comment drift in Dim 5 owners (bundle)

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4893

**Labels**: low,renderer,documentation,doc-rot

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle (doc rot)
- **Status**: NEW, except where marked
- **Items** (doc claim → code fact):
  1. `docs/engine/memory-budget.md` "Deferred-Destroy Queue": describes `VecDeque<(frame_id, T)>` freed at `current_frame - frame_id >= countdown` and omits the BLAS-scratch queue and the retired instance-buffer pair. The shared primitive is `DeferredDestroyQueue<T>` = `Vec<(T, u32)>` with a per-tick countdown, destroyed on the (`DEFAULT_COUNTDOWN`+1)th tick (`default_countdown_survives_max_frames_in_flight_ticks`). Only the texture registry uses `VecDeque` plus frame ids. `crates/renderer/src/deferred_destroy.rs` says "Three production users today"; `SceneBuffers::retired_instance_buffers` (#4199) is a fourth.
  2. memory-budget.md "Texture Registry": "Bindless array ceiling `min(maxPerStageDescriptorUpdateAfterBindSampledImages, 65 535)`". `device.rs` computes that, but `init.rs` passes `max_bindless_sampled_images / 2` per binding, so the 2D and cube arrays are each 32,767 (one shared index space). Real slot capacity is half the documented ceiling, which halves the #2030 exhaustion horizon. The section also omits `MAX_UPLOAD_BATCH_BYTES` (128 MiB per submit, #4197) and its claim that "GPU image memory itself IS correctly reclaimed" is false for the finding-01 orphans.
  3. `docs/engine/exterior-grid-streaming.md`: "Queued texture uploads are flushed per yielded reference slice". The code flushes only at ≥ 64 uploads or ≥ 128 MiB (`should_flush_pending_cell_textures`). This is what makes finding 01 reachable.
  4. `docs/engine/archives.md` (DDS header reconstruction): "caps1 = TEXTURE | MIPMAP | COMPLEX" unconditional. `build_dds_header` sets MIPMAP|COMPLEX only when `num_mips > 1` (COMPLEX also for cubemaps). The stale `ba2.rs` comment "renderer's dds.rs is lenient (ignores arraySize)" is false (arraySize must be 1 or 6), and "valid for our DDS parser" omits the 8192 cap.
  5. `crates/renderer/src/texture_registry/upload.rs` `flush_pending_uploads` doc: "the staging buffers leak into the pool" → the code destroys them (finding 13).
  6. `docs/engine/shader-pipeline.md`: never says the instance and previous-model SSBO pair starts at `INITIAL_INSTANCE_CAPACITY` (65,536) and grows per slot, nor that a grow rewrites Set 1 bindings 4 and 18 in place (and the caustic set's binding 5). Its `MAX_TERRAIN_TILES … 32 B each` row (actual 160 B) is **Existing #4870 item (5)**, not re-filed.
  7. `crates/renderer/src/vulkan/acceleration/predicates.rs` (`screen_scaled_reservation_bytes` doc) links `FrameUpscaler::resident_bytes`, which no longer exists; the cached figure is `FrameUpscaler::sdk_memory_bytes`.
  8. `crates/renderer/src/vulkan/device.rs` `RT_EXTENSIONS` doc says "Optional RT extensions (enabled when available)"; they are mandatory since #3759.
  9. `crates/renderer/src/vulkan/gpu_timers.rs` test doc: "`QUERIES_PER_FRAME` (40) … 20 start/end brackets" vs `QUERIES_PER_FRAME = 56` (28 brackets). The module header is right.
  10. `crates/renderer/src/vulkan/context/mod.rs`: the `pending_skin_unload_victims` doc cites a bare `mod.rs` line number for a loop that now lives in `teardown.rs::destroy_allocator_owned_resources`, and the struct comment "later fields are destroyed first" is inverted (Rust drops in declaration order).
  11. Pre-existing, out of window: per-entity `SkinSlot::output_buffer` (DEVICE_LOCAL, `vertex_count × 12 B`, up to `SKIN_MAX_SLOTS`) and `WaterPipeline::param_buffers` have no memory-budget.md row.
  12. **Existing #4872** (still current on both counts: the 7 vs 0/14/28 MiB model-tier tail, and the dangling volumetrics noise-volume row) and **#4869 / #4871** are not re-filed.
- **Suggested Fix**: One doc-sweep commit. Each item is a text change with the code fact quoted above.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
