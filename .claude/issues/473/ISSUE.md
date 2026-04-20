# Issue #473

FNV-REN-M2: Caustic scatter runs between SVGF and TAA — caustic energy enters TAA clamp AABB and flickers

---

## Severity: Medium

**Location**: `crates/renderer/src/vulkan/context/draw.rs:1013-1040`

## Problem

Current pipeline order:
1. SVGF temporal denoise (line 1000-1011)
2. Caustic scatter (line 1013-1030, #321)
3. TAA resolve (line 1032-1040)

Caustic splats on top of SVGF-denoised indirect, then TAA's YCoCg neighborhood variance clamp processes the combined result. Caustics are sparse, high-energy, spatially-jittery samples — they enter the TAA history AABB but fail the variance clamp next frame, so they flicker on high-roughness surfaces.

## Impact

Caustic highlights on glass bottles, water surfaces, jewelry, metallic clutter visibly shimmer rather than accumulate stably. Visual noise on every FNV cell with reflective props.

## Fix

Two options:

**(a) Move caustic AFTER TAA**. Requires adding a pass writing to the TAA output image. More pipeline/layout work but cleanest result.

**(b) Tag caustic pixels** with a mesh_id or history flag that TAA treats as "bypass clamp". Lower-effort, preserves caustic energy without neighborhood clamping. Requires shader and descriptor plumbing.

Option (b) is lighter and recommended unless caustic scatter needs TAA smoothing itself (probably not — it already denoises via splat).

## Completeness Checks

- [ ] **TESTS**: Visual regression on a scene with prominent caustics (glass bottles cell, HELIOS One mirrors); assert stable highlights
- [ ] **DROP**: New image if option (a) — verify teardown ordering
- [ ] **SHADER**: TAA compute shader bypass logic if option (b) — update descriptor set layout
- [ ] **LINK**: Cross-reference #321 (caustic scatter) in fix commit

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-REN-M2)
