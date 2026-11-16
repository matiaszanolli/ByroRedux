# REN-D3-01: `build_bindless_descriptor_bindings` rustdoc describes two bindings; it returns three

Labels: low, renderer, documentation, doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/7031

**Severity**: LOW
**Source**: `docs/audits/AUDIT_RENDERER_2026-11-16.md` (renderer audit, delta since 2026-11-09)
**Location**: `build_bindless_descriptor_bindings` (`crates/renderer/src/texture_registry/mod.rs`)
**Status**: NEW (validated against HEAD; no matching open issue)

## Description
The rustdoc names binding 0 (2D array) and binding 1 (authored environment cubemap array); the function returns `[DescriptorSetLayoutBinding; 3]`, the third being the per-slot first-level table read by `residentLod` (`f754eee28`, #6629).

## Suggested Fix
Add "binding 2 is the per-slot first-level table read by `residentLod`" to the rustdoc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
