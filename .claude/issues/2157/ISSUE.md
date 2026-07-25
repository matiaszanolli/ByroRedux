# 2157: RL-D6-04: One-time command buffer still leaked on two ? paths in with_one_time_commands_inner; blast radius widened by new FSR callers

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2157
**Labels**: bug, low, vulkan

---

## Severity
LOW

## Dimension
Resource Lifecycle (GPU teardown ordering) — `/audit-concurrency` 2026-07-25

## Status note
Follow-up to closed #1861 — that fix was narrowed to the post-submit failure paths, not a full close. Filing fresh since #1861 is closed but the underlying leak (2 of the original 3 sites) is still present, and the FSR work widened the blast radius.

## Location
`crates/renderer/src/vulkan/texture.rs:662-696`

## Description
#1861's fix covered the post-submit failure paths (`reset_fences`, `create_fence`, `queue_submit`, `wait_for_fences` — all now free the command buffer + destroy the fence). Two `?` sites still leak the allocated command buffer: `begin_command_buffer(...)?` and `end_command_buffer(...)?`. Neither frees `cmd`.

## Evidence
`texture.rs:666-668` and `:693-695` are the only remaining early returns between allocation and the #1861-annotated cleanup block; the recording-closure failure path is correctly handled.

## Impact
Unchanged magnitude from #1861 — bounded by how many one-time submits fail, both under device-loss/OOM where the process is already doomed. What changed: the FSR work added `FrameUpscaler::initialize_outputs` and `ExposureResource::initialize` as new callers, both re-entered on **every swapchain recreate** (not load-time-only anymore), which is why this stays open rather than closing as "one-shot init only."

## Related
#1861 (closed) — not a regression, strictly improved from 3 sites to 2, but its "load-time one-shot" framing is now stale given the new per-resize callers.

## Suggested Fix
Free `cmd` on both `?` paths (same two-line shape already used at `:683-684`), and amend #1861's description to note the per-resize FSR/exposure callers.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
