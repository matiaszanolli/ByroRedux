# REN-D3-02: Stale "7 color attachments" comments in pipeline.rs

- **Issue**: #1646
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Source audit**: docs/audits/AUDIT_RENDERER_2026-06-16.md
- **Labels**: low, pipeline, documentation
- **Location**: `crates/renderer/src/vulkan/pipeline.rs` — comments ~L275-277 (opaque) and ~L709 (UI)

## Description
Comments say "main render pass has 7 color attachments (... + reservoir)" but the
arrays below correctly have 6 entries. Lag from `218b425b` reservoir removal.

## Evidence
Confirmed at HEAD: `pipeline.rs:275-276` and `:709` still say "7". All arrays = 6;
`create_render_pass` builds 6 color + depth; `triangle.frag` has 6 outputs;
`reflect.rs::triangle_frag_declares_six_color_outputs` PASSES. Comments only.

## Impact
None functional. Misleads readers into expecting a 7th attachment.

## Suggested Fix
Update both comments to "6 color attachments (HDR + normal + motion + mesh_id +
raw_indirect + albedo)".

## Completeness Checks
- [ ] SIBLING: both comment sites (~L275, ~L709) updated; no other "7 color attachment" comment remains
