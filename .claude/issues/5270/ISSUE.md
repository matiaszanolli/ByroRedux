# #5270: SAFE-D5-2026-10-05-01: `with_one_time_commands_inner` frees the command buffer and destroys/releases the fence on the `MaybeInFlight` wait-failure arm, though its own error says the commands may still be pending

**Labels**: low,safety,renderer,vulkan,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5270

**Source**: `docs/audits/AUDIT_SAFETY_2026-10-05.md` — `SAFE-D5-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: LOW.
  - The decision tree's "Vulkan spec violation → at least HIGH" floor was weighed and not applied. The violation is reachable only when `vkWaitForFences` with an infinite timeout fails with `VK_ERROR_OUT_OF_HOST_MEMORY` or `VK_ERROR_OUT_OF_DEVICE_MEMORY`. After a true `VK_ERROR_DEVICE_LOST`, destroying and freeing these objects is valid per the spec's "Lost Device" section.
  - #5201 rated this exact window LOW for the AS objects the same command buffer writes. This finding is that issue's sibling inside the helper itself.
- **Dimension**: Vulkan Spec Compliance
- **Location**: `crates/renderer/src/vulkan/texture.rs:931-942`, the `wait_for_fences` error arm of `with_one_time_commands_inner`. The class contract is at `:671-674`, and the reusable-fence guard at `:869`.
- **Status**: NEW.
  - #5201 (CLOSED, `65b0217f3`) made `build_blas_batched` consult `OneTimeCommandError::may_be_in_flight` before unwinding its ASes.
  - #4891 (CLOSED, `9cc77cec0`) introduced the `NotSubmitted` / `MaybeInFlight` split for the callers.
  - Neither touched the helper's own `cmd` / fence disposal, which dates from #1861. No issue or audit names it (searched `OneTimeCommandError free_command_buffers` and `MaybeInFlight fence`).
- **Description**:
  - #4891 defines `MaybeInFlight` as "`vkQueueSubmit` or the fence wait failed… the commands may be pending, so a host-side destroy could race an in-flight transfer: the caller must keep what they reference alive". Every caller now honours that:
    - `buffer.rs:914/1528/1634` and `texture_registry/upload.rs` leak their staging and destinations;
    - `blas_static.rs` calls `mem::forget` on `prepared` and `compact_accels`.
  - The helper that returns the error does the opposite with the objects it owns. On the fence-wait failure it:
    - calls `free_command_buffers(pool, &[cmd])`, which is invalid while the buffer is pending (`VUID-vkFreeCommandBuffers-pCommandBuffers-00047`);
    - destroys the fence it created (`VUID-vkDestroyFence-fence-01120`);
    - or, on the reusable-fence path, drops the guard, so the *next* caller's `reset_fences` hits a fence possibly still tied to pending work (`VUID-vkResetFences-pFences-01123`).
  - The `vkQueueSubmit`-failure arm (`:920-929`) is spec-fine. A failed submit leaves the command buffer not pending, except on device loss. Only the wait arm contradicts its own class.
- **Evidence**:
  ```rust
  if let Err(e) = device.wait_for_fences(&[fence], true, u64::MAX) {
      if owned {
          device.destroy_fence(fence, None);
      }
      drop(fence_guard);
      device.free_command_buffers(pool, &[cmd]);
      return Err(OneTimeCommandError::maybe_in_flight(
          e,
          "wait for one-time commands",
      ));
  }
  ```
- **Impact**: None in normal operation. On an OOM-from-wait (the renderer is already failing), the helper frees or reuses objects the GPU may still be executing. That is undefined behaviour of the same class #5201 closed one layer up, and it leaves the "one helper, one failure contract" policy inconsistent.
- **Related**: #5201, #4891, #1861, #1713. Owner overlap: `/audit-renderer` Dim 1 raised #5201; `/audit-concurrency` covers fence discipline.
- **Suggested Fix**: On the wait-failure arm, branch on the `vk::Result`:
  - `ERROR_DEVICE_LOST` keeps today's free and destroy;
  - any other code leaks `cmd` and the owned fence, and poisons or marks the reusable fence so it is recreated rather than reset.

  Pin the arm with the existing `one_time_failure_class_tests` needle scan. Per the *Speculative Vulkan Fixes* rule, confirm the classification under `BYRO_VALIDATION=1` with fault injection before landing.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
