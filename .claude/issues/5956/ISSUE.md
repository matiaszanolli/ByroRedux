# CONC-2026-10-17-C3: The volumetrics and caustic skip-clear latches are committed at record time and never rolled back by the three tail `Err` sites

Labels: low,sync,renderer,bug
Source: docs/audits/AUDIT_CONCURRENCY_2026-10-17.md

- **Severity**: LOW (NEW; same class as #3991, which fixed the skin chain only)
- **Dimension**: Compute -> AS -> Fragment Chains (volumetrics skip path) / Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs:904-909` (`skip_clear_decision` + `self.volumetrics_cleared_on_skip[frame] = next_latch`; the caustic twin at `:536-537`); not reset by `rollback_skin_frame_state` (`context/skinned_blas_refit.rs:54`) nor in any `draw.rs` `Err` arm (`reset_fences`, `queue_submit`, `end_command_buffer`)
- **Status**: NEW (open-issue snapshot and `gh` searches for `cleared_on_skip`, "skip-clear latch submit failure" return nothing relevant; #3991's body lists volumetrics as a #917-compliant submit-time consumer, which holds for `history_valid` / `transport_fields` / `dispatched_this_frame` (promoted by `mark_frame_completed` after `queue_submit`) but not for this latch)
- **Description**: On a skipped frame (no TLAS yet, e.g. every frame at scene load) the first skip of a streak records `record_neutral_frame` (a `vkCmdClearColorImage` of `integrated_volumes[frame]`) and sets `volumetrics_cleared_on_skip[frame] = true` immediately. If that frame then fails at `end_command_buffer` / `reset_fences` / `queue_submit`, `draw_frame` returns `Err` before `current_frame` advances (`draw.rs:736`), the command buffer is discarded, the clear never runs, and the latch stays `true`. The retry on the same slot is again a skip, `skip_clear_decision(false, true)` returns `(false, true)`, so no clear is recorded. Composite keeps sampling the slot's last real integrated volume (the previous cell's fog), while the other slot shows the neutral volume: a visible flicker that persists until a real dispatch lands on that slot.
- **Trigger Conditions**: a skip-clear frame, followed by a tail submit failure on exactly that frame (host/device OOM, `ERROR_SURFACE_LOST`-style churn, i.e. the cases `draw.rs`'s #3837/#910 comments call reachable). Rare. Self-heals on the first real dispatch for the slot.
- **Verification Path**: pure CPU. A unit test that drives the decision + a simulated `Err` return is enough once the latch is promoted at submit. No RenderDoc/validation evidence needed because no barrier is involved.
- **Fix shape**: mirror #3991/#917 — record the latch into a "pending" value during recording and promote it where `mark_frame_completed` runs (after the successful `queue_submit`), or reset both latches in the failure arms that already call `rollback_skin_frame_state`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other init/teardown sites)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_CONCURRENCY_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
