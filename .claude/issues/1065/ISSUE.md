# #1065 — REN-D14-NEW-05: ui.vert header comment falsely claims materialId/MaterialBuffer lookup

**Severity**: LOW  
**Audit**: `docs/audits/AUDIT_RENDERER_2026-05-14_DIM14.md`  
**Location**: `crates/renderer/shaders/ui.vert:11-14`

## Summary

Lines 11-14 say the UI vertex stage "reads `materialId` to look up the texture in the `MaterialBuffer` SSBO". The code at line 48 reads `inst.textureIndex`. Same wrong mental model as #776/#785.

## Fix

Replace lines 11-14:
```glsl
// R1 Phase 6 — GpuInstance collapsed to per-DRAW data only. The UI
// vertex stage reads `textureIndex` directly from the per-instance
// struct — NOT from the MaterialBuffer SSBO. See #776 / #785 for why
// this is intentional: the UI quad's `materialId = 0` would alias
// the first scene material. Layout mirror of triangle.{vert,frag}.
```
