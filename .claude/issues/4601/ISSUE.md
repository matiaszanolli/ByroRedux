# CONC-D1-2026-09-21-01: The all-slots fence wait is unpinned as to its argument, and two more of its riders are missing from the #870/#3643 list

**Labels**: medium, sync, renderer, vulkan, test-gap, bug

Filed via /audit-publish from docs/audits/AUDIT_CONCURRENCY_2026-09-21.md.

**Severity**: MEDIUM (same class and grade as #3643 and #4516) · **Dimension**: 1 — Vulkan Queue & AS Sync (frame-in-flight discipline)
**Location**:
- The wait: `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:65` — `wait_for_fences(&self.frame_sync.in_flight, true, u64::MAX)` in `sync_and_acquire_frame`
- The rider list: `crates/renderer/src/vulkan/sync.rs:23-99` (the #870/#3643 block), and its tripwire test `frames_in_flight_contract_names_every_dependent_resource` (`sync.rs:599`)
- Rider 1: `crates/renderer/src/vulkan/context/draw.rs:2383-2427` (post-present TLAS shrink of the *next* slot) → `crates/renderer/src/vulkan/acceleration/memory.rs:331,408` (`shrink_tlas_scratch_to_fit`'s immediate `old.destroy`)
- Rider 2: `crates/renderer/src/vulkan/groundcover.rs:1198-1300` (`GroundCoverPipeline::prepare`), reached via `VulkanContext::prepare_groundcover` (`context/resources.rs:713-727`) from `byroredux/src/app_frame.rs:369` (→ `:823`), before `draw_frame` at `:516`

**Status**: NEW
**Verified against**: HEAD `f97775ca8` (read at the symbols; line numbers as of HEAD).

## Description

`sync_and_acquire_frame`'s top-of-frame `wait_for_fences(&self.frame_sync.in_flight, true, u64::MAX)` is the only safety argument for every resource in the `sync.rs` #870/#3643 block (7 entries). Nothing pins that the wait covers the *whole* array:

- The #3442 pin, `temporal_history_indexing_uses_the_general_previous_slot_form` (`crates/renderer/src/shader_constants.rs:1127`), rejects only the `(f + 1) % MAX_FRAMES_IN_FLIGHT` spelling.
- The seven tests that `include_str!` `sync_and_acquire_frame.rs` (in `caustic.rs`, `skin_compute.rs`, `context/depth_capture.rs`, `shader_constants.rs`, `morph_compute.rs`, `context/resources.rs`, `context/build_and_upload_instances.rs`) check only presence, or the *position* of `.wait_for_fences(` relative to a later call. None asserts on the wait's argument.
- `context/helpers.rs:28-30` nonetheless says "That wait is itself pinned since `ac48ab63` (#3442)".
- So the textbook per-slot form, `&[self.frame_sync.in_flight[frame]]`, passes every test. It is also a plausible perf change: the all-slots wait means CPU recording of frame N never overlaps GPU execution of frame N-1. The throughput side is PERF-D5-2026-09-21-01 (`docs/audits/AUDIT_PERFORMANCE_2026-09-21.md`).

Two riders on that wait are not in the list:

1. **Post-present shrink of the *next* slot.** After `self.current_frame` advances (`draw.rs:2341`), `draw_frame` calls `shrink_tlas_to_fit` and then `shrink_tlas_scratch_to_fit` on the new `current_frame` slot (`draw.rs:2409`, `:2426`). `shrink_tlas_scratch_to_fit` destroys that slot's TLAS scratch immediately: `memory.rs:331` on the slot-`None` arm, and `:408` on the live arm once the replacement is allocated. The SAFETY text at `draw.rs:2383-2399` credits "the standard MAX_FRAMES_IN_FLIGHT alternation" and says the slot is "the one whose previous frame work signalled at the start of this frame". That is true only because the start-of-frame wait covered *both* slots. At N = 2 the next slot's last user is frame N-1, which a per-slot wait on frame N's own slot never waits for, so frame N-1's TLAS build can still be executing when the scratch is freed.
2. **Ground-cover `prepare`.** It runs from the app before `draw_frame`, on `self.current_frame` (`resources.rs:713-727`). It harvests `counter_readback[frame]`, writes six host-visible per-slot buffers (chunk, cell, species, species-table, field-state, disturber), and rewrites that slot's descriptor sets. Its documented contract, "Must be called after slot `frame`'s fence has been waited" (`groundcover.rs:1198`), holds only through the *previous* `draw_frame`'s all-slots wait. The upcoming frame's own wait has not run yet.

Neither rider is a MAX_FRAMES_IN_FLIGHT-bump hazard: both hold at any N while the wait stays all-slots. They are wait-*policy* hazards, which the `== 2` const-assert and the tripwire test cannot see.

## Evidence

- `grep -rn 'in_flight, true' crates/ byroredux/` returns only the production line (`sync_and_acquire_frame.rs:65`).
- The `sync.rs` block names seven riders: `blas_scratch_buffer`, `depth_capture_staging`, `terrain_tile_buffer`, `screenshot_staging` + `depth_capture_pending_readback`, `weight_buffer`, the HUD `texture_handles`, and `FrameSync::images_in_flight`. Neither the TLAS scratch/instance buffers nor any ground-cover buffer appears in that prose or in `frames_in_flight_contract_names_every_dependent_resource`'s table.

## Impact

None today. A one-line change to a per-slot wait stays green and silently breaks nine sites (the 7 listed plus these 2):
- immediate BLAS/TLAS scratch destroys under an in-flight build (use-after-free);
- host writes into in-use mapped buffers;
- `VUID-vkUpdateDescriptorSets-None-03047` on in-use descriptor sets.

**Trigger conditions**: any change of the top-of-frame wait to cover fewer than all `in_flight` fences, followed by a frame in which the next slot's TLAS scratch shrinks or ground cover is active.

**Verification path**: `cargo test` for the pin. The consequences would show only after such a change, as `BYRO_VALIDATION=1` use-after-free / descriptor-in-use VUIDs.

## Related

- #870 and #3643 (the rider list this extends), #3442 (made the wait all-slots), #4516 (the last rider added), #2929 (the TLAS shrink's deferred-destroy history).
- CONC-D2-2026-09-21-01 (#4602): the ground-cover readback that `prepare` harvests.
- PERF-D5-2026-09-21-01 (`docs/audits/AUDIT_PERFORMANCE_2026-09-21.md`): the throughput cost of this same wait. A move to a per-slot wait has to land everything below first.

## Suggested Fix

- Add a source-scan pin that the top-of-frame `wait_for_fences` passes the whole `in_flight` slice. Compose the needle at runtime so it cannot match the test's own literal (the #3442 technique).
- Add both riders to the `sync.rs` #870/#3643 block, to `frames_in_flight_contract_names_every_dependent_resource`'s table, and to the const-assert message's list.
- Reword `draw.rs:2383-2399` to cite the all-slots wait rather than "standard alternation". Once the pin exists, `helpers.rs:28-30`'s "pinned since `ac48ab63`" becomes true.

Source: docs/audits/AUDIT_CONCURRENCY_2026-09-21.md (CONC-D1-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every other site that writes, rewrites or destroys a non-per-FIF resource outside `draw_frame`'s own wait (pre-draw app hooks, post-present tails) checked against the list
- [ ] **TESTS**: the new pin fails when the wait's argument is changed to `&[self.frame_sync.in_flight[frame]]` (mutation-check, then revert)
