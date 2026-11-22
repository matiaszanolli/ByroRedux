# #7270: INC-R-03: `SkyCubeParams` lost its rustdoc to the new `SKY_CUBE_SUN_DISC_OFF` const

- **Severity**: LOW
- **Dimension**: audit-renderer / tech-debt (doc rot)
- **Location**: `crates/renderer/src/vulkan/sky_cube.rs:66-82`
- **Changed in**: 84104564e (RLT-IBL-010)
- **Status**: NEW
- **Description**: The const and its `///` block were inserted between the struct's doc block ("Host mirror of `include/sky.glsl`'s `SkyDome` …") and `#[repr(C)] pub struct SkyCubeParams`. Because the comment lines are contiguous, both doc blocks now attach to the const, and the struct has none.
- **Evidence**: Lines 66-75 hold the struct doc, 76-81 the const doc, then `pub const SKY_CUBE_SUN_DISC_OFF: f32 = 2.0;`, then `#[repr(C)]`.
- **Impact**: The rendered docs mis-describe the const and drop the struct's std140 rationale.
- **Related**: —
- **Suggested Fix**: Move the const and its doc above the `SkyCubeParams` doc block.

**Found by**: `docs/audits/AUDIT_INCREMENTAL_2026-11-22b.md`

## Completeness Checks
- [ ] **TESTS**: n/a (doc placement)
