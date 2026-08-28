# OBL-2026-08-27-02

Issue: #3517 — https://github.com/matiaszanolli/ByroRedux/issues/3517
Filed: 2026-08-27 by /audit-publish from docs/audits/AUDIT_OBLIVION_2026-08-27.md

Source: `docs/audits/AUDIT_OBLIVION_2026-08-27.md` — finding `OBL-2026-08-27-02`

- **Severity**: MEDIUM
- **Dimension**: 4 — Rendering Path (legacy `NiProperty` chain)
- **Location**: `crates/nif/src/import/material/legacy_properties.rs:272-276` vs. its four siblings at `:405-408`, `:525-528`, `:596-599`, `:610-613`

## Description

`apply_legacy_property_chain` walks `direct_properties` **then** `inherited_props`, so the shape's own property must win (#208). #2328 / FO3-D1-06 implemented that for clamp mode with a `texture_clamp_mode_consumed` latch, and all four `BSShader*` writers (`apply_pp_lighting_property`, `apply_no_lighting_property`, `TileShaderProperty`, `SkyShaderProperty`) read *and* set it.

The `NiTexturingProperty` writer does neither — it gates on the value shape (`if info.texture_clamp_mode == 3`) instead:

```rust
// legacy_properties.rs:272-276
if info.texture_clamp_mode == 3 {
    if let Some(base) = tex_prop.base_texture.as_ref() {
        info.texture_clamp_mode = (base.flags & 0xF) as u8;
    }
}
```

This is precisely the pattern the sibling `apply_legacy_alpha_property` documents as wrong three functions above (#1201: "gate on `alpha_property_consumed`, not on the `!alpha_blend && !alpha_test` value-shape").

## Evidence

`grep -n 'texture_clamp_mode_consumed' crates/nif/src/import/material/legacy_properties.rs` returns the latch at `:405/:407`, `:525/:527`, `:596/:598`, `:610/:612` — and nothing in the `NiTexturingProperty` arm at `:272-276`.

The asymmetry breaks precedence in both directions:

- A **shape-level** `NiTexturingProperty` writes the clamp mode without latching consumption, so an **inherited** `BSShaderPPLightingProperty` later in the same walk sees `!consumed` and overwrites it — inherited beats shape, the exact inversion #2328 was written to prevent.
- A **shape-level** `BSShader*` that legitimately authors clamp mode 3 sets `consumed = true`, but the `== 3` gate does not consult that latch, so an **inherited** `NiTexturingProperty` overwrites it anyway.

## Impact

Wrong sampler address mode on FO3/FNV meshes that mix a legacy `NiTexturingProperty` with a `BSShader*` property across the shape/parent-node boundary — FNV ships 58 706 `BSShaderPPLightingProperty` and 3018 `NiTexturingProperty` blocks, so the mixed shape exists. Structurally latent on Oblivion (`oblivion.tsv` contains **zero** `BSShader*` rows, so only one writer can ever run), which is why this survived the FO3/FNV sweeps that introduced the latch.

Filed at MEDIUM rather than HIGH because it needs the mixed-chain shape, where `OBL-2026-08-27-01` needs nothing at all.

## Related

- `OBL-2026-08-27-01` (same four lines; fixing both together is one edit).
- `#2328` / FO3-D1-06 introduced the latch.
- `#1201` is the identical value-shape-vs-latch bug already fixed for `NiAlphaProperty`.

## Suggested Fix

Replace the `== 3` value gate with `if !info.texture_clamp_mode_consumed { … ; info.texture_clamp_mode_consumed = true; }`, matching the four siblings. Add a chain-order unit test with a direct `NiTexturingProperty` plus an inherited `BSShaderPPLightingProperty` asserting the shape's value survives.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
