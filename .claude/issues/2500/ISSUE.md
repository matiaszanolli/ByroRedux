# REN-D11-2026-08-07-03: create_main_framebuffers doc omits the two FSR mask attachments

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2500
**Finding ID**: REN-D11-2026-08-07-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 11 — Pipeline/RenderPass
**Location**: `crates/renderer/src/vulkan/context/helpers.rs:282-287` (`create_main_framebuffers` doc comment)
**Status**: NEW

## Description
The doc enumerates the bound views but stops at `albedo`, even though the function binds 9 views and `GBufferViews` carries `reactive_views` + `transparency_views` (correctly documented as attachments 6 and 7 on the struct fields themselves).

## Evidence
```
/// Create one main framebuffer per frame-in-flight slot. Each framebuffer
/// binds that slot's HDR + normal + motion + mesh_id + raw_indirect +
/// albedo views, plus the shared depth view.
```
vs. the actual `attachments` array at `helpers.rs:336-346`, which has 9 entries including `reactive_views[i]` and `transparency_views[i]`.

## Impact
Doc-only. The `debug_assert_eq!` length checks below do cover all seven colour slices, so the code is self-guarding.

## Related
REN-D11-2026-08-07-02 (this report).

## Suggested Fix
Append "+ the two FSR masks" to the enumeration.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
