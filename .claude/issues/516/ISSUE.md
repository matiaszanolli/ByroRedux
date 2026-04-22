# FNV-RT-1: Frustum culling drops off-screen BLAS from the TLAS

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/516
- **Severity**: CRITICAL
- **Dimension**: RT lighting
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

- `byroredux/src/render.rs:296-306` — cull → `continue`
- `crates/renderer/src/vulkan/acceleration.rs:1364-1373` — LRU-stamp site gated on `in_tlas`

## Summary

Frustum-culled geometry is dropped before `DrawCommand` push, which excludes it from the TLAS **and** skips the `last_used_frame` update. On-screen fragments lose shadow/GI from off-screen occluders; BLAS age drifts toward LRU eviction despite being valid shadow casters.

Fix with: `/fix-issue 516`
