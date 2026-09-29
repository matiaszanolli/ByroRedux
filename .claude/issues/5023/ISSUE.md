# REN-D4-2026-09-29-01: shader-pipeline.md still describes the reactive attachment as FSR-only; TAA samples it since #4944

**Labels**: low,documentation,doc-rot,renderer

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-29.md`
**Severity**: LOW (doc-rot)
**Dimension**: Pipeline/RenderPass (doc)
**Location**: `docs/engine/shader-pipeline.md` §G-Buffer Layout (the Reactive row reads "FSR 3.1 reactive mask (transparent coverage)"; the prose attributes the two mask attachments to `frame_upscaler`'s FSR dispatch); `crates/renderer/shaders/taa.comp` header prose (lists only scene + motion vectors + mesh IDs as inputs).

## Description
Since #4944, `taa.comp` binding 9 (`uReactive`) reads G-buffer attachment 6 under `--upscaler taa`: it bypasses history at 1.0 and raises α below that. The docs attribute the attachment to FSR alone.

## Impact
Documentation only. A future change to the reactive writers could miss TAA as a consumer.

## Related
#4944 (closed); #4958 / #4871 (open, cover frame order, not this).

## Suggested Fix
Name TAA as a reader of attachment 6 in the G-buffer table and in the `taa.comp` header.

Validated at HEAD 9fcfdc3fc: `taa.comp` declares `layout(set = 0, binding = 9) uniform sampler2D uReactive;` and samples it; `shader-pipeline.md` Reactive row says "FSR 3.1 reactive mask".

## Completeness Checks
- [ ] **SIBLING**: other attachment rows in the G-buffer table checked for missing consumers
