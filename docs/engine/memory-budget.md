# Memory Budget

Where VRAM and RAM go, what the ceilings are, and how each subsystem
handles overflow. The dev GPU is an RTX 4070 Ti (12 GB); the RT-minimum
target is 6 GB. Constants are verified against source; byte math is shown.

---

## Scene Buffers (per-frame SSBOs / UBOs)

Resident for the lifetime of `VulkanContext`. Double-buffered
(`MAX_FRAMES_IN_FLIGHT` = 2) — two live copies, two in-flight frames.
Constants in [`scene_buffer/constants.rs`](../../crates/renderer/src/vulkan/scene_buffer/constants.rs).

| Buffer | Constant | Entries | Entry size | Per-frame | × 2 FIF |
|---|---|---|---|---|---|
| Light SSBO | `MAX_LIGHTS` = 512 | 512 | 64 B | 32 KB | **64 KB** |
| Instance SSBO | `MAX_INSTANCES` = 262 144 | 262 144 | 112 B | 29.4 MB | **58.8 MB** |
| Indirect draw SSBO | `MAX_INDIRECT_DRAWS` = 262 144 | 262 144 | 20 B | 5.2 MB | **10.5 MB** |
| Material SSBO | `MAX_MATERIALS` = 16 384 | 16 384 | 300 B | 4.9 MB | **9.8 MB** |
| Terrain tile SSBO | `MAX_TERRAIN_TILES` = 1 024 | 1 024 | 32 B | 32 KB | **64 KB** |
| Bone-palette SSBO | `MAX_TOTAL_BONES` = 196 608 | 196 608 | 64 B | 12.6 MB | **25.2 MB** ¹ |
| Camera UBO | — | 1 | 336 B | 336 B | **672 B** |

¹ The bone-palette SSBO has a third copy for the previous frame (motion
vectors); total bone buffer is 3 × 12.6 MB ≈ **37.8 MB**.

**Total resident scene buffers:** ≈ **140 MB** across all copies.

Exceeding `MAX_INSTANCES` causes the TLAS to be partitioned across
multiple instance buffers (pending work); currently a `debug_assert`
fires. Exceeding `MAX_MATERIALS` silently reuses material slot 0.

---

## ReSTIR Reservoirs

[`restir.rs`](../../crates/renderer/src/vulkan/restir.rs) — screen-sized,
double-buffered (`MAX_FRAMES_IN_FLIGHT` = 2) STORAGE buffers for ReSTIR-DI
temporal reservoir reuse (Session 49 denoiser overhaul). Unlike every other
entry on this page, size scales with **swapchain resolution**, not a fixed
constant — recreated on every resize.

Formula: `width × height × RESERVOIR_STRIDE` bytes per FIF slot
(`RESERVOIR_STRIDE` = 32 B, one [`Reservoir`] per pixel).

| Resolution | Per-slot | × 2 FIF |
|---|---|---|
| 1920×1080 | 66.4 MB | **132.7 MB** |
| 2560×1440 | 118.0 MB | **235.9 MB** |
| 3840×2160 | 265.4 MB | **530.8 MB** |

This was the largest single VRAM addition of the denoiser overhaul (PERF-D5-NEW-04
/ #1814) — at 4K it is over 13% of the ~4 GB engine budget target below — but
had no ledger entry here and no attributing telemetry until #1814 added a
`log::info!` at both `ReservoirBuffers::new` and `recreate_on_resize` reporting
the computed size.

No leak: create-once + recreate-on-resize with a fenced destroy
(`recreate_swapchain` waits both frames-in-flight before dropping the old
buffers). Stale reservoir contents across a resize are harmless — the
final visibility ray re-validates every shaded sample.

---

## Acceleration Structures (BLAS / TLAS)

[`acceleration/constants.rs`](../../crates/renderer/src/vulkan/acceleration/constants.rs)

### Scratch buffers

| Constant | Value | Role |
|---|---|---|
| `BLAS_REBUILD_SLACK_BYTES` | 16 MB | Retained headroom above peak before BLAS-scratch shrink |
| `TLAS_SCRATCH_SLACK_BYTES` | 256 KB | Retained headroom above peak before TLAS-scratch shrink |
| `TLAS_REBUILD_SLACK_BYTES` | 1 MB | Retained headroom above peak before TLAS instance-buffer shrink |

`shrink_blas_scratch_to_fit` and `shrink_tlas_to_fit` run at cell-unload time
to reclaim VRAM after a peak scene is evicted.

### Reserve floors

| Constant | Value | Role |
|---|---|---|
| `MIN_TLAS_INSTANCE_RESERVE` | 8 192 instances | Never shrink the TLAS instance buffer below this |
| `WORKING_SET_FLOOR` | 8 192 instances | Post-shrink TLAS capacity floor |
| `MIN_BLAS_BUDGET_BYTES` | 256 MB | Minimum BLAS-budget floor (device_local_heap / 3, capped below) |

### Build flags (split post #1196)

| Constant | Value | Applies to |
|---|---|---|
| `UPDATABLE_AS_FLAGS` | `PREFER_FAST_TRACE \| ALLOW_UPDATE` | TLAS (refit on static-layout frames) |
| `SKINNED_BLAS_FLAGS` | `PREFER_FAST_BUILD \| ALLOW_UPDATE` | Skinned BLAS (refits >> builds at steady state) |
| `STATIC_BLAS_FLAGS` | `PREFER_FAST_TRACE \| ALLOW_COMPACTION` | Static mesh BLAS (compact after build) |

`SKINNED_BLAS_FLAGS` deliberately uses `FAST_BUILD` not `FAST_TRACE`:
empirically on RTX 4070 Ti, small skinned-mesh BVHs (~5K–15K triangles)
produced worse total GPU cost with `FAST_TRACE` (wider tree adds refit
overhead that exceeds the traversal saving). Switching back recovered
+15.8 FPS on Prospector (R6a-prospector-regress, 2026-05-16).

### LRU eviction

`AccelerationManager::evict_unused_blas` runs pre-batch and mid-batch
(triggered at 90% of BLAS budget). Eviction check interval:
`BATCH_EVICTION_CHECK_INTERVAL` = 64 BLAS builds. LRU victim = the BLAS
with the smallest last-used frame tick.

BLAS refit count before a forced rebuild: `SKINNED_BLAS_REFIT_THRESHOLD`
= 600 frames (~10 seconds at 60 FPS). After 600 refits the BLAS is
rebuilt from scratch to prevent BVH quality decay.

---

## Texture Registry

[`crates/renderer/src/texture_registry.rs`](../../crates/renderer/src/texture_registry.rs)

| Item | Value |
|---|---|
| Bindless array ceiling | `min(device.maxPerStageDescriptorUpdateAfterBindSampledImages, 65 535)` |
| Descriptor pool | `max_textures × MAX_FRAMES_IN_FLIGHT` combined image sampler descriptors |
| Staging pool cap | 128 MB (retained after upload flush, #239) |
| Deferred-destroy countdown | `MAX_FRAMES_IN_FLIGHT` = 2 frames |

There is no explicit texture-count eviction policy. When the bindless
array fills (rare on vanilla content; a concern for large mod load-orders)
new uploads are rejected with an error. A future eviction pass is tracked
as tech debt.

---

## Mesh Registry

[`crates/renderer/src/mesh.rs`](../../crates/renderer/src/mesh.rs)

| Constant | Value | VRAM |
|---|---|---|
| `MAX_MESH_SLOTS` | 16 777 216 (1 << 24) | handle-table slots only (not VRAM) |
| `VERTEX_POOL_SOFT_CAP` | 4 M vertices | ~400 MB (100 B/vertex) |
| `VERTEX_POOL_HARD_CAP` | 16 M vertices | ~1.6 GB |
| `INDEX_POOL_SOFT_CAP` | 16 M indices | ~64 MB (4 B/index) |
| `INDEX_POOL_HARD_CAP` | 64 M indices | ~256 MB |

The vertex stride is 100 B (19 × f32 + 4 × u32 + 8 × u8 — position,
colour, normal, UV, bone indices/weights, splat channels, tangent).
Soft caps emit a `warn!`; hard caps return an error.
`check_pool_growth()` is called at every upload.

**Registry overflow guard** (`667d1a28`): `NifImportRegistry` now defaults
to a 2 048-entry LRU cap (configurable via `BYRO_NIF_CACHE_MAX=N`; `=0`
disables the LRU). Before this guard, unbounded cell loads could silently
exhaust the `MAX_MESH_SLOTS` table.

---

## NIF Import Cache

[`byroredux/src/cell_loader/nif_import_registry.rs`](../../byroredux/src/cell_loader/nif_import_registry.rs)

Caches parsed + imported `NifScene` objects to avoid re-parsing the same
NIF when multiple REFRs reference it.

| Item | Value |
|---|---|
| Default cap | 2 048 entries |
| Override | `BYRO_NIF_CACHE_MAX=N` env var (`=0` disables LRU entirely) |
| Eviction strategy | LRU by last-access tick; smallest tick = victim on overflow |

The cap bounds *scene count*, not VRAM. Each cached entry holds
`ImportedScene` in CPU RAM (vertex data, block tree); the GPU resources
reside in `MeshRegistry` and are keyed separately.

---

## Material / BGSM Cache

[`byroredux/src/asset_provider/material.rs`](../../byroredux/src/asset_provider/material.rs)

| Constant | Value | Eviction |
|---|---|---|
| `MAX_BGEM_CACHE_ENTRIES` | 1 024 | Half-evict (remove oldest 512) on overflow |
| `MAX_FAILED_PATHS` | 1 024 | Half-evict (remove oldest 512) on overflow |
| `TemplateCache` cap | 256 entries | BGSM chain templates; LRU |

**Half-eviction** (`797424e4`, #1430): both maps use a companion
`VecDeque<String>` as an insertion-order tracker. When the map reaches
its ceiling, the oldest `N/2` keys are drained from the deque and
removed from the map. This keeps the recent working-set resident and
eliminates the cold-restart thundering-herd that a full flush caused.

---

## Deferred-Destroy Queue

[`crates/renderer/src/deferred_destroy.rs`](../../crates/renderer/src/deferred_destroy.rs)

GPU resources (textures, buffers, BLAS handles) cannot be freed
immediately after an ECS component drops them — the GPU may still be
reading them from an in-flight frame.

| Item | Value |
|---|---|
| Countdown depth | `DEFAULT_COUNTDOWN` = `MAX_FRAMES_IN_FLIGHT` = 2 frames |
| Implementation | `VecDeque<(frame_id, T)>` per resource type |
| Tick site | `draw_frame()` step 4 — after the in-flight fence wait, before recording |

Resources are not freed until `current_frame - frame_id >= countdown`.
The fence wait in step 1 of `draw_frame` guarantees all GPU work for
the fence slot is complete before the tick runs (#418).

---

## VRAM Rough Budget (RTX 4070 Ti, typical FNV interior)

| Subsystem | Typical | Peak |
|---|---|---|
| G-buffer (6 attachments × 2 FIF) | ~22 MB | ~45 MB (4K) |
| Scene SSBOs | ~140 MB | ~140 MB |
| ReSTIR reservoirs (2 FIF) | ~133 MB (1080p) | ~531 MB (4K) |
| Vertex / index pools | ~200 MB | ~1.6 GB cap |
| Textures (BC compressed) | ~400 MB | ~2 GB |
| BLAS structures | ~300 MB | ~1 GB (heavy scene) |
| TLAS + scratch | ~50 MB | ~256 MB |
| Pipeline cache blob | < 10 MB | — |
| **Estimated total** | **~1.25 GB** | **< 4 GB target** |

The 6 GB RT-minimum and 4 GB budget ceiling are not enforced by code;
they are design targets. The RTX 4070 Ti (12 GB) has headroom for all
known scene sizes. A warning fires when total allocated bytes exceed
80% of the smallest DEVICE_LOCAL heap (`(heap / 5) * 4`, with a 2 GB
fallback when no DEVICE_LOCAL heap is reported).

---

## See Also

- [`constants.rs`](../../crates/renderer/src/vulkan/scene_buffer/constants.rs) — all `MAX_*` values
- [`acceleration/constants.rs`](../../crates/renderer/src/vulkan/acceleration/constants.rs) — BLAS/TLAS slack + eviction thresholds
- [Shader Pipeline](shader-pipeline.md) — SSBO sizes in context of descriptor sets
- [Vulkan Renderer](renderer.md) — BLAS/TLAS lifecycle, LRU eviction, compaction
