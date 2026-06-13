## Finding REN2-10 — Renderer Audit 2026-06-11

- **Severity**: LOW (informational)
- **Dimension**: Acceleration Structures
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:180`, `crates/renderer/shaders/skin_vertices.comp`, `crates/renderer/shaders/triangle.vert:190`, ray sites in `triangle.frag`
- **Status**: NEW. Validated CONFIRMED at HEAD `1e8a25ab` (`docs/engine/shader-pipeline.md` has zero mentions of render_origin / f32 headroom / absolute-space convention).

## Description

TLAS transforms, skinned BLAS vertices, and reconstructed ray origins stay in **absolute** world space (by design — the TLAS is absolute, `triangle.vert:187-190`). At |world| ≈ 176k the f32 quantization is ~0.02–0.03 u vs the 0.05–0.15 bias/tMin margins → 2–3× headroom. The margin scales linearly with |world| and exhausts near ~0.7–1 M-unit worldspaces. Nothing documents this ceiling, so future worldspaces could trip it silently.

## Suggested Fix

Record the bound in `docs/engine/shader-pipeline.md` (or add a debug_assert on worldspace extents at cell load).

## Completeness Checks
- [ ] **SIBLING**: Note the same ceiling applies to any future absolute-space shader consumer
- [ ] **TESTS**: debug_assert on worldspace extents at cell load, if that route is chosen

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
