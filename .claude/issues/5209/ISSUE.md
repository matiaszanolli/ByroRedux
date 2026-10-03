# #5209 — REN-D5-2026-10-03-05: `VulkanContext::drop` still `expect`s the transfer-fence lock — the #4599 poison policy covers allocator locks only

**Labels**: low,renderer,safety,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `impl Drop for VulkanContext` (`crates/renderer/src/vulkan/context/teardown.rs`): `.lock().expect("transfer fence lock poisoned")`. The lock holder is `with_one_time_commands_inner` (`crates/renderer/src/vulkan/texture.rs`), `fence_guard`.
- **Status**: NEW, a sibling of CLOSED #4599, whose scope was allocator locks: its issue body lists only allocator sites.
- **Description**:
  - #4599 established that a poisoned lock on a teardown path must be recovered, because a panic there skips `save_pipeline_cache`, `destroy_device` and `destroy_instance`, or aborts during unwind. It routed allocator locks through `lock_recovering` / `into_inner_recovering`.
  - `VulkanContext::drop` still acquires `self.transfer_fence` with `.expect`.
  - That mutex is held as `fence_guard` across reset, submit and wait in `with_one_time_commands_inner`. The one panicking call inside that window is `queue.lock().expect("graphics queue lock poisoned")`.
  - The pin `allocator_lock_direct_acquires_are_confined_to_allocation_and_reports` counts only `.expect("allocator lock poisoned")` / `.lock().unwrap()` spellings, so it cannot see this site.
- **Evidence**:
  ```rust
  let fence = *self.transfer_fence.lock().expect("transfer fence lock poisoned");
  self.device.destroy_fence(fence, None);
  ```
- **Impact**: Reaching it needs a chain: an earlier panic while the graphics-queue lock is held poisons that lock; a one-time submit then panics while holding the fence; the context then drops. This is hardening only, with no realistic trigger at HEAD, but it is the exact failure mode #4599 named.
- **Suggested Fix**: Use `allocator::lock_recovering(&self.transfer_fence)` in Drop. For consistency, also use it for the queue lock inside the fence window. Extend the #4599 allowlist scan to the `expect("… lock poisoned")` family within `teardown.rs`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix
