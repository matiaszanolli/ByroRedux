# Performance Audit — 2026-05-19 (Dim 7 focused)

**Scope**: Dimension 7 — TAA & GPU Skinning Cost (M37.5 + M29.5 + M29.6)
**Mode**: `--focus 7 --depth deep`
**Other dimensions**: not run in this pass (single-dim audit per `/audit-performance 7`)

## Executive Summary

- **0 CRITICAL · 0 HIGH · 4 MEDIUM · 5 LOW · 4 INFO**
- Estimated FPS impact ceiling on Prospector (34 skinned NPCs, ~20 idle):
  ~3 ms/frame combined recoverable from PERF-DIM7-01 + 02 (skin-dispatch + BLAS-refit gating) — **needs measurement** (no per-pass GPU timer today).
- VRAM win bounded: 5–50 MB on exterior→interior transitions from PERF-DIM7-04 (skinned BLAS scratch shrink), one-line dedup of open issue **#1127**.
- TAA verified clean: O(pixels) only with 22 taps/pixel; Halton(2,3) period-16 jitter wired (#1093), motion-vector point sample correct, variance clamp γ=1.5 (#1108), NaN/Inf guard live.
- M29.6 persistent `bind_inverses` SSBO + slot-pool architecture verified live and correct; hotfix bundle (#1191 / #1192 / #1193) confirmed in code.
- TAA + skin-compute + skin-palette pipelines all consume the global `VkPipelineCache` (per #381). No throwaway caches.
- M29.3 raster fast-path remains deferred (triangle.vert still inlines weighted-matrix sum); output buffer deliberately not flagged `VERTEX_BUFFER` per #681.

### Dedup notes vs prior audits

- **#1127** (REN-D2-NEW-01) — still applies; re-listed as PERF-DIM7-04.
- **PERF-D7-NEW-01** (2026-05-16, per-frame scratch allocs in skin chain) — **resolved by #1133** (the three `Vec::new()` are now `mem::take` from `skin_*_scratch` siblings on `self`, context/draw.rs:741/743/811). Removed.
- **PERF-D7-NEW-02** (2026-05-16, fixed-stride bone palette zero-pad) — **architecture changed by M29.6**. The "all 32 slots × 6.4 KB padding/frame" cost is gone from `upload_bone_worlds`. Reframed as PERF-DIM7-09 (residual MBPM-stride on `bone_world`).

### Infrastructure gap (carried forward)

dhat / alloc-counter regression coverage is **NOT wired** (2026-05-04 baseline). None of the Dim 7 findings here are alloc-hot-path (they are GPU-bandwidth and dispatch-count), so this gap does not directly bear on Dim 7 numbers — but the proposed CPU-side hash gate in PERF-DIM7-01 would benefit from a future dhat regression test.

---

## Hot Path Analysis (per-frame, Prospector baseline, 34 skinned NPCs / 20 idle)

| Pass | Per-frame ops | Status |
| --- | --- | --- |
| TAA dispatch | 22 taps × pixels (constant per-pixel); ~1.1 GB RGBA16F bandwidth at 720p | Clean — PERF-DIM7-10 |
| TAA history buffers | 2 × RGBA16F @ swapchain res (~127 MB at 4K) | Resize-only churn — PERF-DIM7-05 |
| Skin compute dispatch | 34 dispatches × ~5K vertices, **no idle gate** | Wasteful — PERF-DIM7-01 |
| Skinned BLAS refit (UPDATE) | 34 refits, **no idle gate** | Wasteful — PERF-DIM7-02 |
| Descriptor writes (skin) | 3× per entity per frame = 102 writes | Wasteful — PERF-DIM7-03 |
| Skinned BLAS scratch | Grow-only across session peak | VRAM bloat — PERF-DIM7-04 |
| Bone palette (`bone_world`) | MBPM=144 stride × slot; ~120–160 KB/frame zero-pad | Residual — PERF-DIM7-09 |
| Bone palette (`bind_inverses`) | Persistent SSBO, single seed-once write per slot | Clean — PERF-DIM7-12 |
| Pending bind_inverses upload | Cap of 16 / frame → 2-frame partial population at first-sight | LOW — PERF-DIM7-07 |
| Skin output buffer | Lazy alloc, despawn-bounded, ~2–7 MB ceiling at 227 slots | Clean — PERF-DIM7-11 |
| Pipeline caches | All three compute pipelines use global cache | Clean — PERF-DIM7-06 |

---

## Findings — by Severity

### MEDIUM

#### PERF-DIM7-01: Skin compute dispatch fires per-entity every frame regardless of whether bones changed

- **File**: [crates/renderer/src/vulkan/context/draw.rs:887-911](crates/renderer/src/vulkan/context/draw.rs#L887-L911)
- **Symptom**: Idle NPC (no animation / paused / T-pose waiting) still pays full `skin_vertices.comp` dispatch + BLAS refit every frame. On Prospector with 34 NPCs that's 34 dispatches × ~5K vertices regardless of motion.
- **Cause**: Dispatch loop walks `dispatches` unconditionally; no per-entity "bones changed" gate. `build_skinned_palettes` re-uploads `bone_world` for every allocated slot whether GlobalTransform moved or not.
- **Fix** (two viable strategies — both speculative until measured):
  1. CPU gate: xxhash3 the per-entity bone_world slice in `build_skinned_palettes`; cache previous-frame hash; skip dispatch + refit when unchanged AND slot already populated. ~0.3 µs / slot, ~10 µs total at 34 slots.
  2. ECS gate: route `AnimationPlayer::dirty` through the `SkinnedMesh` query so unchanged players skip palette construction entirely.
- **Estimated Impact**: needs measurement. Theoretical upper bound ~0.5–1 ms / frame at 60% idle rate.
- **Regression Risk**: HIGH — a missed gate (animation done but `dirty` not flipped, mark_skinned_visible races) leaves BLAS holding stale geometry. Gate only in steady state once slot already has populated output buffer + live BLAS; never skip first-sight.
- **Testability**: No measurement infra. Extend `SkinCoverageFrame` (skin_compute.rs:102) with `dispatches_skipped: u32`; surface through `tex.skin`. Add `bench-stats --break-down skin`.

#### PERF-DIM7-02: BLAS refit unconditional even when skin compute dispatch was a no-op

- **File**: [crates/renderer/src/vulkan/context/draw.rs:994-1024](crates/renderer/src/vulkan/context/draw.rs#L994-L1024)
- **Symptom**: Every entity in `dispatches` gets `refit_skinned_blas` UPDATE. ~30–80 µs each on 5K-vertex mesh × 34 entities = ~1.7 ms unaccounted BLAS work / frame on Prospector.
- **Cause**: No gate paired to PERF-DIM7-01's hypothetical dispatch gate. BLAS whose vertex buffer wasn't written this frame doesn't need refit.
- **Fix**: Same skip-flag plumbing as PERF-DIM7-01 (1:1 paired). Critical sub-fix: bump `last_used_frame` on the skipped path so LRU at draw.rs:1077 doesn't reap a quiescent-but-live slot.
- **Estimated Impact**: needs measurement. Combined with PERF-DIM7-01: ~3 ms / frame off Prospector skin chain at 20-idle / 34 NPCs.
- **Regression Risk**: HIGH — split decisions are the trap. Compute and refit must gate on the **same** bool.
- **Testability**: same as 01 + `--bench-hold` synthetic with one NPC held still; assert refit count → 0.

#### PERF-DIM7-03: Per-dispatch descriptor-set rewrite at 60 fps in steady state

- **File**: [crates/renderer/src/vulkan/skin_compute.rs:425-446](crates/renderer/src/vulkan/skin_compute.rs#L425-L446)
- **Symptom**: `vkUpdateDescriptorSets` fires 3× per skinned entity per frame (input / palette / output). 102 writes / frame on Prospector. ~1–3 µs each on NVIDIA driver.
- **Cause**: Doc at skin_compute.rs:144-151 acknowledges the inline-rewrite choice. Inputs are actually stable between cell transitions; output is fixed per slot for slot lifetime. Writes happen anyway.
- **Fix**: Move input + palette + output writes from `dispatch` to a `mark_slot_resident` call that runs on slot creation + on cell transition (when global vertex SSBO changes). Hook the cell-transition rewrite into `MeshRegistry::rebuild_geometry_ssbo` exit.
- **Estimated Impact**: ~100–300 µs / frame at 34 entities. Below FPS-signal threshold.
- **Regression Risk**: MEDIUM — stale buffer reference if rebuild doesn't trigger rewrite. Mitigation: same site that bumps the static-BLAS-map invalidation counter.
- **Testability**: dhat won't see it. Track `descriptor_writes_per_frame` in `SkinComputePipeline`; surface through `tex.skin`.

#### PERF-DIM7-04: Skinned BLAS scratch buffer is grow-only (dedup of #1127)

- **File**: [crates/renderer/src/vulkan/acceleration/blas_skinned.rs:193-226](crates/renderer/src/vulkan/acceleration/blas_skinned.rs#L193-L226), [crates/renderer/src/vulkan/acceleration/memory.rs:40-102](crates/renderer/src/vulkan/acceleration/memory.rs#L40-L102)
- **Symptom**: `blas_scratch_buffer` grows to session peak across all skinned BLAS builds and never shrinks. After exterior cell (~100 NPCs) settles into small interior, scratch stays sized for worldspace peak.
- **Cause**: `shrink_blas_scratch_to_fit` walks `self.blas_entries` (static BLAS only) for `build_scratch_size` peak — doesn't account for skinned-BLAS first-sight peak even when invoked from cell-unload.
- **Fix**: Chain `.chain(self.skinned_blas.values())` into the peak scan at [memory.rs:51](crates/renderer/src/vulkan/acceleration/memory.rs#L51). `BlasEntry::build_scratch_size` is already recorded on every skinned entry at blas_skinned.rs:177 and 296.
- **Estimated Impact**: 5–50 MB DEVICE_LOCAL VRAM freed on exterior→interior transitions. Below FPS-signal threshold; matters on 6 GB VRAM dev box at session length > 1 h.
- **Regression Risk**: LOW — shrink path already documents its fenced-call-site contract at memory.rs:34-37.
- **Testability**: `accel.total_blas_bytes() / blas_scratch_buffer.size` via `tex.stats`; before/after on repeatable cell-pair transition.

---

### LOW

#### PERF-DIM7-05: TAA history image resize destroys + reallocates 2× full RGBA16F

- **File**: [crates/renderer/src/vulkan/taa.rs:781-845](crates/renderer/src/vulkan/taa.rs#L781-L845)
- **Symptom**: Every swapchain recreate destroys both history images + allocates 2 fresh RGBA16F at new extent. ~127 MB churn at 4K per resize. Minimize→restore can trigger several in a row.
- **Cause**: `recreate_on_resize` unconditionally drains + destroys + recreates. No reuse when only extent moves.
- **Fix**: Not worth fixing today. Documented for the next pass.
- **Estimated Impact**: 0 in steady state; ~5–10 ms hitch on resize.
- **Regression Risk**: N/A.

#### PERF-DIM7-06: TAA + skin compute pipeline caches verified consistent with #381

- **File**: [crates/renderer/src/vulkan/taa.rs:178-369](crates/renderer/src/vulkan/taa.rs#L178-L369), [crates/renderer/src/vulkan/skin_compute.rs:162-298](crates/renderer/src/vulkan/skin_compute.rs#L162-L298), [crates/renderer/src/vulkan/skin_compute.rs:533-655](crates/renderer/src/vulkan/skin_compute.rs#L533-L655)
- **Symptom**: None — verification. All three pipelines accept the global `pipeline_cache` argument and pass it to `create_compute_pipelines`. No throwaway caches.

#### PERF-DIM7-07: Pending bind_inverses upload cap of 16 spreads large first-sight frames

- **File**: [crates/renderer/src/vulkan/scene_buffer/constants.rs:44](crates/renderer/src/vulkan/scene_buffer/constants.rs#L44), [crates/renderer/src/vulkan/scene_buffer/upload.rs:207](crates/renderer/src/vulkan/scene_buffer/upload.rs#L207)
- **Symptom**: Cell with > 16 skinned NPCs first-sighting in the same frame (FO4 MedTek 23 SkinnedMesh; FO3 Megaton REFR spill) takes 2 frames to populate persistent SSBO. Un-uploaded entities render in bind pose for one frame (palette = identity × identity per #1191).
- **Cause**: Cap chosen as 16 × MBPM × 64 B = 144 KB HOST_VISIBLE staging — comfortable but conservative.
- **Fix**: Bump `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME` from 16 to `MAX_TOTAL_BONES / MAX_BONES_PER_MESH = 227`. Staging cost moves 144 KB → ~2 MB. Eliminates one-frame bind-pose glitch.
- **Estimated Impact**: 0 FPS; visual-quality win on cell-load frames.
- **Regression Risk**: LOW — verify no test pins the literal 16.
- **Testability**: synthetic cell-load test with 30+ NPCs; screenshot at frame N / N+1 / N+2.

#### PERF-DIM7-08: TAA history descriptor write covers slot 0 in UNDEFINED layout on frame 0 — guarded but fragile

- **File**: [crates/renderer/src/vulkan/taa.rs:506-570](crates/renderer/src/vulkan/taa.rs#L506-L570)
- **Symptom**: None today — first-frame guard at taa.comp:96 skips prev_history texelFetch when `params.params.y > 0.5`. Future shader edit touching prev_history outside the guard would VUID-violation crash.
- **Cause**: Known guard documented at taa.rs:514-523. Listed because the checklist asks.
- **Fix**: `initialize_layouts` (taa.rs:604-637) already walks UNDEFINED→GENERAL at startup; `VulkanContext::new` calls it. Optional belt-and-braces: `debug_assert` at top of `dispatch` that `frames_since_creation == 0 ↔ params.params.y > 0.5`.
- **Regression Risk**: LOW — validation layer catches it if the guard fails.

#### PERF-DIM7-09: Bone palette still MBPM-strided post-M29.6; partial poses pay full 144-slot zero-pad

- **File**: [crates/renderer/src/vulkan/scene_buffer/constants.rs:30](crates/renderer/src/vulkan/scene_buffer/constants.rs#L30), `byroredux/src/render/skinned.rs` (`build_skinned_palettes`)
- **Symptom**: Reframe of 2026-05-16 PERF-D7-NEW-02 post-M29.6. `bind_inverses` is bandwidth-clean now (persistent SSBO, write-once). `bone_world` is still MBPM-strided per slot per frame with zero-pad for partial poses (typical Bethesda NPC uses 60–90 of 144 bones). ~120–160 KB / frame zero-pad at 34 NPCs.
- **Cause**: `MAX_BONES_PER_MESH = 144` is the descriptor-side stride; shader uses it as fixed offset multiplier (skin_vertices.comp:137, triangle.vert:149). Removing zero-pad needs per-mesh `bone_count` in `GpuInstance` + prefix-sum offset OR per-entity bone-count uniform.
- **Fix**: Same path as the deferred PERF-D7-NEW-02; on ROADMAP. Defer until M29.x stabilises further.
- **Estimated Impact**: ~125 KB / frame bandwidth at FO4 interior populations. Below FPS-signal. Above it on Whiterun-class crowds (~3 MB / frame at 600 NPCs).
- **Regression Risk**: HIGH — variable-stride packing changes the descriptor offset arithmetic for compute + raster. Milestone-grade, not hotfix.
- **Testability**: needs explicit per-frame transfer-size counter through existing `bone_input_upload_bytes` telemetry.

---

### INFO (no-finding verification pass-throughs)

#### PERF-DIM7-10: TAA shader correctness pass

- **File**: [crates/renderer/shaders/taa.comp:76-223](crates/renderer/shaders/taa.comp#L76-L223)
- **Item 1 (O(pixels))**: confirmed. 22 taps / output pixel constant. At 4K (8.3 Mpix): ~183 Mtaps / frame. RGBA16F bandwidth at 720p ~1.1 GB / frame — comfortable on 504 GB/s 4070 Ti.
- **Item 3 (Halton + motion + variance clamp)**: Halton(2,3) jitter at draw.rs:415-421 with period-16 fix (#1093). Motion vectors point-sampled (taa.rs:531-533). Variance clamp γ=1.5 (#1108). NaN/Inf guard at taa.comp:205-207.

#### PERF-DIM7-11: Skin compute output buffer lifecycle is correct

- **File**: [crates/renderer/src/vulkan/skin_compute.rs:306-396](crates/renderer/src/vulkan/skin_compute.rs#L306-L396), [crates/renderer/src/vulkan/context/draw.rs:1077-1107](crates/renderer/src/vulkan/context/draw.rs#L1077-L1107)
- Per-slot `output_buffer` sized exactly to `vertex_count × VERTEX_STRIDE_BYTES`. Vertex-count change trips `validate_refit_counts` VUID-03667 guard (predicates.rs:85); `drop_skinned_blas` routes through `pending_destroy_blas`. On despawn, `drain_pending_skin_unload_victims` → eviction → `destroy_slot` (FREE_DESCRIPTOR_SET). Slot-pool capacity 227 × ~9–30 KB per mesh = ~2–7 MB upper bound. No leaks.

#### PERF-DIM7-12: Per-frame bone palette upload is single-buffered against MAX_TOTAL_BONES

- **File**: [crates/renderer/src/vulkan/scene_buffer/upload.rs:139-188](crates/renderer/src/vulkan/scene_buffer/upload.rs#L139-L188), [crates/renderer/src/vulkan/scene_buffer/buffers.rs:501-518](crates/renderer/src/vulkan/scene_buffer/buffers.rs#L501-L518)
- M29.6 verified live. `bone_world_staging_buffers[frame]` HOST_VISIBLE 2 MB; `bone_world_device_buffers[frame]` DEVICE_LOCAL 2 MB; one `cmd_copy_buffer` per frame at actually-written byte size. `bind_inverses_persistent` DEVICE_LOCAL 2 MB written by `record_pending_bind_inverse_copies` with per-slot regions, single `cmd_copy_buffer` + whole-buffer barrier. Slot-0 identity seeded per #1191. No per-mesh upload churn.

#### PERF-DIM7-13: M29.3 raster fast-path deferred, confirmed not landed

- **File**: [crates/renderer/shaders/triangle.vert:132-158](crates/renderer/shaders/triangle.vert#L132-L158)
- triangle.vert still inlines weighted-matrix sum. Pre-skinned vertex SSBO consumed only by BLAS refit ([acceleration/blas_skinned.rs:341](crates/renderer/src/vulkan/acceleration/blas_skinned.rs#L341)). Output buffer deliberately not flagged `VERTEX_BUFFER` per #681 (memory-type-mask bloat). M29.3 ALU win (~50 ops / vertex) remains milestone-gated.

---

## Prioritized Fix Order

### Quick wins (one-line / few-line, low-risk)

1. **PERF-DIM7-04** — `.chain(self.skinned_blas.values())` at [memory.rs:51](crates/renderer/src/vulkan/acceleration/memory.rs#L51). Dedup of #1127, ships VRAM headroom on cell transitions. **LOW risk.**
2. **PERF-DIM7-07** — bump `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME` 16 → 227. Eliminates one-frame bind-pose glitch on heavy first-sight frames. **LOW risk.**

### Instrumentation prerequisite (unblocks the MEDIUM tier)

3. Add `dispatches_skipped: u32` + GPU-pass timer to `SkinCoverageFrame`; surface through `tex.skin` and `bench-stats --break-down skin`. **No risk.** Without this, PERF-DIM7-01 / 02 / 03 land as guesses.

### Larger wins (MEDIUM, instrumentation-gated)

4. **PERF-DIM7-01 + 02** — paired bones-changed gate on skin dispatch + BLAS refit. **HIGH regression risk** without the instrumentation in step 3. Ship as a feature flag first; promote after the synthetic `--bench-hold` test passes.
5. **PERF-DIM7-03** — move descriptor writes from `dispatch` to `mark_slot_resident`. **MEDIUM risk.** Lower priority than 4; ~100–300 µs / frame win.

### Architectural / milestone-grade (defer)

6. **PERF-DIM7-09** — variable-stride bone palette. Defer to a dedicated M29.x milestone; not a hotfix.
7. **PERF-DIM7-05** — TAA history-image reuse on resize. Skip for now.
8. **PERF-DIM7-08** — TAA `debug_assert` belt-and-braces. Optional polish.

---

## Notes

- "Needs measurement" appears on PERF-DIM7-01 / 02 / 03 because the engine has no per-pass GPU timer today. `SkinCoverageFrame` reports dispatch counts but not GPU time. Adding the timer is a prerequisite for the MEDIUM-tier MEDIUM-confidence fixes — see prioritization step 3.
- No Vulkan-spec-speculative changes proposed (per [[feedback_speculative_vulkan_fixes]]). All barriers on the skin chain (draw.rs:919-925 COMPUTE→AS_BUILD, draw.rs:1028-1034 AS_WRITE→AS_READ, draw.rs:658-674 palette COMPUTE→COMPUTE|VERTEX) are documented and traced to specific issue numbers.
- M29.6 hotfix bundle (commit 8ea8d61d): #1191 slot-0 identity seed, #1192 pending re-queue, #1193 bounds assert — all three verified live in code.
