# #5200 — REN-D1-2026-10-03-01: #4633's non-finite TLAS drops are counted nowhere in `TlasIntegritySnapshot`, so `rt.integrity` reports FAIL with every cause counter at zero

**Labels**: low,renderer,vulkan,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas_instances`: the `non_finite_transform` arm and the `self.tlas_integrity = TlasIntegritySnapshot { .. }` assignment); `crates/renderer/src/vulkan/acceleration/mod.rs` (`TlasIntegritySnapshot`); `crates/core/src/ecs/resources/mod.rs` (`RtIntegrityStats`, `RtIntegrityStats::verdict`, `machine_line`); `crates/renderer/src/vulkan/context/telemetry.rs` (`fill_rt_integrity_stats`)
- **Status**: NEW. This is a gap left by #4633, which is closed and whose fix is in place. It is not a regression.
- **Description**: #4633 (`6e3ca2716`) added a fourth way an eligible draw can be dropped from the TLAS: `tlas_instance_transform` returns `None` for a non-finite model matrix.
  - The new counter `non_finite_transform` reaches only the rate-limited `log::warn!`.
  - `eligible_instances` is incremented before that arm, and the instance is not emitted.
  - `TlasIntegritySnapshot` has fields only for `missing_skinned_blas`, `missing_rigid_blas` and `missing_ssbo_instance`.
  - Result: the snapshot has `emitted < eligible` while all three cause counters are 0. `RtIntegrityStats::verdict` correctly turns FAIL, because `tlas_emitted == tlas_eligible` is false. But `machine_line` has no field that explains the gap.
  - The warn's sample suffix also misreports. The `"; ..."` overflow marker compares `missing_blas_total > missing_samples.len()`, which excludes the non-finite count.
- **Evidence**:
  ```rust
  let Some(transform) = tlas_instance_transform(draw_cmd) else {
      non_finite_transform += 1;   // only consumer: the warn below
      ...
      continue;
  };
  ...
  self.tlas_integrity = super::TlasIntegritySnapshot {
      frame, eligible: eligible_instances as u32, emitted: instance_count,
      missing_skinned_blas, missing_rigid_blas, missing_ssbo_instance, // no non-finite field
  };
  ```
  `grep -rn non_finite crates/renderer crates/core` finds only `tlas.rs`.
- **Impact**: Diagnostics only. The AS itself is correct: the instance is dropped, not corrupted.
  - The bench harness and the console report `verdict=FAIL` with `missing_*=0`. That is the "FAIL, no cause" shape #1228 and #3999 removed for the other causes.
  - It fires only when corrupt transforms get past the #4549/#4633 import gates. Those are exactly the frames where the operator most needs to know why.
- **Related**: #4633, #4549, #1228, #3833/#3999 (the integrity chain).
- **Suggested Fix**: Add `non_finite_transform: u32` to `TlasIntegritySnapshot` and to `RtIntegrityStats` (with a `machine_line` field). Fold it into `verdict` beside the other three, and count it in the warn's overflow-marker comparison. Extend `every_blas_residency_accessor_reaches_the_rt_integrity_snapshot`, or add a sibling test, so it pins the field through the whole chain.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
