# DBG-D20-02: egui_pass.rs comment contradicts its own next line and describes a non-existent hazard

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1941

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/egui_pass.rs:194-198` (`EguiPass::dispatch`, step 4)
**Status**: NEW

## Description
The comment argues the begin/end render pass must always record — "cmd_draw early-returns on an empty primitive list, but we still want the begin/end to record because skipping mid-frame would mismatch the composite pass's expected layout transitions" — but the very next line guards the entire begin/end block with `if !primitives.is_empty()`, i.e. it does skip begin+end when the primitive list is empty. The comment's stated rationale is also incorrect: the egui RP has `initialLayout == finalLayout == PRESENT_SRC_KHR`, so skipping it is layout-neutral.

## Evidence
`grep` confirms line 195 ("we still want the begin/end to record") sits immediately above line 198 (`if !primitives.is_empty() {`). Render pass created with `initial_layout(PRESENT_SRC_KHR).final_layout(PRESENT_SRC_KHR)`. The doc header separately and correctly states dispatch "Returns silently on an empty primitive list."

## Impact
Documentation-only; runtime behavior is correct and safe. Risk is that a future reader trusts the comment and removes the `if !primitives.is_empty()` guard (harmless but wasteful), or is confused about which policy is authoritative.

## Related
`b99f5560` ("balance egui render pass on cmd_draw error"); REG-05 / #1637 / #1491

## Suggested Fix
Replace the comment to match the code, e.g.: "Skip the whole pass on an empty primitive list. The RP's initialLayout==finalLayout==PRESENT_SRC_KHR, so not recording it is layout-neutral." Keep the existing guard.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
