# #5195 — REN-D9-2026-10-03-03: The skinned UPDATE refit assumes `updateScratchSize ≤ buildScratchSize`, which the spec does not guarantee; `updateScratchSize` is never read

**Labels**: medium,renderer,vulkan,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: MEDIUM.
  - This is a latent spec violation. If a driver returns `updateScratchSize > buildScratchSize`, every skinned refit violates VUID-…-pInfos-12259 and overruns the shared scratch: HIGH floor, plus a GPU-side overrun.
  - No such driver has been observed. The dev 4070 Ti and RADV values have not been dumped.
- **Dimension**: Skinning (AS scratch; the TLAS UPDATE sibling is Dim 1)
- **Location**:
  - `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`:
    - `build_skinned_blas_batched_on_cmd`: `max_scratch_size = max_scratch_size.max(sizes.build_scratch_size)`, and `BlasEntry { build_scratch_size: p.build_scratch_size, … }`.
    - `refit_skinned_blas`: `debug_assert!(scratch_buffer.size >= entry.build_scratch_size)` and the "UPDATE scratch ≤ BUILD scratch" comment.
  - `crates/renderer/src/vulkan/acceleration/memory.rs`, `shrink_blas_scratch_to_fit`: `shared_blas_scratch_peak(… e.build_scratch_size …)`.
  - Sibling: `crates/renderer/src/vulkan/acceleration/tlas.rs`, comment "Scratch is sized for BUILD which is >= UPDATE per Vulkan spec".
- **Status**: NEW. Searched "updateScratchSize", "update_scratch_size", "update scratch", "UPDATE scratch". The only hit is #247 (TLAS BUILD-vs-UPDATE mode, unrelated). AUDIT_PERFORMANCE_2026-08-12 PERF-D3-01 (#2460) questioned the shrink's static-only peak, not this premise.
- **Description**:
  - **The query result is discarded.** `vkGetAccelerationStructureBuildSizesKHR` returns both `buildScratchSize` and `updateScratchSize`. The skinned path keeps only the former: it sizes `blas_scratch_buffer` from it, stores it in `BlasEntry::build_scratch_size`, the shrink peak walks it, and the refit's only size check (a `debug_assert!`) compares against it.
  - **What the spec requires.** The UPDATE's requirement is VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-12259 (from `/usr/share/vulkan/registry/validusage.json`): for mode UPDATE, `[scratch, scratch + N)` must lie in one buffer, "where N is given by the updateScratchSize member … returned from a call to vkGetAccelerationStructureBuildSizesKHR with an identical VkAccelerationStructureBuildGeometryInfoKHR".
  - **The premise is undocumented.** Nothing in the spec relates the two sizes. Three code comments state "UPDATE scratch ≤ BUILD scratch" as fact, one of them "per Vulkan spec".
  - **Inconsistent with the rest of the file.** The surrounding code is careful about the other scratch VUIDs: alignment (#1386 / 03710), the serialize barrier (#1790), and allocate-before-retire (#4884).
- **Evidence**: `grep -rn "update_scratch_size\|updateScratchSize" crates/ byroredux/ docs/` returns nothing. `sizes.build_scratch_size` is the only `sizes.*scratch*` field read in `blas_skinned.rs`.
- **Impact**:
  - On a driver where the premise fails, every skinned refit overruns `blas_scratch_buffer`, a GPU-side write past the allocation.
  - The shrink can compound it, since it sizes to the build-size peak.
  - It is invisible to `cargo test`, and invisible to the validation layer on hardware where the premise happens to hold.
- **Related**: #2460 (union peak), #1386 (alignment padding), #4884, and Dim 1's TLAS UPDATE path (same assumption in `tlas.rs`).
- **Suggested Fix**:
  - Store `max(build_scratch_size, update_scratch_size)` from the query that already runs, either in the existing field or in a new `scratch_requirement`. Use it for the batch max, the shrink peak and the refit `debug_assert!`.
  - Correct the three comments.
  - The TLAS site can take the same one-line change.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
