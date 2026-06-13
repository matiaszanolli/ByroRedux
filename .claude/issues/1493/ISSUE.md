## Finding REN2-08 — Renderer Audit 2026-06-11

- **Severity**: LOW
- **Dimension**: Volumetrics (Dims 2 + 18)
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:71-92` vs the CameraUBO-only pin pattern in `crates/renderer/src/vulkan/reflect.rs:432-469`
- **Status**: NEW. Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

The `VolumetricsParams` UBO just grew (`render_origin: [f32; 4]` at `volumetrics.rs:88-91`, now 144 B, currently verified matching at offsets 0/64/80/96/112/128) but unlike CameraUBO has no `uniform_block_size_by_name` reflection pin — `reflect.rs:432-469` pins only `"CameraUBO"` across 6 shaders; grep for `VolumetricsParams`/`volumetrics_inject`/`volumetrics_integrate` in `reflect.rs` returns zero matches. `volumetrics.rs` only calls `validate_set_layout` (binding shape, not block size). This is exactly the stale-`.spv` drift mode the camera-relative delta risked.

## Suggested Fix

Add the same reflection size pin for `VolumetricsParams` (and consider `CausticParams` while there).

## Completeness Checks
- [ ] **SIBLING**: Audit every other non-CameraUBO uniform block for missing size pins (CausticParams, SSAOParams, SkyParams, …)
- [ ] **TESTS**: The pin itself is the regression test — verify it fails on a deliberately mismatched struct

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
