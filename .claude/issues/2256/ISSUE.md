# TD1-078: volumetrics.rs crossed 2000 LOC — Session 62 fog/shadow-policy feature push, plus a 556-LOC constructor

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2256

**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/renderer/src/vulkan/volumetrics.rs` (2075 LOC); `VolumetricsPipeline::new_inner` (~556 LOC, line 522)
**Status**: NEW

**Description**: `volumetrics.rs` grew from 1172 LOC (07-25 boundary) to 2075 LOC today (+903, +77%), driven by the Session-62 feature arc: procedural fog density/extinction, boot-generated tileable density volumes, clustered local fog volumes, material-aware fog chromaticity, and the shadow-policy refactor's froxel-grid shadow integration. Within the file, `VolumetricsPipeline::new_inner` (the construction logic behind the public `new()` wrapper) is 556 LOC — the same "every new GPU resource appends another inline block to one giant constructor" shape already tracked for `VulkanContext::new()` (#1749), recurring in a different pipeline object.

**Evidence**: `git show 2cb86be5:crates/renderer/src/vulkan/volumetrics.rs | wc -l` → 1172; current → 2075. `new_inner` boundaries: line 522 to `create_volume` at line 1078.

**Impact**: Maintainability only. No correctness signal — a large, fast-moving, multi-week feature arc predictably outpaced extraction discipline.

**Related**: Same growth-shape as #1749 (`VulkanContext::new()`); no prior tracking for this file (it was well under threshold as of the last tech-debt audit).

**Suggested Fix**: Split along the file's own phase boundaries — pull `new_inner`'s per-image/per-buffer allocation blocks (density volume, scattering volume, two integrated volumes for temporal double-buffer, descriptor-set + pipeline-layout setup) into named helpers, or move `new_inner`+`create_volume`+`initialize_layouts` into a `volumetrics/init.rs` sibling and keep `dispatch`/`write_tlas`/`write_lights_and_clusters` (the per-frame recording path, already unit-tested) in `volumetrics.rs` proper — mirroring the `context/{mod,draw,resize,...}.rs` construct-vs-record split already established for `VulkanContext`.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
