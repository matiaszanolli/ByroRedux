# #1202 — NIF-DIM4-02: BSEffectShader implicit alpha_blend not unwound by explicit opaque NiAlphaProperty

**Source**: docs/audits/AUDIT_NIF_2026-05-19_DIM4.md (Dim 4, MEDIUM)
**Severity**: medium / Labels: bug, medium, nif-parser, import-pipeline
**State**: OPEN (filed 2026-05-19)
**Paired**: #1201 (same root)

## Cause

walker.rs:426-435 BSEffectShader branch sets `info.alpha_blend = true` implicitly; walker.rs:480-484 `apply_alpha_flags` only clears it when `alpha_test` is set. A `NiAlphaProperty { flags: 0 }` never unwinds the implicit blend.

## Fix

Defer the implicit-blend write until after `alpha_property_ref` consumed. Use `set_implicit_blend = !info.alpha_property_consumed` flag; apply post-`alpha_property_ref` only if still untouched.

## Game / Risk

Skyrim+ / FO4+ effect-shader shapes with `NiAlphaProperty { flags: 0 }`. LOW risk.
