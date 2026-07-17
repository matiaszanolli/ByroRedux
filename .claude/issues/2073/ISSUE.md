# TD2-114: Bindless COMBINED_IMAGE_SAMPLER WriteDescriptorSet hand-rolled 2x in texture_registry.rs

**GitHub Issue**: #2073
**Labels**: low,vulkan,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `crates/renderer/src/texture_registry.rs:1165,1200`

## Description
Both sites need `.dst_array_element()`, which the existing `write_combined_image_sampler` helper in `descriptors.rs` doesn't expose, so they hand-roll the full `WriteDescriptorSet` builder instead of using the shared helper.

## Evidence
Confirmed live: `descriptors.rs:40` defines `write_combined_image_sampler(dst_set, binding, info)` with no array-element parameter. `texture_registry.rs:1168` and `:1203` both hand-build `vk::WriteDescriptorSet::default().dst_set(...).dst_binding(0).dst_array_element(w.handle)...` — the array-element call is exactly what's missing from the shared helper.

## Suggested Fix
Add a `write_combined_image_sampler_at()` variant with an array-element parameter. Low value in isolation — bundle with any other `descriptors.rs` touch (e.g. TD2-112).

**Effort**: small

## Completeness Checks
- [ ] **TESTS**: Existing bindless-texture-registry tests cover both call sites — purely mechanical extraction
