# FO4-D1-NEW-01: BSLightingShaderProperty Backlight Power gate is inverted

**Labels**: bug, nif-parser, high

**Source**: [`docs/audits/AUDIT_FO4_2026-05-18.md`](docs/audits/AUDIT_FO4_2026-05-18.md)
**Dimension**: NIF BSVER 130 + half-float vertices
**Severity**: HIGH

## Observation

`crates/nif/src/blocks/shader.rs:876-890` (specifically line 882):

```rust
let (subsurface_rolloff, rimlight_power, backlight_power) = if (130..=139).contains(&bsver) {
    let sub = stream.read_f32_le()?;
    let rim = stream.read_f32_le()?;
    // Backlight only present if rimlight is not the FLT_MAX sentinel.
    // Use 3.0e38 threshold (below 3.4028235e38) to handle float precision.
    let back = if rim < 3.0e38 {
        stream.read_f32_le()?
    } else {
        0.0
    };
    (sub, rim, back)
} else { ... };
```

The gate reads `Backlight Power` when `rim < 3.0e38`. nif.xml 6609 spec:
`cond="(Rimlight Power #GTE# #FLT_MAX#) #AND# (Rimlight Power #LT# #FLT_INF#)"` — Backlight present **iff** `rim == FLT_MAX` (sentinel).

Both reference parsers agree:
- nifly `Shaders.cpp:477` reads Backlight when `rim == FLT_MAX`
- openmw `property.cpp:335` same

The boolean opposite.

## Why bug

Whichever direction reality picks, one of two failure modes is permanent:

- **(a)** Content with the spec-prescribed sentinel `rim=FLT_MAX` has its Backlight float skipped → 4-byte drift across the rest of `BSLightingShaderProperty` (`grayscale_to_palette_scale`, `fresnel_power`, the 7-float `WetnessParams`, plus optional shader-type trailing data and FO76 luminance/translucency).
- **(b)** Content with a real `rim` value (no backlight per spec) → we eat 4 bytes that are the next field.

`block_size` recovery resyncs at block boundary, so parse-rate stays at 100% (matches the ~3.5% silent under-read pattern called out in FO4-D1-C1 / #403). The misread fields silently default to wrong values without surfacing in parse logs.

The fixture at `crates/nif/src/blocks/shader_tests.rs:506` was hand-authored to match the inverted code, so the regression test passes for the wrong reason.

## Trigger Conditions

Any FO4 NIF whose `BSLightingShaderProperty` authors `rimlight_power = FLT_MAX` (the documented sentinel) — common for materials without a rimlight pass. Affects subsequent Phong / wetness fields.

## Fix

Flip the gate to `rim >= 3.0e38` (equivalently `rim.is_finite() && rim >= f32::MAX`). Update the test fixture comment + value at `shader_tests.rs:506` to set `rimlight_power = f32::MAX`, keeping the backlight read live. Re-run the FO4 main + DLC NIF sweep to confirm downstream under-read counts on `wetness` / `fresnel` drop.

## Completeness Checks

- [ ] **UNSAFE**: N/A (no unsafe)
- [ ] **SIBLING**: check FO76 (BSVER 155+) and Skyrim (BSVER 100) BSLightingShaderProperty parsers for the same sentinel-vs-finite pattern
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: update fixture at `shader_tests.rs:506` to set `rimlight_power = f32::MAX` and confirm the new `back` value is read live; add a sibling test pinning the inverted-case (real finite `rim` → no backlight read)

## Related

- FO4-D1-C1 / #403 — the silent under-read pattern this fix addresses at the source
