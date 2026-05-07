# Issue #881 (OPEN): CELL-PERF-03: synchronous texture uploads with no per-frame budget cap stall the main thread during cell loads

URL: https://github.com/matiaszanolli/ByroRedux/issues/881

---

## Description

Every fresh DDS / RGBA texture upload during cell load issues a synchronous `with_one_time_commands` (`crates/renderer/src/vulkan/texture.rs:155-243` for RGBA, `:455-560` for DDS mip chains, `:719-812` for the inner). Each call's submit + `wait_for_fences(.., u64::MAX)` blocks the main thread to completion (`texture.rs:802`).

There is **no per-frame upload budget cap** anywhere in `TextureRegistry::load_dds`, `acquire_by_path`, or `Texture::from_dds_with_mip_chain`. A cell load that touches 100 fresh DDS files (typical for a worldspace edge with a fresh biome) accumulates ~100 sync `wait_for_fences` calls — at ~1 ms each that's ~100 ms of stall on top of parse + import work.

Combined with **#879 (CELL-PERF-01)** and **#880 (CELL-PERF-02)**, the cell-load critical path is essentially "sleep while the GPU drains the queue."

## Evidence

```rust
// texture.rs:769-808 — every fresh upload blocks to completion
device.queue_submit(q, &[submit_info], fence)
    .context("submit one-time commands")?;
device.wait_for_fences(&[fence], true, u64::MAX)
    .context("wait for one-time commands")?;
```

`TextureRegistry::acquire_by_path` (#524) correctly dedupes at the unique-texture level — repeat lookups hit the cache. But every fresh DDS still pays the full sync stall.

## Why it matters

The GPU upload work itself is fine; the issue is the synchronous main-thread fence-wait per texture. The descriptor write side already handles this correctly: `pending_writes` at `texture_registry.rs:107` is a deferred-flush queue that batches descriptor updates per-frame. Extending the same pattern to the actual image upload completes the picture.

## Proposed Fix

Introduce `TextureUploadBudget` resource:

```rust
pub struct TextureUploadBudget {
    pub bytes_per_frame: usize,           // e.g. 16 MB
    pub queue: VecDeque<PendingUpload>,   // (handle, dds_bytes)
}
```

`acquire_by_path` enqueues the DDS bytes (and reserves the bindless slot eagerly so callers can reference the handle). A new `tick_texture_uploads` system, called once per `draw_frame`, drains the queue up to the byte budget into the per-frame transfer command buffer that's already submitted with the rest of the per-frame work.

Result: **single `with_one_time_commands` per frame**, not per texture. Cell load completes "instantly" (placeholder) and textures stream in over subsequent frames without stalling.

Mirrors existing `texture_registry::pending_writes` deferred-flush pattern — extending the same architecture from descriptor writes to image uploads.

## Cost Estimate

Worldspace edge crossing (100 fresh DDS files): currently ~100 ms stall; with budget cap: 0 ms perceptible (textures stream in at 16 MB/frame ≈ 4 frames at 60 fps for 64 MB of texture data).

## Completeness Checks

- [ ] **UNSAFE**: New deferred-upload path uses the same ash submit machinery; preserve invariants around staging buffer lifetime (staging Vec<u8> must outlive the GPU command consuming it).
- [ ] **SIBLING**: Same pattern should apply to mesh uploads (couples with #879 CELL-PERF-01) — the per-frame transfer command buffer is the right place for both.
- [ ] **DROP**: The `PendingUpload` queue must drop staging buffers exactly once after the per-frame fence signals; verify no leak on shutdown drain
- [ ] **LOCK_ORDER**: New `TextureUploadBudget` Resource adds one RwLock; ensure it sits below `TextureRegistry` in TypeId order
- [ ] **FFI**: N/A
- [ ] **TESTS**: Regression test — load a worldspace cell with 100 fresh DDS textures, assert main-thread blocking time < 5 ms (vs current ~100 ms); steady-state texture lookup latency must not regress

## Profiling-Infrastructure Gap

Wall-clock issue. Needs `tracing` span on `acquire_by_path` and `tick_texture_uploads`. NOT dhat. File the "wire `tracing` for cell-load critical path" follow-up that gives this finding + #879 + #880 their regression guards.

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06b.md` (CELL-PERF-03)
- Pairs naturally with: #879 (CELL-PERF-01), #880 (CELL-PERF-02) — cell-load wall-clock trio
- Mirrors pattern from: `texture_registry::pending_writes` deferred descriptor flush
- Builds on: #524 (TextureRegistry refcounted dedup)
