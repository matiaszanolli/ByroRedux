# PERF-D5-2026-09-21-01: The top-of-frame wait on every frame-in-flight fence idles the GPU for all of `draw_frame`'s CPU recording, on every GPU-bound frame

**Labels**: bug, renderer, medium, vulkan, sync, performance

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: MEDIUM (throughput: suboptimal CPU/GPU pipelining, no correctness impact) · **Dimension**: 5 — GPU Pipeline
**Location**:
- The wait: `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:65`, `wait_for_fences(&self.frame_sync.in_flight, true, u64::MAX)` in `sync_and_acquire_frame`. The premise comment is at `:59-60`.
- `crates/renderer/src/vulkan/context/draw.rs:1812`: `draw_frame`'s first step (`sync_and_acquire_frame`). Recording follows (`cmd_t0` at `:1952`, `cmd_record_ns` at `:2125`), then the single `queue_submit` at `:2197`.
- `crates/renderer/src/vulkan/sync.rs:20-97`: the rider list that depends on the wait staying all-slots.

**Status**: NEW for the throughput cost. The mechanism predates the baseline: #282 introduced it, and #3442 kept it all-slots at N = 2.
**Verified against**: HEAD `73aaed7b9`, which is docs-only on top of the audited `f97775ca8`. Read at the symbols.

## Description

With `MAX_FRAMES_IN_FLIGHT = 2`, the "other" slot's fence belongs to frame N-1. On a GPU-bound frame, that is the submission the GPU is still executing. `sync_and_acquire_frame` waits on the whole `in_flight` array before `draw_frame` N records anything, so the GPU queue is empty when the wait returns. From then until N's `queue_submit`, the GPU has nothing to run while the CPU does:

- swapchain acquire;
- camera/light assembly;
- skin and TLAS recording (`tlas_build`);
- `build_and_upload_instances` (`ssbo_build`);
- geometry and post-pass recording (`cmd_record`, disjoint from the other buckets);
- the submit call itself.

The frame period is therefore about `max(T_pre, GPU) + T_post`, not `max(T_pre + T_post, GPU)`. Only the app work before `draw_frame` overlaps the GPU. The engine keeps one frame in flight at the `draw_frame` boundary, not two.

The comment at `:59-60` says: "Cost stays zero in practice — the GPU is rarely more than 1 frame behind the CPU, so the other fences are almost always signaled". That is backwards for the GPU-bound case, where the other fence is exactly the frame the GPU is still running.

## Evidence

- `t.fence_wait_ns` is measured around exactly this call (`sync_and_acquire_frame.rs:61-68`).
- The bench-of-record medians (`docs/audits/BENCH_stepped-camera_4c9a5b36.tsv`, re-read at publish time) show most of each GPU-bound frame spent in this wait:
  - Prospector TAA: `fence_ms` 10.59 of 13.08 ms `wall_ms`;
  - Prospector FSR-Q: 7.39 of 9.58;
  - Whiterun TAA: 7.53 of 10.74;
  - MedTek TAA: 16.12 of 37.64.
- The upper bound on the post-wait share is `wall − fence`: 2.49 ms Prospector TAA, 3.21 ms Whiterun TAA, 6.99 ms Dugout TAA. The bound also includes CPU work before `draw_frame`, so it is loose.

## Impact

- GPU-bound scenes (the RT default) lose `T_post` every frame. At about 1 ms of post-wait CPU, that is about 8-10 % throughput (*est.*).
- Every CPU cost inside `draw_frame`, such as the instance build and command recording, becomes frame time even on GPU-bound frames.
- Confidence: high on the mechanism (fence semantics plus `draw_frame`'s order). Medium on the magnitude: the post-wait share has not been measured.

## Related

- #4601 (CONC-D1-2026-09-21-01, open, MEDIUM): the *safety* side of the same wait. Its all-slots argument is unpinned, and nine riders depend on it: the seven in the #870/#3643 list plus the post-present TLAS scratch shrink and ground-cover `prepare`. That issue mentions the lost overlap only as context.
- #282: the SVGF previous-slot G-buffer read, the original reason for waiting on the other slot.
- #870, #3643: the rider list. #3442 made the wait all-slots. #4516 added the last rider.

## Suggested Fix

Measure first. On a GPU-bound TAA bench, the `cpu_ms:` sum `acquire + tlas_build + ssbo_build + cmd_record` is the per-frame GPU idle. An Nsight Systems or RenderDoc timeline shows the queue gap directly. If `T_post` is about 1 ms or more:

1. Make all nine riders per-FIF, or defer-destroy them (#4601's list).
2. Cover the SVGF previous-slot G-buffer read (#282) with an in-command-buffer barrier. A barrier's first scope includes earlier submissions on the same queue.
3. Only then wait on `in_flight[frame]` alone.

Changing the wait first is exactly the nine-site use-after-free that #4601 warns about. The speculative-Vulkan rule applies: validate with `BYRO_VALIDATION=1` plus RenderDoc on both upscaler modes before landing. Either way, correct the "cost stays zero" comment at `:59-60`.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D5-2026-09-21-01)

## Completeness Checks
- [ ] **MEASURE**: `T_post` captured on a GPU-bound TAA bench (Prospector / Whiterun) before any wait-policy change
- [ ] **UNSAFE**: If the fix changes the fence wait or adds `unsafe`, the SAFETY comment states the upheld invariant
- [ ] **SIBLING**: All nine riders on the all-slots wait (#4601) are per-FIF or deferred before the wait narrows
- [ ] **DROP**: If Vulkan objects change (per-FIF rider copies), the Drop impl is still reverse-order correct
- [ ] **TESTS**: A pin fails if the wait narrows while any rider is still slot-shared (coordinate with #4601's argument pin)

