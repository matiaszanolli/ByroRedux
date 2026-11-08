# CONC-2026-11-08-2.1: linear capture arms its readback and frame count at record time, not after queue_submit

_Immutable snapshot of the issue as filed (GitHub is authoritative for state). Source: docs/audits/AUDIT_CONCURRENCY_2026-11-08.md. Labels: low,sync,renderer,bug_

Found by `/audit-concurrency` 2026-11-08 (`docs/audits/AUDIT_CONCURRENCY_2026-11-08.md`, HEAD 0419437ab).

- **Severity**: LOW (diagnostic output only; no GPU hazard, no engine state corrupted)
- **Dimension**: Compute → AS → Fragment chains (latch promote-after-submit class); also surfaced as an INFO in Dim 1
- **Location**: `crates/renderer/src/vulkan/context/linear_capture.rs` (`linear_capture_copy_recorded`, called from `linear_capture_record_copy` and `linear_capture_record_copy_post_taa`; readback in `linear_capture_finish_readback`)

## Description
`linear_capture_copy_recorded` bumps `job.frames_recorded` and sets `pending_readback = Some(extent)` while the copy is only *recorded*. `draw_frame`'s three tail `Err` sites (`end_command_buffer`, `reset_fences`, `queue_submit`) discard the command buffer and roll back skin state and the staged skip-clear latches, but nothing touches the capture state.

## Evidence
After a discarded frame the next `linear_capture_finish_readback` sees `pending_readback = Some`, accumulates whatever the staging buffer held (the previous frame's pixels, or unspecified initial bytes on the first frame), counts it in `frames_read`, and the EXR is written with one wrong frame in the average and no error. The `screenshot` and `depth_capture` twins arm at record time the same way. The `dynamic_rgba` path already does the right thing (`note_frame_submitted` → `submitted(slot)`).

## Trigger Conditions
`--linear-capture` active plus a tail `Err` on a frame that recorded a copy — in practice device loss.

## Impact
One stale or uninitialised frame silently averaged into a reference EXR (the capture feeds the M-LIGHT oracle gates). Rare.

## Verification Path
`cargo test` only — a source-order scan of `draw.rs` / `linear_capture.rs`; no validation layer or RenderDoc needed (host-side ordering).

## Suggested Fix
Stage `frames_recorded` / `pending_readback` in a `pending_*` slot, promote it next to `promote_skip_clear_latches`, discard it next to `discard_pending_skip_clear_latches`. Pin it with a test shaped like `skip_clear_latches_are_promoted_after_submit_not_at_record_time`. Consider fixing the three capture twins together.

## Completeness Checks
- [ ] **SIBLING**: `screenshot.rs` and `depth_capture.rs` record-time arming handled the same way
- [ ] **TESTS**: a source-scan test pins promote-after-submit for the capture state
