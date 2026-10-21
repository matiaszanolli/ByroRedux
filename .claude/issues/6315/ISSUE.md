# #6315: PERF-D4-02: `fold_svgf_scene_inputs` re-hashes every unique `GpuMaterial` each frame although the material table already holds each hash

**Labels**: low,performance,renderer,bug

**Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-21.md · **Severity**: LOW · **Dimension**: SSBO Sizing & Upload · **Location**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs (fold_svgf_scene_inputs)`

- **Location**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:46-52`; the existing hashes are in `crates/renderer/src/vulkan/material.rs:1095-1120` (`MaterialTable.index: FxHashMap<u64, u32>`, keyed on `hash_gpu_material_fields`).
- **Status**: NEW. The skill's Dim 4 asked for a number ("O(unique materials), unmeasured; quantify before filing").
- **Description**: one frame now hashes the material bytes three times. `intern` hashes once per draw (#4201). `hash_material_slice` hashes once per unique material for the upload gate. `fold_svgf_scene_inputs` hashes once per unique material again.
- **Evidence**: **measured** on the 7950X (`fxprobe`, release, FxHasher over 432-B records, best of 200): 1,000 unique materials 14 µs, 4,000 57 µs, 8,000 113 µs. memory-budget.md records 4,000+ unique materials on a Skyrim radius-3 exterior.
- **Impact**: ~0.06–0.11 ms of main-thread CPU per exterior frame. No quantitative guard exists for this site.
- **Related**: PERF-D4-01 (same function; its fix may remove the loop).
- **Suggested Fix**: have `MaterialTable` keep an insertion-ordered `Vec<u64>` of the hashes it already computes, and fold those (O(unique) u64 folds, ~µs). Alternatively fold the upload gate's single `hash_material_slice` value.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
