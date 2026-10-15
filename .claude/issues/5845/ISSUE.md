# TD7-2026-10-15-03: The three ground-cover passes hand-sum their descriptor-pool sizes while eight sibling passes derive them from the layout

Labels: low,renderer,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD7-2026-10-15-03`).

- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants (also Dim 2: a consolidation #5593 did not reach)
- **Location**: `crates/renderer/src/vulkan/groundcover/construct.rs:395-415` (`17 * frames`, `max_sets(3 * frames)`; layouts at `:325-363`), `crates/renderer/src/vulkan/groundcover_models.rs:272-285,421-434` (layout is `(0..12)`, pool is `11 * frames` + `frames` TLAS), `crates/renderer/src/vulkan/groundcover_bench.rs:690-712` (`4 * frames + 2 * frames`, `3 * frames`, `3 * frames`, `max_sets(2 * frames)`)
- **Status**: NEW
- **Age**: `0f9982177` (2026-10-01), `aabd99a057` (2026-09-24), `40b5c5b6a` (2026-09-06)
- **Effort**: small
- **Description**: `DescriptorPoolBuilder::from_layout_bindings(&bindings, sets)` (`vulkan/descriptors.rs:643`) makes "layout is the single source of truth" for pool sizing; ssao, exposure_meter, sky_cube, presentation, caustic, skin_compute, taa and compute use it. The ground-cover passes still carry a comment-arithmetic count ("7 scatter + 7 draw + 3 interaction storage bindings per frame-in-flight, plus the scatter's TLAS"). Counts are correct today (checked: scatter 7 storage + TLAS, interaction 3, draw 7; models 12 bindings, binding 3 the TLAS), so this is lockstep bait, not a live bug: adding a binding to a layout without touching the pool exhausts it at `allocate_descriptor_sets`. `passes_build_layouts_writes_and_teardown_through_the_shared_helpers` (`descriptors.rs:931`) polices layout/write/macro spellings but not pool sizing.
- **Evidence**: `.descriptor_count(17 * frames)` beside `set_layout_contracts()` which already returns the three binding lists.
- **Impact**: a future ground-cover binding addition fails at runtime only on a device/driver that enforces pool limits (validation layers catch it, release builds may not).
- **Related**: #5593 (closed 2026-10-14, helper consolidation), #5113 class.
- **Suggested Fix**: build each pool with `DescriptorPoolBuilder::from_layout_bindings(&contract.bindings, ...)` (chain `add_layout_bindings` for the three scatter/interaction/draw layouts); extend the shared-helper test to flag `descriptor_count(<n> * frames)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
