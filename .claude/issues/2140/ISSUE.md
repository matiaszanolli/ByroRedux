# 2140: CHAIN-D2-03: FSR dispatch-failure recovery assumes zero partial recording by the SDK before it errors — HYPOTHESIS

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2140
**Labels**: bug, medium, sync

---

## Severity
MEDIUM

## Dimension
Compute → AS → Fragment Chains — `/audit-concurrency` 2026-07-25

## Status note
HYPOTHESIS — not reachable by `cargo test`; needs a fault-injection harness before any code change.

## Location
`crates/renderer/src/vulkan/frame_upscaler.rs:441-468`, `:667-698` (`record_fsr_depth_restore`)

## Description
When `context.dispatch` returns `Err`, the recovery path latches `dispatch_failure` and records depth-restore + native-blit barriers whose declared `old_layout` values are correct only if the SDK recorded zero image transitions into `cmd` before failing. `ExecuteGpuJobsVK` in the vendored SDK (`third_party/fidelityfx-sdk-v1.1.4/sdk/src/backends/vk/ffx_vk.cpp:4187-4240`) iterates every queued GPU job and records each into the command buffer, checking `errorCode` only **after** the loop, with the code overwritten each iteration — so a mid-sequence failure can leave partially-recorded transitions while reporting `FFX_OK`, or an error can arrive after real work was already recorded.

## Evidence
`frame_upscaler.rs:453-457` SAFETY comment: "`record_fsr_barriers_before` established the exact layouts these two transition out of" — true only under the zero-partial-recording assumption. `blit_output_src_access` (`:812-818`) encodes the same assumption in code.

## Impact
On a real SDK dispatch rejection, this could produce a device loss or corrupted frame instead of the intended graceful degradation to the native blit — a crash-on-crash in the exact path meant to handle "something already went wrong."

## Trigger Conditions
Any `ffxFsr3UpscalerContextDispatch` failure — SDK OOM, invalid descriptor, device-lost mid-frame. Rare, never exercised on the happy path.

## Verification Path
Add a debug-only env gate (e.g. `BYRO_FSR_FORCE_DISPATCH_FAIL=1`) making the FFI shim's `dispatch` return `Err` without calling into the SDK, to isolate "recovery is sound when nothing was recorded." Separately, run `BYRO_VALIDATION=1` with a genuinely invalid dispatch description to see whether the SDK records before validating. Confirming signal: validation reporting an `oldLayout` mismatch on the depth or output image only on the forced-failure frame.

## Related
CHAIN-D2-02 (same boundary, filed separately), commit `f9a42e07` ("survive an FSR dispatch failure instead of dropping the frame").

## Suggested Fix
Not on reasoning alone. If the SDK is confirmed to record before it can fail, the robust shape is recording the FSR boundary barriers + dispatch into a secondary command buffer that is simply not executed on failure — a real restructure, not to be attempted without the repro above.

## Completeness Checks
- [ ] **DROP**: N/A pending validation result
