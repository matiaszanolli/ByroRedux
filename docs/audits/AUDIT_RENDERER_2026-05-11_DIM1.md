# Renderer Audit — 2026-05-11 (Dimension 1 focus)

**Scope**: Dimension 1 — Vulkan Synchronization (semaphores, fences, pipeline barriers, queue submit ordering, swapchain recreation).
**Depth**: deep.
**Baseline**: `AUDIT_RENDERER_2026-05-09.md` (Dim 1: 1 HIGH + 2 MEDIUM).
**Method**: orchestrator + single dimension agent.

## Executive Summary

- **Findings**: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 2 LOW, 3 INFO.
- **Pipeline areas affected**: draw-frame submission path; swapchain teardown/recreation.
- **Net verdict**: **CLEAN.** All three HIGH/MEDIUM items from 2026-05-09 (REN-D1-NEW-01, -02, -03) have shipped fixes (commits `165cd6b`, `d89071f`, `#906`) and were verified live in code. Remaining items are defensive LOWs.

## Sync Pipeline Assessment

- **Per-frame fence wait** before command-buffer reset: present, both slots waited (`draw.rs:144-156`).
- **Per-frame `render_finished` semaphore**: shipped via `#906`; signal at submit, wait at present, stage mask `COLOR_ATTACHMENT_OUTPUT` correct (`draw.rs:2075, 2098`).
- **`recreate_swapchain`**: `device_wait_idle` precedes destruction (`resize.rs:21-23`); fences recreated as SIGNALED post-resize via `FrameSync::recreate_for_swapchain` (`sync.rs:156-177`) — closes REN-D1-NEW-01.
- **TLAS → fragment/compute barrier**: present and correctly masked (`AS_WRITE → AS_READ`, `AS_BUILD → FRAGMENT | COMPUTE`) at `draw.rs:864-876`.
- **Cluster cull → fragment barrier**: present at `draw.rs:913-925`.
- **Composite UBO host barrier**: folded into the bulk barrier (`draw.rs:1356-1373`) via `#909` — closes REN-D1-NEW-03.
- **No `vkDeviceWaitIdle` in hot path**: only used in resize, shrink-buffer post-submit paths, and Drop.

## Findings

### [LOW] In-flight fence is reset BEFORE command recording — fallible code path leaves it UNSIGNALED
**Dimension**: Vulkan Sync
**Location**: `crates/renderer/src/vulkan/context/draw.rs:189-2090`
**Severity**: LOW
**Observation**: `reset_fences` at line 191 sits before a series of `?`-propagating fallible calls (`reset_command_buffer`, `begin_command_buffer`, host uploads, `end_command_buffer`) and the `queue_submit` at line 2090 that re-signals the fence. Any mid-frame error returns from `draw_frame` leaving `in_flight[frame]` UNSIGNALED; the next frame's both-slots `wait_for_fences` (`draw.rs:147`) then blocks at `u64::MAX` and the engine hangs.
**Why bug**: Logical deadlock, not a spec violation. The window covers ~1900 lines of fallible operations that almost never fail mid-frame in steady state, hence LOW. This is the hot-path counterpart of the resize-path issue fixed by `#908`.
**Fix**: Move `reset_fences` to immediately before `queue_submit` (canonical Khronos pattern), or re-create the fence SIGNALED on the error path.
**Confidence**: HIGH
**Dedup**: Adjacent to REN-D1-NEW-01 (resize path fixed in `#908`); hot-path mirror unaddressed.

### [LOW] `images_in_flight` invariant is implicit-but-correct
**Dimension**: Vulkan Sync
**Location**: `crates/renderer/src/vulkan/sync.rs:156-177` + `crates/renderer/src/vulkan/context/draw.rs:179-187`
**Severity**: LOW
**Observation**: `images_in_flight` is reset to all-null only on resize (`sync.rs:161`). The aliasing guard at `draw.rs:180` (`if image_fence != null && image_fence != in_flight[frame]`) prevents waiting on an unrelated frame's just-reset fence. With `MAX_FRAMES_IN_FLIGHT == 2` and ≥3 swapchain images, a stored fence handle can outlive a `reset_fences` by the other slot, but the both-slots wait at `draw.rs:144-156` ensures the stored handle is always signaled by the time it's read.
**Why bug**: Not a spec violation; the invariant ("a fence handle stored in `images_in_flight` is always either null or signaled when next read") is correct but unstated.
**Fix**: Add a rustdoc invariant comment on `FrameSync::images_in_flight`, or a `debug_assert!` on fence status after the line-183 wait.
**Confidence**: MED
**Dedup**: None.

### [INFO] `recreate_swapchain` correctly waits idle and recreates fences SIGNALED — REN-D1-NEW-01 verified shipped
**Dimension**: Vulkan Sync
**Location**: `crates/renderer/src/vulkan/context/resize.rs:21-23, 527-535` + `crates/renderer/src/vulkan/sync.rs:156-177`
**Severity**: INFO
**Observation**: `device_wait_idle` precedes destruction; `FrameSync::recreate_for_swapchain` destroys and recreates fences with `FenceCreateFlags::SIGNALED`. The `const_assert` at `sync.rs:32-36` pins the shared-depth-image constraint gating any raise of `MAX_FRAMES_IN_FLIGHT`.
**Dedup**: REN-D1-NEW-01 (2026-05-09) → **resolved** (commit `165cd6b` / `#908`).

### [INFO] Per-frame `render_finished` semaphores ship — MAILBOX discard race closed
**Dimension**: Vulkan Sync
**Location**: `crates/renderer/src/vulkan/sync.rs:60-72, 95-104` + `crates/renderer/src/vulkan/context/draw.rs:2075, 2098`
**Severity**: INFO
**Observation**: One `render_finished` semaphore per frame-in-flight (not per swapchain image). Submit signals `render_finished[frame]`; present waits on it with stage `COLOR_ATTACHMENT_OUTPUT`. Doc comment at `sync.rs:43-57` explains the MAILBOX reasoning.
**Dedup**: REN-D1-NEW-02 (2026-05-09 HIGH) → **resolved** (`#906`).

### [INFO] TLAS → fragment/compute and cluster-cull → fragment barriers present and correctly masked
**Dimension**: Vulkan Sync
**Location**: `crates/renderer/src/vulkan/context/draw.rs:864-876` (TLAS), `913-925` (cluster cull), `1356-1373` (host bulk barrier)
**Severity**: INFO
**Observation**: TLAS barrier: `AS_WRITE → AS_READ`, `AS_BUILD → FRAGMENT | COMPUTE`. Bulk host barrier covers all host-write SSBO sources with `HOST → VS | FS | DRAW_INDIRECT`. SVGF dispatch (`draw.rs:1842-1843`) and composite (`draw.rs:2043-2046`) input-attachment transitions are covered by render-pass subpass dependencies. Deeper barrier correctness for those passes is owned by Dim 8 and Dim 10.
**Dedup**: REN-D1-NEW-03 (2026-05-09 MEDIUM, composite UBO host barrier) → **resolved** (`d89071f` / `#909`).

## Prioritized Fix Order

Neither remaining item is urgent; bundle with the next LOW sweep.

1. **LOW** — Reorder `reset_fences` in `draw_frame` to sit immediately before `queue_submit`, or re-signal on the error path. Closes a latent error-path deadlock window mirroring `#908`.
2. **LOW** — Document the `images_in_flight` "always signaled when next read" invariant via rustdoc or a `debug_assert!`.

## Notes

- Dimensions 4, 8, 9, 11, 15 have recent focused audits on disk (`docs/audits/AUDIT_RENDERER_2026-05-*_DIM*.md`); other dimensions were last broadly swept on 2026-05-09. Sync (Dim 1) has now reached a stable steady state.
