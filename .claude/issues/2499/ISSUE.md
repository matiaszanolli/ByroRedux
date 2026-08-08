# REN-D11-2026-08-07-02: GBufferFormats doc says seven attachment formats / six G-buffer color targets

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2499
**Finding ID**: REN-D11-2026-08-07-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 11 — Pipeline/RenderPass
**Location**: `crates/renderer/src/vulkan/context/helpers.rs:48-50` (`GBufferFormats` doc comment)
**Status**: NEW

## Description
The struct doc predates the two FSR mask attachments (`5c56e311`/`5c7acfe2`). The struct itself has 8 fields and describes 9 render-pass attachments (8 colour + depth); the `fsr_mask_format` field is reused for attachments 6 and 7.

## Evidence
```rust
/// The seven attachment formats the main render pass writes — the six
/// G-buffer color targets plus depth. Groups the formats that travel
/// together into [`create_render_pass`].
pub(super) struct GBufferFormats { /* 8 fields, incl. fsr_mask_format */ }
```
Contrast with the accurate inline table further down (`helpers.rs:86-122`) and `log::info!("Render pass created (8 color + depth)")` at line 278.

## Impact
Someone adding a ninth colour attachment reads "seven" and mis-sizes one of the four per-pipeline blend arrays. That failure mode is a pipeline-creation error (VUID-...-renderPass-07609), so it's loud, but the doc is the first thing a new attachment author reads.

## Related
REN-D11-2026-08-07-03 (this report — same drift, sibling function).

## Suggested Fix
"The nine attachments the main render pass writes — eight G-buffer color targets (the two FSR masks share one format) plus depth."

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
