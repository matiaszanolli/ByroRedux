# 2152: CHAIN-D2-05: ReSTIR reservoir ping-pong reads never-initialised device memory on first-use frames

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2152
**Labels**: bug, low, vulkan

---

## Severity
LOW

## Dimension
Compute → AS → Fragment Chains — `/audit-concurrency` 2026-07-25

## Location
`crates/renderer/src/vulkan/restir.rs:52-60,102-131`; `crates/renderer/shaders/triangle.frag:2485-2530`

## Description
`ReservoirBuffers` are allocated with `create_device_local_uninit` and never cleared on creation or resize. The temporal ping-pong therefore reads undefined device memory on each slot's first use and again after every `recreate_on_resize`, relying entirely on shader-side validation (`sameSurface && ... && rp.M > 0.0 && rp.W > 0.0 && !isnan && !isinf`) instead of an explicit clear.

## Evidence
The shader gate is genuinely strong, but the surface tag is a masked field (`packReservoirLightAndSurface`) with well under 32 bits of effective comparison width — garbage that happens to match the masked surface ID plus a finite positive W/M and an in-range light index will be accepted.

## Impact
At worst a small number of single-frame bright specks on the first frames after launch or a resize — visually indistinguishable from the temporal-discontinuity recovery window already scheduled for those exact frames. Not a correctness cliff. SVGF/TAA (the analogous consumers) do clear their history on init; ReSTIR is the outlier.

## Trigger Conditions
Frames 0-1 of a session; frames 0-1 after any resize or runtime upscaler switch.

## Verification Path
Add a `vkCmdFillBuffer(0)` on both slots inside `ReservoirBuffers::new`/`recreate_on_resize` behind a debug env var and compare the first two frames' output. If identical, the shader validation is sufficient and this closes as documentation-only.

## Related
#1814 (closed, PERF-D5-NEW-04), commit `e5d02f83`, `svgf.rs:183-185` (`should_force_history_reset`).

## Suggested Fix
A one-time `vkCmdFillBuffer(0)` in `ReservoirBuffers::new` and `recreate_on_resize` (near-free, once per swapchain generation — requires adding `TRANSFER_DST` to buffer usage), or an explicit per-slot `frames_since_creation` gate mirroring SVGF/TAA.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (compare first-two-frame output before/after)
