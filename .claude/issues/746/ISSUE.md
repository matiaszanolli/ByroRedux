# #746: SF-D1: Starfield shader-property tail-fields gated on `bsver == 155` skip on BSVER 172 (regression of #109)

URL: https://github.com/matiaszanolli/ByroRedux/issues/746
Labels: bug, nif-parser, high, legacy-compat

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 1, SF-D1-01/02/04)
**Severity**: HIGH
**Status**: Regression of closed **#109**

## Description

Three sites in `crates/nif/src/blocks/shader.rs` gate FO76+ trailing fields on `bsver == 155` when nif.xml gates them on `BSVER >= 155`. Starfield reports `bsver = 172` per `crates/nif/src/version.rs:129`, so all three sites silently skip on every Starfield shader block — every subsequent block parse for the same NIF drifts by tens of bytes.

This is the value-gate sibling of SF-D1-DISPATCH (filed separately): same `==` vs `>=` mistake, but distinct from the BSShaderType155 enum dispatch gate.

## Evidence

| Site | File / line | Skipped on BSVER 172 |
|---|---|---|
| SF-D1-02 | `shader.rs:923-927` (WetnessParams `unknown_2`) | 4 bytes |
| SF-D1-01 | `shader.rs:947-985` (BLSP LuminanceParams + TranslucencyParams + texture_arrays) | ~24 + ≤22 + variable |
| SF-D1-04 | `shader.rs:1418-1422` (BSEffectShaderProperty `refraction_power`) | 4 bytes |
| SF-D1-04 | `shader.rs:1462-1477` (BSEffectShaderProperty trailing reflectance/lighting/emittance/emit_gradient + Luminance) | ≥40 + 4 sized strings |

```rust
// shader.rs:923
let unknown_2 = if bsver == 155 { stream.read_f32_le()? } else { 0.0 };

// shader.rs:947
if bsver == 155 {
    luminance = Some(LuminanceParams { ... });
    do_translucency = stream.read_byte_bool()?;
    ...
}

// shader.rs:1418
let refraction_power = if bsver == 155 { stream.read_f32_le()? } else { 0.0 };

// shader.rs:1462
if bsver == 155 {
    reflectance_texture = stream.read_sized_string()?;
    ...
}
```

nif.xml `BSLightingShaderProperty` and `BSEffectShaderProperty` gate these fields on `BSVER >= 155`, not equality.

## Why this regressed from #109

#109 was closed under the assumption that the FO76 fix would carry forward. The original fix added the `bsver == 155` branches but never extended them to BSVER 168/172 when Starfield support landed via `version.rs:112-113`. Variant detection put Starfield on `bsver = 172`; the shader.rs gates kept `== 155`.

## Impact

Every Starfield NIF reads the WetnessParams + (when present) trailing FO76 sections at the wrong offset. Subsequent block parses drift; `block_size` skip recovers but the block contents are garbage. Visible result: PBR scalars on Starfield meshes default to zero, no luminance, no translucency, no texture arrays.

## Suggested Fix

Mechanically change every `bsver == 155` to `bsver >= 155` in the 4 cited sites. Once landed, run the parse-rate sweep across all five Starfield mesh archives and expect clean rate to climb.

## Completeness Checks

- [ ] **SIBLING**: Audit all `bsver == 155` occurrences in `crates/nif/src/blocks/`. The dispatch-gate sibling (SF-D1-DISPATCH) covers `:799 / 827 / 990`; this issue covers the value gates.
- [ ] **TESTS**: Add a regression test that parses a real Starfield shader block and asserts the WetnessParams + FO76 trailing fields are populated, not defaulted.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a (parser-only change).
