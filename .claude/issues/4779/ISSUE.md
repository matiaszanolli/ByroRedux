# #4779: CONC-D1-2026-09-23-01: After a TLAS build fails, the volumetrics inject keeps tracing the stale per-slot TLAS past the BLAS deferred-destroy window

**Severity**: CRITICAL
**Labels**: critical, concurrency, sync, renderer, vulkan, bug
**Source**: docs/audits/AUDIT_CONCURRENCY_2026-09-23.md (CONC-D1-2026-09-23-01)

- **Severity**: CRITICAL. The impact is a use-after-free of an acceleration structure: a ray query dereferences a freed BLAS, leading to a GPU page fault or `VK_ERROR_DEVICE_LOST`. The trigger is failure-path only. The rating follows the precedent of #2673, which was rated CRITICAL for the same VRAM-pressure regime.
- **Dimension**: Vulkan Queue & AS Sync
- **Location**:
  - `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs:324-361` (failure arm) and `:383-392` (this comment states that volumetrics still ray-queries).
  - `crates/renderer/src/vulkan/context/post_passes.rs:575-578` and `:608-621` (the gate is `tlas_handle(frame)` only).
  - `crates/renderer/src/vulkan/acceleration/tlas.rs:807-840` (allocate-then-swap keeps the old TLAS), `:57`, `:468` and `:548` (the LRU clock and the working set).
  - `crates/renderer/src/vulkan/acceleration/blas_static.rs:66` (`drop_blas`), `:314` and `:413` (the batched build bumps the clock and evicts), `:1252-1318` (`evict_unused_blas`).
  - `crates/renderer/src/vulkan/acceleration/static_working_set.rs:25-27`.
  - `crates/renderer/src/deferred_destroy.rs:46`.
- **Status**: NEW. This is a residual of the #2673 design (keep the old AS alive when a build fails) combined with #2931 (publish the barrier on both arms). Neither issue covers BLAS lifetime under a reused stale TLAS. #1449 fixed only the immediate-destroy case.
- **Description**: Since #2673, `ensure_tlas_state` is allocate-then-swap. When `build_tlas` fails, `tlas[frame]` keeps the TLAS from this slot's last *successful* build, and `tlas_handle(frame)` stays `Some`. The failure arm makes the main pass safe by clearing `rt_flag`. `caustic_splat.comp` is also safe: it exits early on the same `sceneFlags.x` (`caustic_splat.comp:316`). `volumetrics_inject.comp` has no such gate. `record_volumetrics_pass` checks only `accel.tlas_handle(frame)`, so every failure frame binds the stale TLAS and traces shadow rays through it. The code comment at `dispatch_skin_and_cluster.rs:383-392` says so explicitly.

  The BLAS lifetime contract does not account for a TLAS that old:
  - Eviction protects only the current frame's draw set. `build_tlas_instances` rebuilds `static_working_set` from this frame's draws (`tlas.rs:468`, `:548`), and this happens even on a frame whose build later fails. Beyond that it protects only by LRU age: `MIN_IDLE_FRAMES = MAX_FRAMES_IN_FLIGHT + 1` ticks of `frame_counter`.
  - `frame_counter` also advances on failed builds (`tlas.rs:57`), and once per streaming `build_blas_batched` call (`blas_static.rs:314`), which also evicts (`:413`).
  - Evicted BLAS and BLAS dropped at cell unload (`drop_blas`, `:66`) are freed after `DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT` ticks.
  - The countdown only guarantees that command buffers which *built* a TLAS referencing the BLAS have retired. A stale TLAS that is re-bound on a later failure frame re-introduces those references into a new command buffer.
- **Evidence**: In `record_volumetrics_pass`, the only TLAS gate is:
  ```rust
  let vol_tlas = self.accel_manager.as_ref().and_then(|accel| accel.tlas_handle(frame));
  ```
  It has no `tlas_build_succeeded_last_frame` or `rt_flag` term, although `tlas_build_succeeded_last_frame` is set to `false` at `dispatch_skin_and_cluster.rs:315` and to `true` only at `:409`.
- **Trigger Conditions** (FIF = 2):
  1. Slot *a* last built successfully at frame A. BLAS *X* is in that TLAS but is not drawn after A. For example, it belongs to a cell that is being streamed out, or it was culled.
  2. Slot *a*'s build then fails persistently. Plausible causes are TLAS growth failing under VRAM pressure, the exact regime #2673 describes, or a failing per-slot shrink/regrow. It keeps failing at A+2 and A+4.
  3. Meanwhile, streaming's `build_blas_batched` (which bumps the clock and evicts), or a cell-unload `drop_blas`, queues *X* for deferred destruction at about A+1.
  4. *X* is freed at the tick of about A+3.
  5. At A+4, slot *a*'s volumetrics dispatch traces the stale TLAS, which still references *X*.

  With no streaming, eviction by the other slot's successful build needs about three consecutive failures on slot *a*. BLAS eviction and TLAS allocation failure are both symptoms of VRAM exhaustion, so the two conditions tend to occur together.
- **Impact**: Device loss or a GPU page fault during a cell transition under VRAM pressure. At best, garbage shadow visibility in the fog. Only the volumetrics inject is exposed, because it is the one ray-query consumer without an RT-valid gate.
- **Verification Path**: Not reachable by `cargo test`. To reproduce:
  1. Add a temporary fault gate that makes `ensure_tlas_state` fail on one slot for about 10 frames.
  2. Set a tiny BLAS budget so that eviction fires.
  3. Stream a cell boundary under `BYRO_VALIDATION=gpuav`.

  The expected signal is a GPU-AV invalid acceleration-structure or device-address report, or `VK_ERROR_DEVICE_LOST`.
- **Related**: #2673 and #2931 (both closed; their fixes are the premise of this finding), #1449 (closed).
- **Suggested Fix**: Treat a failed build as "no TLAS" for the volumetrics dispatch. Gate `vol_tlas` on `self.tlas_build_succeeded_last_frame`, so the existing `ran = false` → neutral-clear path takes over, just as `rt_flag = 0` does for the fragment consumers.

  An alternative is to pin, per slot, the BLAS entries that the live TLAS still references. That means excluding `last_blas_addresses` from eviction and delaying `drop_blas` until that slot rebuilds successfully. The gate is smaller and matches the existing caustic and fragment policy.

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
