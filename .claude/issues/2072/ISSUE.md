# TD2-113: Compute-pipeline-create helper (#1751) never migrated to caustic.rs/taa.rs/svgf.rs/compute.rs

**GitHub Issue**: #2072
**Labels**: low,vulkan,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `caustic.rs:374`, `taa.rs:371`, `svgf.rs:587,934`, `compute.rs:185`

## Description
`caustic.rs`/`taa.rs`/`svgf.rs` keep the shader module on their pipeline struct (freed later in `destroy()`), unlike the #1751 compute-pipeline-create helper's immediate-free semantics — that lifecycle difference is why they weren't swapped to the helper originally. `compute.rs` already matches the helper's semantics exactly (loads, uses, destroys immediately).

## Evidence
Confirmed live: `compute.rs:175-210` loads `shader_module` as a local, uses it for pipeline creation, then calls `device.destroy_shader_module(shader_module, None)` at both the success and cleanup-on-error paths (immediate-free). `caustic.rs`, `taa.rs`, `svgf.rs` each declare `shader_module: vk::ShaderModule` as a struct field, assign it during `new()`/build, and only free it in their `destroy()` methods (long-lived, freed at teardown) — confirmed via the struct field + constructor + `destroy_shader_module` call sites in each file.

## Suggested Fix
Migrate `compute.rs` first (trivial — it already matches). For the other three, removing the `shader_module` struct field is a small, isolated refactor per file.

**Effort**: `compute.rs` trivial; others small each

## Completeness Checks
- [ ] **DROP**: If `shader_module` is removed as a struct field from `caustic.rs`/`taa.rs`/`svgf.rs`, confirm the reverse-order `destroy()`/Drop sequence for the surrounding Vulkan objects (pipeline, pipeline layout, descriptor set layout) is unaffected
- [ ] **SIBLING**: Same lifecycle-mismatch pattern across all 3 remaining files — migrate consistently, not just one
- [ ] **TESTS**: Existing per-pass smoke/golden-frame tests (caustic/taa/svgf) cover pipeline creation and teardown paths
