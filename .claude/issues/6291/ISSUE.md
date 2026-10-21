# #6291: CONC-2026-10-21-03: Interaction-field ping-pong committed at record time

**Labels**: low,sync,renderer,terrain-exterior,bug

**Source**: docs/audits/AUDIT_CONCURRENCY_2026-10-21.md · **Severity**: LOW · **Dimension**: Compute → AS → Fragment Chains (Dim 2), cross-frame ping-pong · **Location**: `crates/renderer/src/vulkan/groundcover/frame.rs:317-323`

- **Location**: `crates/renderer/src/vulkan/groundcover/frame.rs:317-323` (`if self.record_interaction(...) { commit_field_state(...) }`), `:776-783` (`commit_field_state`); tail `Err` sites in `crates/renderer/src/vulkan/context/draw.rs:512-624` (`end_command_buffer_failed`, `reset_fences_failed`, `submit_failed`); field buffer `crates/renderer/src/vulkan/groundcover/construct.rs:117` (`create_device_local_uninit`, never filled)
- **Status**: NEW, residual of #6031 (closed by 21b9743e0 today). #6031 moved the flip from `prepare` into `record_scatter`, which fixes the no-TLAS and upload-failure skips. But record time is still before the three tail `Err` sites that discard the command buffer.
- **Description**: `commit_field_state` flips `field_write_half` and sets `field_previous = Some(origin)` as soon as the dispatch is recorded. If `end_command_buffer`, `reset_fences` or `queue_submit` then fails, the recording is discarded but the host state says the half was written. The next frame then reprojects from a half the GPU never wrote this time: the field from two frames back, or uninitialised memory on the first frame. `previous.w = 1.0` marks it valid. The renderer already has the right shape for this class: the skin chain (#3991, `promote_skin_frame_state` / `rollback_skin_frame_state`), the volumetrics and caustic skip-clear latches (#5956, `promote_skip_clear_latches`), and SVGF / TAA / volumetrics `mark_frame_completed` (#917) are all promoted only after a successful submit. The ground-cover field is the one cross-frame ping-pong outside that set. As #6031 itself notes, `groundcover_interaction.comp` does not sanitise a NaN it reads, so an uninitialised half can persist until the camera leaves the field extent.
- **Trigger Conditions**: a tail `Err` (device lost excluded: OOM in `end_command_buffer`, a failed `reset_fences`, or a failed `queue_submit`) on a frame where ground cover recorded its interaction dispatch. It is worst on the first such frame of a session.
- **Verification Path**: `cargo test` (a source-shape pin like `skip_clear_latches_are_promoted_after_submit_not_at_record_time`); no GPU capture needed.
- **Impact**: low-probability visual corruption of grass displacement. It is not a Vulkan hazard: no barrier or layout is involved.
- **Related**: #6031 (closed), #5956 (closed), #3991 (closed).
- **Suggested Fix**: stage the committed state in a `pending_field_state: Option<…>` at record time and promote it next to `promote_skip_clear_latches(frame)` after the submit succeeds. Discard it in `discard_pending_skip_clear_latches`'s position at the top of `draw_frame`. Alternatively, zero both halves once at creation with a fenced one-time `cmd_fill_buffer` (the buffer already has `TRANSFER_DST`), which removes the uninitialised case outright.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
