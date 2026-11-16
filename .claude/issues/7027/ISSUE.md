# REN-D12-01: depth-capture and linear-capture invalidate-before-read have no pin

Labels: low, renderer, vulkan, test-gap, bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/7027

**Severity**: LOW
**Source**: `docs/audits/AUDIT_RENDERER_2026-11-16.md` (renderer audit, delta since 2026-11-09)
**Location**: `depth_capture_finish_readback` (`crates/renderer/src/vulkan/context/depth_capture.rs`), the readback block in `crates/renderer/src/vulkan/context/linear_capture.rs`; only `screenshot_readback_invalidates_before_mapping` (`crates/renderer/src/vulkan/context/resources.rs`) pins the screenshot site
**Status**: NEW (validated against HEAD; no matching open issue)

## Description
After #6960 each unpinned site is one `invalidate_allocation(...)` line; deleting it compiles and passes all 1640 tests. Stale reads would corrupt depth / linear-radiance samples on non-coherent memory (the M-LIGHT linear-capture gates are the consumer).

**Impact**: test gap; only non-coherent drivers would exercise the failure.

## Suggested Fix
Extend the screenshot ordering test into a loop over the three capture files (`production_text` for the two with test modules).

Related (closed): #6960, #4602.
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
