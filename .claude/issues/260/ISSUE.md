# #260: R-05..R-09 — Renderer LOW findings bundle

**Severity**: LOW | **Domain**: renderer | **Type**: enhancement
**Source**: `AUDIT_RENDERER_2026-04-12.md`

## Findings
- R-05: BLAS/TLAS scratch alignment not enforced (`acceleration.rs:182-187`)
- R-06: Stale size comments in `scene_buffer.rs:45,114` (112 != 128/192)
- R-07: Stale "4 attachments" comment in `pipeline.rs:414` (actual: 6)
- R-08: Contradictory depth store comment in `helpers.rs:76-79`
- R-09: Composite fog UBO fields uploaded but unused (`draw.rs:545-554`)
