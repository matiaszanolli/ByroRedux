# REN-D11-02: Pipeline doc comment overclaims "no push constants on any pipeline"

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1930

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/pipeline.rs:693-695` (`create_ui_pipeline` doc comment)
**Status**: NEW

## Description
The doc comment states "No push constants exist on any pipeline — per-instance data lives in the instance SSBO." This is true for the shared scene+UI pipeline layout, but the water pipeline uses a 128-byte push-constant range on its own layout.

## Evidence
`crates/renderer/src/vulkan/water.rs:299,306,473`; pinned by the `water_push_layout_is_128_bytes` test.

## Impact
Documentation only — could mislead a future reader auditing push-constant/layout consistency into thinking the whole renderer is push-constant-free.

## Related
None

## Suggested Fix
Reword to scope the claim to the shared scene/UI layout, e.g. "No push constants on the scene/UI pipeline layout (water uses its own 128-byte push-constant layout)."

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
