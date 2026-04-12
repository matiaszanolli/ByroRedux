# #269: R2-03..R2-08 — Renderer re-audit LOW findings bundle

**Severity**: LOW | **Domain**: renderer | **Type**: enhancement

## Findings
- R2-03: GBuffer.recreate_on_resize partial-failure cleanup (`gbuffer.rs:237-248`)
- R2-04: SVGF.recreate_on_resize same pattern (`svgf.rs:758-773`)
- R2-05: SSAO per-pixel inverse(viewProj) (`ssao.comp:51`)
- R2-06: cluster_cull per-workgroup inverse(viewProj) (`cluster_cull.comp:83`)
- R2-07: GBuffer doc table missing 2 attachments (`gbuffer.rs:6-11`)
- R2-08: Window portal 0.6-unit blind zone (`triangle.frag:385`)
