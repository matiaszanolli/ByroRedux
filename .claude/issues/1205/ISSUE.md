# #1205 — NIF-DIM4-05: FO76 BSEffectShader fields dropped at capture

**Source**: docs/audits/AUDIT_NIF_2026-05-19_DIM4.md (Dim 4, MEDIUM)
**Severity**: medium / Labels: bug, medium, nif-parser, import-pipeline
**State**: OPEN (filed 2026-05-19)

## Cause

`BsEffectShaderData` (material/mod.rs:752-812) has no fields for: `reflectance_texture`, `lighting_texture`, `emit_gradient_texture`, `emittance_color`, `luminance`. Parser at `blocks/shader.rs:1346-1355` reads them; `capture_effect_shader_data` (shader_data.rs:11-63) drops them.

## Fix

Extend `BsEffectShaderData` with 5 Option<…> fields; populate in capture. Renderer consumption = separate scope.

## Game / Risk

FO76. ZERO risk today (no consumer reads these fields yet).

## Estimated impact

Data plumbing only. Mirrors #345 pattern (capture-ready, renderer dispatch follows).
