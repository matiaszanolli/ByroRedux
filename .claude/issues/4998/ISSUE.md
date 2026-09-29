# #4998: CONC-D6-2026-09-28-01: `GpuPerFrameTimers::new` leaks the earlier TIMESTAMP query pools when a later slot's `create_query_pool` fails, and the caller treats the error as non-fatal

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,renderer,vulkan,memory,bug
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:629-651` (TIMESTAMP pool loop); caller `crates/renderer/src/vulkan/context/init.rs:658-667`
- **Status**: NEW. The code dates from e5774b19c (#1194, 2026-05-21), but no issue title covers it. #1483 and #1478 are about the allocator-None drop path and the hostQueryReset gate.
- **Description**: The TIMESTAMP loop fills `pools[i]` and exits through `.with_context(...)?` if a slot fails. When slot 1 fails, slot 0's `VkQueryPool` is already created but only exists in a local array, so no `Self` exists to destroy it. `init.rs:658-667` maps that `Err` to `None` and logs a warning. The engine then runs the whole session with a pool that nothing will ever destroy, and it is still alive at `vkDestroyDevice`. The fragment-invocation branch added in this window (`gpu_timers.rs:652-690`) handles the same partial failure correctly: it destroys every non-null candidate at `:676-683`. The two branches in the same function now use different cleanup policies.
- **Evidence**: `*slot = unsafe { device.create_query_pool(&info, None).with_context(|| format!("create TIMESTAMP query pool slot {i}"))? };` (`:636-640`). There is no cleanup before the `?`. Compare `for pool in candidate { if pool != vk::QueryPool::null() { unsafe { device.destroy_query_pool(pool, None) }; } }` (`:677-683`).
- **Trigger Conditions**: `vkCreateQueryPool` must return an error (OOM) for slot ≥ 1 after slot 0 succeeded. That is very unlikely on a desktop driver.
- **Impact**: One small leaked `VkQueryPool` (56 queries), and a validation error at shutdown. There is no memory-safety hazard.
- **Verification Path**: Inject a failure on slot 1 (for example, a test-only hook), run with `BYRO_VALIDATION=1`, and expect `VUID-vkDestroyDevice-device-05137` naming a `VkQueryPool`.
- **Related**: #1483, #1478, #4891 (inconsistent failure-path policy across subsystems).
- **Suggested Fix**: Before propagating the error, destroy every non-null entry in `pools`, using the same shape as the fragment-invocation branch. Alternatively, build `Self` first and let `destroy()` handle the partial state.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D6-2026-09-28-01) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in the other one-time/partial texture paths (texture_registry, dynamic_rgba, overlay uploads)
- [ ] **DROP**: If Vulkan objects change, teardown ordering is still correct (see the three load-bearing orderings in `context/teardown.rs`)
- [ ] **TESTS**: A regression test (or a recorded `BYRO_VALIDATION=1` capture) pins this specific fix
