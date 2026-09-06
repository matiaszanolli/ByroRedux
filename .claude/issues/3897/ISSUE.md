# #3897: FO4-2026-09-05-D5-01: FO4 greyscale→palette remap is unreachable on the lit path — the LUT binds, the enable bit never does (30,166 properties affected)

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3897 --json state`).*

---

**Audit**: `docs/audits/AUDIT_FO4_2026-09-05.md` (suite preset `texture-roles-deep`)
**Severity**: HIGH · **Dimension**: 5 (FO4 shader flags)

## Description

FO4's greyscale→palette remap is **unreachable on the lit path**. The texture binds; the enable bit never does.

`slot_to_role` correctly routes FO4 `BSShaderTextureSet` slot 3 into the `GreyscaleLut` role (#2997), and the LUT reaches the GPU as `GpuMaterial.greyscale_lut_index`. But nothing ever sets `MAT_FLAG_EFFECT_PALETTE_COLOR` for a `BSLightingShaderProperty`, so the shader branch that would consume it never runs.

## Evidence

`is_palette_color_from_modern_shader_flags` (`crates/nif/src/import/material/mod.rs:242`) has **exactly one caller** — `crates/nif/src/import/material/shader_data.rs:48`, which builds the `effect_*` field group for the **`BSEffectShaderProperty`** arm. There is no `BSLightingShaderProperty` equivalent.

`ImportedMaterial::bgsm_greyscale_lut_enabled` defaults to `false` (`crates/nif/src/import/types.rs:808`, `crates/nif/src/import/material/mod.rs:1535`) and is never set on the lit path.

The consumption gate at `byroredux/src/cell_loader.rs:318` requires **both**:
```rust
if material.textures.greyscale_lut.is_some() && material.bgsm_greyscale_lut_enabled {
```
The first half is now true on FO4; the second is permanently false. The shader branch at `crates/renderer/shaders/triangle.frag:1202-1212` is therefore dead code on FO4.

**Measured this audit** (corpus census over the FO4 archives): **30,166** FO4 properties set `SLSF1::Greyscale_To_PaletteColor`, of which **30,155** also carry a populated slot 3.

## Impact

Every FO4 asset authored to use the palette remap renders with the wrong colour: all hair and beards, power-armour and combat-armour palette variants, vehicle rust/paint variants, high-tech interior panelling, and brick/painted-wood architecture. This is a large, visible fraction of FO4's surface variety, and it fails silently.

Note this is one of **two independent gates** on the same feature — see the companion MEDIUM for the BGSM-side enable bit, which is dropped separately. Closing only one gate changes nothing on screen.

## Suggested Fix

Set the palette-colour flag for `BSLightingShaderProperty` at the same import boundary that already computes it for `BSEffectShaderProperty` — extend the `shader_data.rs` capture to the lit arm rather than adding a second classifier. Per NIFAL's single-boundary rule this belongs at the parser→`Material` boundary, not in the shader or the renderer.

## Completeness Checks
- [ ] **SIBLING**: The FO76 / Starfield lit paths checked — both share `parse_fo76_plus` and the same CRC-list flag union
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary, never pushed into shaders or re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins that a lit FO4 property with `Greyscale_To_PaletteColor` + slot 3 reaches the shader with the flag set
- [ ] **TESTS**: A regression test pins this specific fix

## Related
- #2997 (routed slot 3 into the `GreyscaleLut` role)
- #2108 / #2643 (the BGSM/BGEM enable-bit plumbing)
- Companion MEDIUM from the same audit — the second gate

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
