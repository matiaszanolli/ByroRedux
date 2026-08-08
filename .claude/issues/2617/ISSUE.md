# SF-D8-2026-08-07-01: BSEffectShaderProperty stub guard missing - every externally-referenced Starfield effect shader renders invisible

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2617
**Finding ID**: SF-D8-2026-08-07-01

**Severity**: HIGH
**Dimension**: 8 (NIFAL Canonical Material Translation for Starfield)
**Location**: `crates/nif/src/blocks/shader.rs:1616-1650` (`BSEffectShaderProperty::material_reference_stub`), `:1681-1698` (Starfield stub discriminator), `crates/nif/src/import/material/dedicated_shader.rs:365-500` (`apply_bs_effect_shader`, no guard) vs `:85-88` (the `BSLightingShaderProperty` guard that exists), `crates/renderer/shaders/triangle.frag:790-799`
**Status**: NEW — not covered by #2359 (tracks the `.mat`/CDB merge forwarding zero authored data, an approximate-not-invisible outcome) or #2354 (particles)

## Description
`#2353` added `if shader.material_reference { return; }` to the
`BSLightingShaderProperty` walker with the rationale that a
material-reference stub's fields are parser placeholders, not authored
data, and copying them would falsely suppress the external CDB values.
`apply_bs_effect_shader` has no equivalent guard — `grep material_reference
crates/nif/src/import/` returns exactly one production hit (the BSLSP arm).
For a stub, `apply_bs_effect_shader` copies the full placeholder set into
`MaterialInfo`: `base_color=[1,1,1,1]` → fabricated emissive tint,
`emissive_source` wrongly set to `Effect` (nothing was authored), and — the
lethal one — `falloff_start_opacity = falloff_stop_opacity = 0.0`.

## Evidence
```rust
// crates/nif/src/import/material/dedicated_shader.rs:365-... (apply_bs_effect_shader)
// no `if shader.material_reference { return; }` guard anywhere in this function,
// unlike the BSLightingShaderProperty walker at :85-88
```
`triangle.frag:790-799`'s cone-fade math:
```glsl
float coneFade = mat.falloffStartOpacity;
float denom = mat.falloffStartAngle - mat.falloffStopAngle;
if (denom > 1e-5) { ... }
...
finalAlpha = texColor.a * coneFade;
```
The in-shader comment asserts the identity default is
`start_op = stop_op = 1.0` ("the math reduces to a no-op"). The stub
hardcodes `0.0`, and with `start_angle == stop_angle == 1.0` (also stub
defaults), `denom == 0` skips the branch entirely — `coneFade` stays `0.0`
→ `finalAlpha = 0.0` on every affected surface. Scope: the stub
discriminator on Starfield is `!name.is_empty()`, and Starfield FX materials
are authored in `materialsbeta.cdb` and referenced by name — i.e. this is
the **dominant** path for Starfield effect geometry, not an edge case.
Full-body (non-stub) blocks are the ones with an *empty* name.

## Impact
Every externally-referenced Starfield `BSEffectShaderProperty` surface
renders fully transparent, with zero visual signal that anything is wrong —
a content-visibility failure with no workaround. Per the severity table,
"wrong/divergent Material out of NIFAL" is HIGH minimum; this is also
flatly worse than divergent — it's invisible.

## Suggested Fix
Mirror the #2353 guard in `apply_bs_effect_shader`: after
`info.material_path` capture, `if shader.material_reference {
info.material_kind = 101; return; }` (keep the kind tag, drop the
placeholder payload). Add a test asserting a stub yields
`emissive_source == EmissiveSource::None` and `effect_falloff == None`.

## Related
#2353 (the guard this mirrors, on the sibling type), #2359, #2354.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Fix belongs at the NIF-import walker (`apply_bs_effect_shader`), mirroring the existing BSLSP guard — same NIFAL boundary discipline
- [ ] **TESTS**: A stub fixture asserts `emissive_source == EmissiveSource::None` and `effect_falloff == None`
