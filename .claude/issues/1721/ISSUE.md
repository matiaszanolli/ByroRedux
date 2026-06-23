# NIF-D5-01: BSEffectShaderProperty Starfield material-reference stopcond missed the #1510 !name.is_empty() discriminator

**Issue**: #1721
**Severity**: MEDIUM
**Labels**: medium, nif-parser, nif, bug
**Source audit**: `docs/audits/AUDIT_NIF_2026-06-23.md`
**Dimension**: Collision/Shader Parsing (stream position)
**Game Affected**: Starfield (`bsver >= STARFIELD = 172`)
**Location**: `crates/nif/src/blocks/shader.rs` — `BSEffectShaderProperty::parse` (the `bsver >= FO76` stopcond block, ~lines 1632–1638)

## Description
`BSEffectShaderProperty::parse` decides whether a block is a material-reference stub by testing `is_material_reference(name)` for **all** `bsver >= FO76`, including Starfield. Its sibling `BSLightingShaderProperty::parse_fo76_plus` was explicitly fixed under #1510 to use `!name.is_empty()` for Starfield (content-hash paths with no suffix). `BSEffectShaderProperty` is the same `NiObjectNET`-derived subclass with the identical stopcond pattern but never received the fix. A Starfield `BSEffectShaderProperty` whose `net.name` is a suffix-less content-hash material reference therefore takes the **full-body parse path** instead of the 12-byte stub.

## Evidence
- `BSLightingShaderProperty::parse_fo76_plus` has the `bsver >= STARFIELD ⇒ !name.is_empty()` branch (regression test `parse_bs_lighting_starfield_hashpath_name_stubs`).
- `BSEffectShaderProperty::parse` (`shader.rs:1634`) has only `if is_material_reference(name)`.
- No Starfield-hashpath BSEffect test in `shader_tests.rs`.

## Impact
For any Starfield `.bgem`/`.mat`-referenced effect shader stored as a content-hash name, the parser reads body fields off bytes that belong to the next block's padding or the file footer. Damage bounded by `block_size` reconciliation (realigns the outer stream). Per `_audit-severity.md` ("NIF parse mismatch the `block_size` reconciliation covers" = MEDIUM): the struct lands with garbage source-texture/base-color/falloff fields — a silent-wrong-material risk, not a crash. Blast radius: Starfield effect shaders only.

## Related
#1510, #749, #746, #747

## Suggested Fix
Mirror the `parse_fo76_plus` discriminator (FO76 suffix-aware vs Starfield empty-name) in `BSEffectShaderProperty::parse`. Add a `parse_bs_effect_starfield_hashpath_name_stubs` test.

## Completeness Checks
- [ ] **SIBLING**: Discriminator matches `BSLightingShaderProperty::parse_fo76_plus` exactly
- [ ] **CANONICAL-BOUNDARY**: Effect-shader material still classifies at the NIFAL parser→Material boundary
- [ ] **TESTS**: `parse_bs_effect_starfield_hashpath_name_stubs` pins the Starfield stub path

## Validation
CONFIRMED against current code (HEAD 2d4c350d): shader.rs:1634 BSEffect uses bare `is_material_reference`; BLSP at ~1113 uses the STARFIELD `!name.is_empty()` split.
