# #5250: REN-D1-2026-10-05-01: #5195 fixed the TLAS scratch grow site but not the shrink target: `tlas_scratch_peak_bytes` still records `build_scratch_size`, so `shrink_tlas_scratch_to_fit` can cut the scratch below `updateScratchSize`

**Labels**: medium,renderer,vulkan,sync,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5250

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-05.md` — `REN-D1-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: MEDIUM
- **Dimension**: AS Correctness
- **Location**:
  - `crates/renderer/src/vulkan/acceleration/tlas.rs`, `ensure_tlas_state`: `self.tlas_scratch_peak_bytes[frame_index] = sizes.build_scratch_size;` and the comment above it ("refit/update reuse the existing scratch on the spec guarantee `BUILD ≥ UPDATE`").
  - `crates/renderer/src/vulkan/acceleration/memory.rs`, `shrink_tlas_scratch_to_fit` (`target = peak + scratch_alignment_padding`).
  - Field doc in `crates/renderer/src/vulkan/acceleration/mod.rs`.
  - Pin in `crates/renderer/src/vulkan/acceleration/tests/scratch_tests.rs`.
- **Status**: NEW. This is an incomplete fix of #5195, which is closed; the same class as baseline D9-03.
- **Description**: #5195 established that the spec does not bound `updateScratchSize` by `buildScratchSize` (VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-12259). It fixed:
  - the skinned path;
  - `BlasEntry::scratch_requirement` (the skinned shrink peak);
  - the TLAS *allocation* (`build_scratch_size.max(update_scratch_size) + padding`).

  It missed the TLAS peak, which is the shrink target. The shrink runs every frame from `draw.rs` on the next slot. Once its hysteresis (`tlas_scratch_should_shrink`) fires after a shrink-triggered rebuild, the slot's scratch is reallocated at BUILD size plus padding. Subsequent UPDATE builds (`decide_use_update`) reuse that buffer. The regrow check lives only inside `need_new_tlas`, so nothing re-grows it before the next fresh build.
- **Evidence**:
  - `fresh_build_records_peak_unconditionally_of_scratch_regrow` searches for the literal `self.tlas_scratch_peak_bytes[frame_index] = sizes.build_scratch_size;`, so the guard pins the defect.
  - The `shrink_tlas_scratch_to_fit` doc still says "the recorded peak is the unpadded `build_scratch_size`".
- **Impact**: On a driver whose `updateScratchSize > buildScratchSize`, every TLAS refit after a shrink writes past the scratch allocation, a GPU out-of-bounds write. It is reachable only on such a driver and only after the shrink path fires. That matches baseline D9-03's MEDIUM: it is unmeasured on the 4070 Ti and RADV.
- **Related**: #5195, #2915, #682.
- **Suggested Fix**: Record `sizes.build_scratch_size.max(sizes.update_scratch_size)` as the peak. Fix the comment and the `mod.rs`/`memory.rs` docs, and update the test needle to the max form.

## Publisher note

Incomplete fix of the closed #5195 (`640a3039d`). Independently corroborated by **CONC-D1-2026-10-05-01** in `docs/audits/AUDIT_CONCURRENCY_2026-10-05.md` (not filed separately). That report adds:
- The shrink is called post-present from `crates/renderer/src/vulkan/context/draw.rs` (`shrink_tlas_scratch_to_fit`).
- If `update > 2 × build + 256 KB` (`tlas_scratch_should_shrink`), the shrink fires on the very next post-present tick after **every** fresh build; the next frame on that slot fits the existing TLAS, so `ensure_tlas_state` returns early without re-growing scratch and `decide_use_update` picks UPDATE.
- Cheapest evidence: a one-line log of `sizes.update_scratch_size` vs `sizes.build_scratch_size` at the `ensure_tlas_state` query. Escalate to CRITICAL if the 4070 Ti or RADV values show `update > build`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
