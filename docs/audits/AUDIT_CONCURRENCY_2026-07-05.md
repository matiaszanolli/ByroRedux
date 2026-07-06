# Concurrency & Synchronization Audit — 2026-07-05

**Scope**: Dimensions **1** and **2** only (RT-focused sweep).
- **Dimension 1** — Vulkan Queue & Acceleration-Structure Sync (CRITICAL surface)
- **Dimension 2** — Compute → AS → Fragment Chains

**Depth**: deep (traced concurrent paths + timing windows).
**Method**: This dimension pair was audited four times in the preceding week
(`AUDIT_CONCURRENCY_2026-07-01/-02/-03.md`), which closed the two CRITICAL
findings (`#1782` scratch UAF, `#1790` skinned-BLAS scratch barrier). This
sweep therefore focused on (a) re-verifying those fixes are still in place, and
(b) auditing the **two D1/D2-relevant commits that landed after the 07-03
audit** (`#1811`, `#1812`) for regressions, plus a fresh trace of the core
build-input barrier chain.

Per the standing speculative-fix guardrail, no barrier/stage/layout change is
proposed on reasoning alone; every claim below is a **traced concrete state**,
not a hypothesis.

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 0 |

**No new findings.** Dimensions 1 and 2 are in a clean state. The post-07-03
skin-chain optimisation commits were verified correct, all prior CRITICAL fixes
remain in place, and the only D1/D2-touching defects still live are already
filed (`#1861`, `#1874`). Details of what was verified are below so the next
sweep does not re-walk the same ground.

## Post-07-03 commits verified (Dimension 2 — skin → AS chain)

### `#1811` (a60a0153) — "skip skinning GPU refresh on fully-clean frames": CORRECT
- **What it does**: Adds `VulkanContext::clean_skin_frames`, a streak counter
  reset to 0 by any dirty signal (`!pose_dirty.is_empty() || !bind_inverse_pending_uploads.is_empty()`)
  and incremented on clean frames (`next_clean_skin_frames`, `draw.rs`). Once
  `should_skip_skin_gpu_refresh` is true (streak `>= MAX_FRAMES_IN_FLIGHT + 1`,
  i.e. `>= 3`), it skips the `bone_world` staging upload + device copy
  (`draw.rs:2746`) **and** the `skin_palette.comp` full-range dispatch
  (`draw.rs:2808`).
- **Why it is safe (traced)**:
  1. The `bone_world` and palette buffers are **per-frame-in-flight** rings
     (`bone_world_buffers()[frame]` / `bone_buffers()[frame]`, `scene_buffer/buffers.rs`).
     With `MAX_FRAMES_IN_FLIGHT == 2`, a pose that goes quiet at streak 0 still
     refreshes at streaks 0, 1, 2 — which covers frame slots `{N%2, (N+1)%2, N%2}`,
     i.e. **both** FIF slots receive the current content before the streak-3
     skip engages. The `refresh_is_not_skipped_within_max_frames_in_flight_of_a_dirty_frame`
     unit test pins exactly this. Skipping at streak ≥3 rewrites only
     byte-identical data.
  2. The palette-publish barrier (`COMPUTE_SHADER_WRITE → SHADER_READ`,
     `draw.rs:2830`) lives **inside** the same `bone_count > 0 && !skip_skin_gpu_refresh`
     block, so a skipped dispatch also skips its now-unneeded barrier — no
     dangling publish, no barrier for a write that never happened.
  3. First-sight entities arrive via `bind_inverse_pending_uploads`, which forces
     `skin_state_dirty == true` and resets the streak — so a streaming spawn can
     never be swallowed by the skip. The `bind_inverses` drain itself
     (`draw.rs:2761`) is **not** gated by the skip, but it is a no-op whenever
     the pending list is empty, and a non-empty list already implies the palette
     recompute runs.
  4. The skip reuses the **same** `pose_dirty` signal that already gates the
     per-entity `skin_vertices.comp` dispatch and the BLAS refit — it adds no new
     correctness dependency that could diverge from the pre-existing gate.

### `#1812` (ad350331) — "skip the redundant post-BUILD BLAS refit on first-sight entities": CORRECT
- **What it does**: Adds `skin_built_this_frame_scratch` (a `HashSet<EntityId>`
  scratch), populated **only** from the `Ok(())` arm of the first-sight build
  loop (`draw.rs:1819`), and gates the refit loop (`draw.rs`
  `if built_this_frame.contains(&entity_id) || (…)`) so a freshly-BUILT BLAS
  skips the immediately-adjacent UPDATE refit against the same vertex data.
- **Why it is safe (traced)**: A first-sight BUILD produces a complete BLAS from
  the exact vertex output a same-`cmd` UPDATE would re-read; the removed refit was
  pure wasted work (plus inflated `refits_attempted` telemetry), **not** a
  correctness step. A **failed** build is deliberately *not* inserted into the set
  (the `insert` is inside the `Ok(())` arm — pinned by
  `skin_built_this_frame_skip_tests`), so a build failure still falls through to
  the `accel.has_skinned_blas`-gated path unchanged. This change also retires the
  exact BUILD-then-UPDATE same-command-buffer adjacency that `#1790`'s
  `record_scratch_serialize_barrier` (WRITE → WRITE|READ) was hardened to cover;
  that barrier remains correct and necessary for the general dirty-refit case, so
  leaving it untouched (as the commit did) is right.

## Prior CRITICAL / HIGH fixes re-verified in place

- **`#1782` — deferred BLAS-scratch destruction**: The mid-frame grow/shrink
  scratch retirement routes through `pending_destroy_scratch`; the one immediate
  `shrink_blas_scratch_to_fit` call is in `recreate_swapchain_core` after
  `device_wait_idle`. Confirmed still present (07-03 confirmation stands).
- **`#1790` — skinned-BLAS scratch serialize barrier**: `record_scratch_serialize_barrier`
  (`blas_skinned.rs:654-669`) dst mask is `ACCELERATION_STRUCTURE_WRITE_KHR | ACCELERATION_STRUCTURE_READ_KHR`
  at `ACCELERATION_STRUCTURE_BUILD_KHR` — the READ bit that covers a same-`cmd`
  BUILD-then-UPDATE reading `srcAccelerationStructure`. In place.
- **`#507945d8` — AS build-INPUT barrier access flag**: Both build-input barriers
  carry `SHADER_READ` at `ACCELERATION_STRUCTURE_BUILD_KHR` (the spec's
  build-input access), **not** `ACCELERATION_STRUCTURE_READ_KHR`:
  - skinned-vertex compute write → BLAS build: `draw.rs:1806-1812`
    (`COMPUTE_SHADER / SHADER_WRITE → ACCELERATION_STRUCTURE_BUILD_KHR / SHADER_READ`).
  - TLAS instance-buffer `TRANSFER_WRITE` → build: `tlas.rs:739-747`
    (`TRANSFER / TRANSFER_WRITE → ACCELERATION_STRUCTURE_BUILD_KHR / SHADER_READ`).
- **Queue submission single-Mutex discipline (`#1713`)**: `queue_submit`
  (`draw.rs:3803-3809`) binds the `graphics_queue` `MutexGuard` to a `let` and
  derefs `*queue` **inside** the call so the guard is held across the submit
  (VUID-vkQueueSubmit-queue-00893), with an explicit comment against the
  `*self.graphics_queue.lock()` early-release trap. `queue_present`
  (`draw.rs:3862-3869`) mirrors it. Fences waited before cmd/resource reuse
  (`draw.rs:2149`, `3775` reset-immediately-before-submit). Correct.
- **Volumetrics `write_tlas` → `dispatch` latch (`#1105`)**: `tlas_written:
  [bool; MAX_FRAMES_IN_FLIGHT]` (`volumetrics.rs:220`) — `dispatch`
  `debug_assert!`s `tlas_written[frame]` then resets it to `false`
  (`volumetrics.rs:806-812`); `write_tlas` sets it. Set/reset symmetry intact.

## D1/D2 defects that remain live — already filed (not re-reported)

- **`#1861` [OPEN, LOW]** — `with_one_time_commands_inner` leaks the command
  buffer (and, on the owned-fence path, the fence) on three post-recording
  `?`-propagated error paths (`reset_fences` / submit / wait). A load-time
  one-shot path, not per-frame — no compounding leak. Owned by the renderer
  audit; noted here for D1 completeness.
- **`#1874` [OPEN, HIGH]** — Ghosted diagonal double-image in TES interiors,
  sticks after the camera parks; mechanism narrowed to a shared bad motion
  vector + TAA parked-camera clamp bypass (SVGF/TAA reprojection — squarely
  Dimension 2), but origin unconfirmed. Explicitly a **RenderDoc-not-speculation**
  investigation per the standing guardrail; not actionable as a barrier/stage
  change from static reading. Left filed, not duplicated.

## Dimension summaries

### Dimension 1 — Vulkan Queue & Acceleration-Structure Sync
Clean. Queue Mutex held-across-submit discipline correct; fence-in-flight and
acquire→submit→present chains verified; AS build→read and build-INPUT barriers
present with correct access flags; deferred scratch + BLAS destruction still
routed through the pending-destroy queues. No new hazard.

### Dimension 2 — Compute → AS → Fragment Chains
Clean. The palette → skin → BLAS-refit → ray-query chain is intact; the two
post-07-03 skin-chain optimisations (`#1811`, `#1812`) reuse the existing
`pose_dirty` gate and per-FIF ring indexing without introducing a stale-geometry
window. Cross-frame ping-pong latches (volumetrics `tlas_written`, and by prior
confirmation SVGF/TAA/caustic per-FIF slots) index the previous frame's slot.
The only open D2 defect (`#1874` ghosting) is a filed, RenderDoc-gated
investigation.

## Report Finalization

- No new GitHub issues warranted from this sweep.
- The two open D1/D2 items (`#1861`, `#1874`) are already tracked.
- `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-07-05.md` — will be a no-op
  (zero findings); run only to confirm dedup against the existing issues.
