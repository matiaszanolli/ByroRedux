# #1211 — REN-SAFETY: draw_frame panics on framebuffers[0] when swapchain recreate failed

**Source**: bench attempt 2026-05-20, FNV exterior streaming on NVIDIA-mismatched system → iGPU fallback → Wayland dmabuf surface loss → recreate failure → draw panic
**Severity**: medium / Labels: bug, medium, renderer, safety, vulkan
**State**: OPEN (filed 2026-05-20)

## Cause

`crates/renderer/src/vulkan/context/draw.rs:356` indexes `self.framebuffers[frame]` unconditionally. When `recreate_swapchain` fails partway (surface lost, format query failed, etc.), `framebuffers` is left at len=0. Caller at `byroredux/src/main.rs:1098-1099` and `:1322-1324` logs the error and continues — next event-loop tick calls `draw_frame` → panic.

## Fix

Two layers:
1. Early-return in `draw_frame` when `self.framebuffers.is_empty()` — load-bearing
2. Optional: `swapchain_invalid: bool` flag in main.rs gating draw_frame calls until next successful recreate

Regression test: mocked `framebuffers: Vec::new()`, assert `draw_frame` returns `Ok(())` without panic.

## Risk

LOW — skipped-frame is observationally indistinguishable from a non-rendered frame. Caller already handles retry.

## Estimated impact

Crash safety only. Surface loss IS normal Vulkan (window minimize, monitor disconnect, compositor restart, driver crash recovery). Engine should degrade gracefully.
