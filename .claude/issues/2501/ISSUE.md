# REN-D11-2026-08-07-04: Water blend-state comment names a removed attachment and the wrong index range

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2501
**Finding ID**: REN-D11-2026-08-07-04 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 11 — Pipeline/RenderPass
**Location**: `crates/renderer/src/vulkan/water.rs:624-628` (`build_pipeline`)
**Status**: NEW

## Description
The comment above the water colour-blend array says "Attachments 1..6 are write-masked off" and lists six names ending in "reservoir". Post-#1583 there is no reservoir attachment; the masked-off range is 1..=5 and attachments 6/7 are *not* masked off — they are the two FSR masks, which water writes at full strength with MAX blending (correctly implemented ~20 lines below, and correctly documented there).

## Evidence
```rust
// Attachments 1..6 are write-masked off: water never updates
// the G-buffer (normal / motion / mesh_id / raw_indirect /
// albedo / reservoir) so SVGF and motion-vector reprojection see
// only the opaque pass behind the water.
```
followed by `let attachments = [hdr_blend, masked_off ×5, fsr_mask_max ×2];`

## Impact
Doc-only, but actively misleading: it asserts water writes no FSR mask when water is described elsewhere in the same function as "the canonical transparency-and-composition case". A reader debugging FSR ghosting on water would be sent the wrong way.

## Related
The accurate sibling comment at `water.rs:641-644`; the stale reservoir reference is the same class as the already-corrected note at `water.rs:660` ("the reservoir attachment was removed under #1583").

## Suggested Fix
"Attachments 1..=5 are write-masked off (normal / motion / mesh_id / raw_indirect / albedo); 6 and 7 (the FSR masks) are written — see below."

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
