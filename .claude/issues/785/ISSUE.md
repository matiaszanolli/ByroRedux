# R-N1 / #785 — ui.vert reads materials[0].textureIndex (regression of #776)

**Severity**: CRITICAL
**Domain**: renderer (Material Table / R1)
**Status**: Regression of #776 (closed by `9c7ea0d`, regressed by `c248a99` 3 hours later)

## Locations
- `crates/renderer/shaders/ui.vert:65-77`
- `crates/renderer/src/vulkan/context/draw.rs:984-994`
- `crates/renderer/src/vulkan/scene_buffer.rs:172-176`

## One-line summary
Stale shader hunk in `c248a99` (a systems.rs fix for #782) re-introduced the Phase-5 `materials[inst.materialId].textureIndex` read into `ui.vert`, undoing the #776 fix. UI overlay samples whatever `materials[0]` is (first scene material).

## Fix shape
One-line revert: drop `MaterialBuffer` SSBO + `GpuMaterial` struct from `ui.vert`, switch the read back to `inst.textureIndex`. Add a build-time grep guard to prevent re-regression.

## Audit source
`docs/audits/AUDIT_RENDERER_2026-05-03.md` finding R-N1.
