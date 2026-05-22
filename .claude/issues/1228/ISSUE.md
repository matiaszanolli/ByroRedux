# REN-D8-NEW-24: missing_blas counter conflates transient skinned vs persistent rigid causes

## Summary

`tlas.rs::build_tlas` increments a single `missing_blas` counter when a draw command references an entity whose BLAS hasn't been built yet. The sample text differentiates skinned ("skinned entity EntityId(...) (no BLAS)") from rigid, but the counter itself does not — so triage on the rate-limited log can tell which is which only by reading the sample strings, not from the count summary line.

## Evidence

- [tlas.rs:113-127](crates/renderer/src/vulkan/acceleration/tlas.rs#L113-L127) — skinned-entity miss path:
  ```rust
  let blas_address: vk::DeviceAddress = if draw_cmd.bone_offset != 0 {
      let Some(entry) = self.skinned_blas.get_mut(&draw_cmd.entity_id) else {
          missing_blas += 1;
          if missing_samples.len() < MISSING_BLAS_SAMPLE_LIMIT {
              missing_samples
                  .push(format!("skinned entity {:?} (no BLAS)", draw_cmd.entity_id));
          }
          continue;
      };
      // ...
  ```
- The rate-limited warn at the bottom of `build_tlas` (around `tlas.rs:259`) prints `missing_blas: N` plus the first ~5 samples.

## Why it matters (mildly)

- **Skinned miss**: transient — resolves once `build_skinned_blas_batched_on_cmd` runs for that entity. Usually first-sight only.
- **Rigid miss**: potentially persistent — means an LRU eviction got something the draw command still references. Should be very rare; if it ever spikes, the operator wants to know.

Today an operator reading the log sees `missing_blas: 27, samples: [skinned entity X, skinned entity Y, …]` and has to count the sample strings to know if it's a 27-skinned warmup or a 24-skinned + 3-rigid eviction bug.

## Suggested fix

Split into `missing_skinned_blas` and `missing_rigid_blas` counters. Update the warn format string to print both. Trivial — ~10 LOC.

## Completeness Checks
- [ ] **UNSAFE**: No unsafe involved.
- [ ] **SIBLING**: The pattern is local to `build_tlas`; no other site to mirror.
- [ ] **DROP**: No Vulkan-object lifecycle change.
- [ ] **TESTS**: Unit test on the predicate path would over-engineer. A log-spy in an integration test is unwarranted at this severity.

## Source

[`AUDIT_RENDERER_2026-05-21_DIM8.md`](docs/audits/AUDIT_RENDERER_2026-05-21_DIM8.md) FINDING-D8-24.
