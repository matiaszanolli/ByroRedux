# #5201 — REN-D1-2026-10-03-02: `build_blas_batched` destroys prepared and compacted BLAS on a `MaybeInFlight` one-time-submit failure, against #4891's policy

**Labels**: low,renderer,vulkan,memory,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW. This is defence in depth, at the same severity #4891 itself was filed. The hazard is reachable only when `vkWaitForFences` with infinite timeout fails with an OOM, or when `vkQueueSubmit` fails. After a true `VK_ERROR_DEVICE_LOST`, destroying child objects is valid per spec §"Lost Device".
- **Dimension**: AS Correctness (Memory/Lifecycle)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs`, `AccelerationManager::build_blas_batched`:
  - the `if let Err(e) = build_result` arm, which calls `unwind_prepared(.., Some(query_pool))`;
  - the `if let Err(e) = copy_result` arm, which destroys both the `prepared` originals and the `compact_accels`.
- **Status**: NEW. It is a sibling gap of #4891 (closed, `9cc77cec0`). #4891 scoped the policy to "the upload orchestrators", and #4883 (`628913593`), landed the same day, re-documented the AS arm with the pre-#4891 premise.
- **Description**: #4891 split one-time-submit failures into `OneTimeCommandError::NotSubmitted` and `MaybeInFlight`.
  - `MaybeInFlight` means `queue_submit` or the fence wait failed, so "the commands may be pending".
  - The documented rule is "destroy when `may_be_in_flight` is false, leak when it is true".
  - `buffer.rs` and `texture_registry/upload.rs` follow that rule. Both BLAS one-time submissions go through the same helper (`submit_one_time` → `with_one_time_commands*`) but never consult it.
  - Their SAFETY comments state a premise the typed error now contradicts:
    - "the build submission failed, so no in-flight command buffer references `prepared` or `query_pool`"
    - "the copy submission failed, so no in-flight command buffer references `p.accel`"
  - On a fence-wait failure the build or compaction-copy command buffer may still be executing. The arms then destroy every AS it writes, plus its query pool. This is the "freeing an AS that in-flight work still reads" class, gated behind an already-failing device.
- **Evidence**: `grep -rn may_be_in_flight crates/renderer/src` finds hits only in `buffer.rs`, `texture_registry/upload.rs` and `texture.rs`, and none in `acceleration/`. `with_one_time_commands_inner` returns `OneTimeCommandError::maybe_in_flight(e, "wait for one-time commands")` from the `wait_for_fences` error arm.
- **Impact**: The whole renderer is degraded by the time this fires. The risk is a host-side destroy racing a still-pending AS build or copy on an OOM-from-wait path, which is undefined behaviour per spec, rather than a clean leak. It is also a policy inconsistency: the same helper has two failure contracts depending on the caller.
- **Related**: #4891, #4883, #1097, #2926.
- **Suggested Fix**: In both arms, branch on `OneTimeCommandError::may_be_in_flight(&e)`. Unwind on `NotSubmitted`. On `MaybeInFlight`, `mem::forget` (leak) the AS handles and buffers, as the upload orchestrators do. Correct the two SAFETY comments. A source pin like `one_time_failure_class_tests`' needle scan can cover `blas_static.rs`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
