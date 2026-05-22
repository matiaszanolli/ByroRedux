# REN-D8-NEW-22: shrink_tlas_scratch_to_fit uses BLAS-scale slack — effectively dead code on TLAS scratch

## Summary

`shrink_tlas_scratch_to_fit` in [memory.rs:248-251](crates/renderer/src/vulkan/acceleration/memory.rs#L248-L251) calls the BLAS-tuned `scratch_should_shrink` predicate, which hard-codes a 16 MB slack margin. TLAS scratch buffers live at <1 MB scale, so the excess condition almost never trips — the entire shrink mechanism (#682 / MEM-2-7) is effectively dead code for the TLAS scratch path.

## Evidence

- [predicates.rs:262-265](crates/renderer/src/vulkan/acceleration/predicates.rs#L262-L265) — `scratch_should_shrink` hard-codes `BLAS_REBUILD_SLACK_BYTES = 16 MB`:
  ```rust
  pub(super) fn scratch_should_shrink(current_capacity: vk::DeviceSize, peak_required: vk::DeviceSize) -> bool {
      current_capacity > peak_required.saturating_mul(2)
          && current_capacity.saturating_sub(peak_required) > BLAS_REBUILD_SLACK_BYTES
  }
  ```
- [constants.rs:16](crates/renderer/src/vulkan/acceleration/constants.rs#L16) — `BLAS_REBUILD_SLACK_BYTES` doc-comment explicitly says "BLAS scratch lives at 80-200 MB scale, slack is ~10% of that"
- [predicates.rs:280-286](crates/renderer/src/vulkan/acceleration/predicates.rs#L280-L286) — the parallel `tlas_instance_should_shrink` already uses a 1 MB TLAS-calibrated slack:
  ```rust
  pub(super) fn tlas_instance_should_shrink(...) -> bool {
      current_capacity_bytes > working_set_bytes.saturating_mul(2)
          && current_capacity_bytes.saturating_sub(working_set_bytes) > TLAS_REBUILD_SLACK_BYTES
  }
  ```
  …but the **scratch** sibling was not migrated to a TLAS-scale slack when MEM-2-7 landed.

## Scale check

- BLAS scratch: 80-200 MB → 16 MB slack ≈ 10% ratio ✓
- TLAS instance buffer: 0-2 MB → 1 MB slack ≈ 50% ratio ✓ (the 2× ratio gate carries the bulk; slack is a sub-MB churn guard)
- **TLAS scratch: tens of KB to <1 MB at 8K instances** → 16 MB slack ✗ permanently disables shrink

## Impact

Wastes a few MB of `DEVICE_LOCAL` VRAM per FIF slot persistently after an exterior peak. Does not affect correctness or stability — purely a missed reclamation opportunity.

## Suggested fix

1. Introduce `TLAS_SCRATCH_SLACK_BYTES` in `constants.rs` — likely 256 KB given the typical TLAS scratch scale (~1024 instances → ~64 KB scratch).
2. Add a `tlas_scratch_should_shrink` predicate in `predicates.rs` that mirrors `tlas_instance_should_shrink`'s shape.
3. Swap the call in [memory.rs:249](crates/renderer/src/vulkan/acceleration/memory.rs#L249) from `scratch_should_shrink` to `tlas_scratch_should_shrink`.
4. Update the `BLAS_REBUILD_SLACK_BYTES` doc-comment in `constants.rs` to clarify "BLAS-only scale — do not reuse for TLAS scratch".
5. Unit test the new predicate alongside the existing `tlas_instance_should_shrink` test.

~30 LOC + unit test.

## Completeness Checks
- [ ] **UNSAFE**: No unsafe involved.
- [ ] **SIBLING**: Verify no other call site reuses `scratch_should_shrink` on a non-BLAS path. Grep `scratch_should_shrink` in `crates/renderer/`.
- [ ] **DROP**: No Vulkan-object lifecycle change; the existing destroy-then-recreate flow in `memory.rs` stays unchanged.
- [ ] **TESTS**: Add `tlas_scratch_should_shrink_fires_at_realistic_excess` unit test (256 KB peak vs 4 MB current → should shrink) alongside the existing predicates tests.

## Source

[`AUDIT_RENDERER_2026-05-21_DIM8.md`](docs/audits/AUDIT_RENDERER_2026-05-21_DIM8.md) FINDING-D8-22.
