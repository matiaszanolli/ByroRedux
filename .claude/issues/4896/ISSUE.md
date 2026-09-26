# REN-D5-2026-09-26-18: Nothing pins that `AllocatorResource` is removed from the `World` before `VulkanContext` drops

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4896

**Labels**: low,renderer,test-gap,memory,bug

- **Severity**: LOW (test gap; the code is correct today)
- **Dimension**: Memory/Lifecycle
- **Location**: `byroredux/src/main.rs` (`impl Drop for App`: `remove_resource::<AllocatorResource>()` then `self.renderer.take()`), `byroredux/src/app_events.rs` (`App::shutdown`)
- **Status**: NEW
- **Description / Evidence**: I confirmed by reading that `shutdown` removes the resource, drains streamed cells, flushes pending destroys and then takes the renderer, and that `Drop for App` repeats it idempotently, so the panic-unwind and non-`CloseRequested` exits are covered. The only protection is the comment "INVARIANT (REG-08 / #1640, #1477)". No test scans either site. Reversing the two lines re-arms the #665 leak-guard branch (device, surface and instance leaked; a `debug_assert!` panic in debug).
- **Suggested Fix**: A source-scan test (runtime-composed needles over production text) asserting the `AllocatorResource` removal precedes `renderer.take()` in both `Drop for App` and `shutdown`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
