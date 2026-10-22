# #6148: TD2-2026-10-19-03: `vk::DescriptorImageInfo::default().sampler(..).image_view(..).image_layout(..)` is hand-spelled 92 times across 15 renderer files, after #5593 consolidated everything around it
STATE: OPEN
LABELS: bug,renderer,low,vulkan,tech-debt

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: counts by file: `composite.rs` 22, `svgf.rs` 17, `volumetrics/init.rs` 14, `taa.rs` 9, `bloom.rs` 8, `caustic.rs` 4, `sky_cube.rs` 3, `scene_buffer/descriptors.rs` 3, others 1-2 (`ssao`, `sky_cube/*`, `presentation`, `exposure_meter`, `groundcover_bench`, `texture_registry/mod.rs`, `water.rs`). Shapes: 37 x (sampler, view, SHADER_READ_ONLY_OPTIMAL), 22 x (sampler, view, GENERAL), 20 x (view, GENERAL), 13 other.
- **Age**: pre-#5593 (c6d36b4e7 consolidated `layout_binding` / `write_*` but not the image-info element)
- **Effort**: small (two helpers, mechanical migration)
- **Description**: `descriptors::write_combined_image_sampler` / `write_storage_image` take `&[DescriptorImageInfo]` that every caller builds by hand; the layout is the only thing that varies and it is determined by the descriptor type.
- **Evidence**: no divergence history found (checked `git log -S` on layout constants); this is breadth, not drift.
- **Proposed fix**: add `descriptors::sampled_image_info(sampler, view)` (SHADER_READ_ONLY_OPTIMAL) and `descriptors::storage_image_info(view)` (GENERAL) next to `write_storage_image` (`descriptors.rs:76`), migrate in place, and extend `passes_build_layouts_writes_and_teardown_through_the_shared_helpers` to count the raw chain.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._

