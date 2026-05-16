# #1123 — REN-D8-NEW-02: built_primitive_count invariant lacks a pinning unit test

**Severity:** LOW (test-only — pins implicit invariant from REN-D8-NEW-01 / #1121)
**Domain:** renderer (RT acceleration structures)
**Status:** OPEN at HEAD `1608e6a2`

## Summary
`built_primitive_count == last_blas_addresses.len()` is the implicit invariant the
TLAS UPDATE path depends on (post-#1121 / a49eb945 it's pinned at runtime via
`debug_assert_eq!` at `tlas.rs:753`). No unit test pins it from outside production
code. The proposed fix is a paired unit test that drives BUILD → UPDATE → BUILD
transitions and asserts the invariant after each frame's bookkeeping.

## Plan
- Pure simulation only — every existing test in `acceleration/tests.rs` operates
  on free helpers without a live Vulkan context, follow the same pattern.
- Mirror the production state machine: hold `built_primitive_count: u32` and
  `last_blas_addresses: Vec<u64>` in the test, then call `decide_use_update`
  + apply the `instance_count > built_primitive_count` guard + perform the
  same swap.
- Drive: BUILD → UPDATE (same count) → BUILD (count grows) → UPDATE → empty
  frame (BUILD) → non-empty (BUILD) → UPDATE. Assert the invariant after each.

## Pair
- REN-D8-NEW-01 (#1121, closed in a49eb945) — the runtime assert.
