# TAA-D13-02: taa.rs comment claims the OTHER history slot is UNDEFINED on frame 0; it's actually GENERAL with undefined contents

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1933

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/taa.rs:530-538` (comment inside `write_descriptor_sets`)
**Status**: NEW

## Description
The comment asserts "on session frame 0, the OTHER slot's images are in `UNDEFINED` layout (initialized but never written)." That is no longer accurate: `initialize_layouts` (called once right after `new()`) transitions all history slots UNDEFINED→GENERAL before any dispatch runs. At frame 0 the OTHER slot is in GENERAL layout with undefined contents — the descriptor's declared GENERAL layout is correct and there is no layout hazard; the protection the comment describes (the `params.y > 0.5` first-frame guard) still correctly avoids reading the undefined contents.

## Evidence
`mod.rs:2391` `t.initialize_layouts(...)`; `taa.rs:627-629` pushes an image barrier for every history slot. So by dispatch time no slot is in UNDEFINED layout.

## Impact
None functional. Risk is only that a future maintainer reads "UNDEFINED layout" and believes there is a live layout-transition hazard (there is a contents hazard, not a layout one).

## Related
REN-D11-NEW-04 (the audit tag cited in the same comment block)

## Suggested Fix
Reword to "the OTHER slot's images are in GENERAL layout but hold undefined contents (allocated + layout-initialised, never written)."

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
