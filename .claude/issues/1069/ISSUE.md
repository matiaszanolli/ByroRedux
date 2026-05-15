# #1069 — F-WAT-09: WATR reflection_color parsed but never propagated

**Severity**: LOW  
**Audit**: `docs/audits/AUDIT_RENDERER_2026-05-14_DIM17.md`  
**Location**: `crates/plugin/src/esm/records/misc/water.rs:78` (parsed) · `byroredux/src/cell_loader/water.rs:298-306` (not transferred)

## Summary

`WaterParams::reflection_color` correctly parsed from WATR DATA. `WaterMaterial` has no matching field; `resolve_water_material` never transfers it. `water.frag:216` hard-codes the reflection hit colour. Lava/chemical/muddy water reflection tints are silently dropped.

## Fix

1. Add `reflection_tint: [f32; 3]` to `WaterMaterial`.
2. Transfer from `rec.params.reflection_color` in `resolve_water_material`.
3. Grow `WaterPush` by 1 `vec4` (112→128 B, within Vulkan minimum).
4. Apply multiplicatively on the hit-colour return path in `traceWaterRay`.
5. Recompile SPIR-V.
