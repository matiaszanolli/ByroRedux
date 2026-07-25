# 2145: CONC-D1-2026-07-25-03: FSR dispatch-failure recovery depends on undocumented FFX partial-recording behaviour

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2145
**Labels**: bug, low, vulkan

---

## Severity
LOW

## Dimension
Vulkan Queue & AS Sync — `/audit-concurrency` 2026-07-25

## Location
`crates/renderer/src/vulkan/frame_upscaler.rs:441-473`, `:479-521`

## Description
`FrameUpscaler::record` treats an `ffxDispatch` error as "nothing was recorded except my own boundary barriers," but the vendored SDK's `ExecuteGpuJobsVK` records every queued job into the command buffer and only checks the error code after the loop — a mid-sequence failure can already have recorded barriers/dispatches. The recovery path happens to be correct today (verified independently: FFX transitions land on states the pre-barriers already established as a no-op, and the blit's src stage/access mask includes `COMPUTE_SHADER`, ordering any partial FFX storage writes before the recovery blit's transfer write) — but the correctness is incidental, not designed.

## Evidence
`ffx_vk.cpp:4198-4236` records all jobs before checking `errorCode`; `frame_upscaler.rs:441-467` recovery assumes only its own barriers ran.

## Impact
A future narrowing of the "over-broad" blit masks (an attractive-looking cleanup) would silently reintroduce a same-command-buffer WAW with no test coverage.

## Trigger Conditions
Requires an actual `ffxDispatch` failure (SDK OOM, internal overflow, device-lost mid-frame). Not reproducible on demand; one-shot latch per swapchain generation.

## Verification Path
Not reachable by `cargo test`. Validation-layer confirmation needs a fault-injected dispatch failure; practical mitigation is documentation plus keeping the currently over-broad masks.

## Related
Same underlying SDK behaviour as CHAIN-D2-03 (filed separately). commit `f9a42e07`, `frame_upscaler.rs:808-818` (`blit_output_src_access`, unit-tested).

## Suggested Fix
Add a comment at `frame_upscaler.rs:441` recording that FFX `ExecuteGpuJobsVK` records all jobs before checking its error code, so the wide src mask on the recovery blit is documented as load-bearing, not defensive padding. No code change required.

## Completeness Checks
- [ ] **TESTS**: Documentation-only fix, no test required
