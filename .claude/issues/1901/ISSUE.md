# NIF-D5-01: parse_fo4 backlight-power presence threshold is 3.0e38, not nif.xml FLT_MAX

**Issue**: #1901 · **Severity**: LOW · **Labels**: low, nif-parser, nif, bug
**Dimension**: Collision/Shader Parsing · **Filed from**: docs/audits/AUDIT_NIF_2026-07-06.md (nif-deep suite)
**Game**: Fallout 4 (bsver 130–139, BSLightingShaderProperty full-body path)
**Location**: crates/nif/src/blocks/shader.rs:1024 (BSLightingShaderProperty::parse_fo4)

## Description
parse_fo4 reads Backlight Power with `if rim >= 3.0e38 && rim.is_finite()` (shader.rs:1024). nif.xml
gates on Rimlight Power >= FLT_MAX (3.4028235e38) && < FLT_INF. The 3.0e38 threshold is ~0.4e38
looser, so a rimlight in [3.0e38, 3.4028235e38) reads a 4-byte backlight_power nif.xml says is absent.

## Impact
Nil in practice — rimlight ships as FLT_MAX sentinel or normal-range value, nothing in the gap;
FO4 block_sizes absorbs drift regardless. .is_finite() correctly covers < FLT_INF + rejects NaN.

## Suggested Fix
`rim >= f32::MAX` (exactly nif.xml FLT_MAX). One-line change.

**Related**: #1175.
