# 2156: RL-D6-03: set_upscaler_mode failure is non-fatal at the call site and soft-locks the renderer into permanent frame-skip

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2156
**Labels**: bug, low, vulkan

---

## Severity
LOW

## Dimension
Resource Lifecycle (GPU teardown ordering) — `/audit-concurrency` 2026-07-25

## Location
`crates/renderer/src/vulkan/context/resize.rs:981-1039`, `byroredux/src/app_step.rs:338-341`

## Description
The two `recreate_swapchain` call sites in `main.rs` treat a resize failure as fatal (`log::error!` + `event_loop.exit()`); the new runtime-upscaler-switch call site does not — it logs and returns while the frame loop continues, after `set_upscaler_mode` has already destroyed TAA, rebound composite, mutated `renderer_config.upscaler`, and entered `recreate_swapchain` (which destroys framebuffers up front and only rebuilds them much later). Any `?` in between — including the new `upscaler.recreate(...)?` and `PresentationPipeline::new(...)?`, both before the framebuffer rebuild — leaves `framebuffers.len() == 0`, `self.presentation == None`, and a drained `FrameUpscaler`.

## Evidence
`app_step.rs:338-341` — no exit, no rollback, versus `main.rs:721-724`/`:969-972` which do exit. `frame_upscaler.rs:769-780` — on `Err`, the reassignment never runs, so `self` keeps the emptied vectors.

## Impact
Not memory-unsafe — the existing #1211 `framebuffers.is_empty()` guard converts it into a permanent "skip every frame" state rather than a panic. But the window never recovers on its own; only a later `WindowEvent::Resized` re-enters `recreate_swapchain`, and the user sees a frozen window with one log line.

## Trigger Conditions
Needs an allocation/SDK failure mid-switch.

## Related
#1211 (closed, the guard that downgrades this from a panic), #1671 (closed).

## Suggested Fix
Either mirror the `main.rs` policy (treat a failed `set_upscaler_mode` as fatal), or have it roll back `renderer_config.upscaler` to `previous` and retry `recreate_swapchain` once so the engine lands in a renderable state instead of a permanent frame-skip.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
