# #7373 — REN-D1-2026-11-23-01: an unsettled patch-BLAS stage is silently dropped at the next record instead of rolled back; nothing forbids a new post-record early exit

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D1-01). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (hardening; no reachable trigger at HEAD)
- **Dimension**: AS Correctness
- **Location**: `StagedPatchContent::begin_frame` (called from `AccelerationManager::record_patch_blas_frame`, `acceleration/blas_terrain_patch.rs`); settle sites `promote_skin_frame_state` / `rollback_skin_frame_state` (`context/skinned_blas_refit.rs`); `VulkanContext::draw_frame` (`context/draw.rs`).
- **Status**: NEW (hardening beside the closed #7360; not a regression: #7360's three tail-Err sites and the success path are verified correct, see Invariants)
- **Description**: #7360 stages the prior `entry.content` at record time and settles it only in `promote_skin_frame_state` (commit) or `rollback_skin_frame_state` (restore, clears `ready`). `begin_frame` then does `self.prior.clear()` on the next record, and the unit test `a_submitted_frame_forgets_its_stage_and_a_new_record_starts_empty` pins that ("an unsettled stage is not carried into the next record"). The doc comment says such a stage "cannot be undone any more", but it could be: `begin_frame` could `rollback` instead of `clear`. The sibling fix landed the same day for the same bug class, #7361 (`terrain_tiles_upload_staged`), does exactly that: `discard_terrain_tile_upload` at the top of the next `draw_frame` re-arms the flag. So the two #7360/#7361 settle designs disagree on what an unsettled frame means.
- **Evidence**: In `draw_frame` between `record_terrain_patches` and the three tail `if let Some(e) = {end_command_buffer,reset_fences,submit}_failed { self.rollback_skin_frame_state(frame); return Err(e); }` blocks there is no `?`, `return` or `.expect` (checked by scanning that span: only the three rollback `return Err` sites and the graphics-queue `expect("graphics queue lock poisoned")`, which is a panic, not an early return). `build_and_upload_instances`, `record_geometry_pass`, `dispatch_skin_and_cluster` and the post passes all return `()`. So today every exit after the record reaches commit or rollback. `the_patch_blas_content_is_settled_with_the_skin_state_after_submit` only checks that the two hook calls exist, not that no other exit sits between record and tail.
- **Impact**: None today. A future `?` / early `return` added between `record_terrain_patches` and the tail would leave `entry.content = Some(..)` for a BUILD that never executed; the next frame plans an UPDATE refit of a never-built AS (VUID-03667 class, the skinned set's #3991), and `begin_frame` would then erase the only record of the prior value. The existing guard would stay green.
- **Related**: #7360 (closed), #7361 (closed, sibling design), #3991.
- **Suggested Fix**: Make `begin_frame` hand back an unsettled stage (restore each entry's prior content and clear `ready`) rather than clear it, or add a source-scan guard that `draw_frame_body()` has no `return`/`?` between `record_terrain_patches(` and `promote_skin_frame_state(` other than the three pinned sites. Update `a_submitted_frame_forgets_its_stage_and_a_new_record_starts_empty`, which currently asserts the drop.
- **Trigger Conditions**: a new fallible call (or early `return`) inserted after `self.record_terrain_patches(cmd, frame)` and before the three tail latches, on a frame that records a first-time patch BUILD (cell entry, camera cut).
- **Verification Path**: unit-level only; the fault-injection route in the "Needs-RenderDoc" section (force a submit failure) covers the existing three sites, not a hypothetical fourth.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop / teardown ordering is still correct
- [ ] **TESTS**: A regression test pins this specific fix
