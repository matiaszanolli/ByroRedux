# NIF-D2-01: NiMaterialProperty compact-form ambient/diffuse default (0.5) contradicts nif.xml (1.0) — systemic ~50% ambient darkening on FO3/FNV

URL: https://github.com/matiaszanolli/ByroRedux/issues/2280
Labels: bug, nif-parser, high, nif

**Severity**: HIGH
**Dimension**: Version Gating
**Game Affected**: Fallout 3, Fallout New Vegas (any content using the legacy `NiMaterialProperty` in its Bethesda-compact form, `bsver >= FLAGS_U32_THRESHOLD (26)` — true for essentially 100% of retail FO3/FNV content, which ships at `bsver = 34`). Oblivion (`bsver = 11`, reads the field explicitly) and Skyrim+ (uses `BSLightingShaderProperty`, not this legacy property) are unaffected.
**Location**: `crates/nif/src/blocks/properties.rs:44-63` (parser default); contradicted by `crates/renderer/shaders/triangle.frag` (consumer assumption, `AMBIENT_FILL` block)

## Description

nif.xml gates `NiMaterialProperty.Ambient Color` / `.Diffuse Color` on `vercond="#BSVER# #LT# 26"` with `default="#VEC3_ONE#"` — verified directly against `/mnt/data/src/reference/nifxml/nif.xml`, `default="1.0, 1.0, 1.0"`. The parser correctly detects the field-absent case (`bethesda_compact = stream.bsver() >= FLAGS_U32_THRESHOLD`, matching the `#LT# 26` vercond with the right operator — `FLAGS_U32_THRESHOLD == 26` confirmed at `version.rs:358`), but substitutes `NiColor { r: 0.5, g: 0.5, b: 0.5 }` instead of `(1.0, 1.0, 1.0)`.

Independently re-verified: the shader's own `AMBIENT_FILL` tuning comment in `triangle.frag` (REND-#1452 "Ulysses Temple floor" fix) explicitly reasons that "FO3/FNV and Skyrim interiors … have `mat_ambient=(1,1,1)`" — the renderer already assumes and depends on the spec-correct value, which the parser has never actually supplied for retail FO3/FNV content.

## Evidence

```rust
// crates/nif/src/blocks/properties.rs:44-63
let bethesda_compact = stream.bsver() >= crate::version::bsver::FLAGS_U32_THRESHOLD;
let ambient = if bethesda_compact {
    NiColor { r: 0.5, g: 0.5, b: 0.5 }   // nif.xml default is VEC3_ONE = (1.0,1.0,1.0)
} else {
    stream.read_ni_color()?
};
// (diffuse mirrors the same pattern)
```

```glsl
// crates/renderer/shaders/triangle.frag — AMBIENT_FILL block
vec3 dielectricAmbient = sceneFlags.yzw * vec3(mat.ambientR, mat.ambientG, mat.ambientB) * (1.0 - metalness);
...
const float AMBIENT_FILL = 0.5;
vec3 ambientFill = sceneFlags.yzw * AMBIENT_FILL;
vec3 ambient = max(dielectricAmbient + metallicAmbient, ambientFill);
```

With `mat_ambient = 0.5` (bugged) and `metalness ≈ 0` (typical dielectric): `dielectricAmbient = 0.5 × cell_ambient`, numerically equal to `ambientFill = 0.5 × cell_ambient`, so `max()` collapses to `0.5 × cell_ambient` instead of the intended `1.0 × cell_ambient`.

## Impact

Every FO3/FNV static mesh carrying a legacy `NiMaterialProperty` (the norm for `BSShaderPPLightingProperty`/`BSShaderNoLightingProperty` content, i.e. most non-Skyrim+ shapes) renders at roughly half the intended ambient-fill brightness outside direct N·L light contribution — a systemic under-lighting bias across two full games' worth of content, in the exact code path that was already tuned once under the false belief the parser default was correct. The same wrong `0.5` also feeds `diffuse_color` consumers (fog tint, conductor-tint path in `asset_provider/material.rs`), though the ambient-term path is the dominant visible effect.

## Suggested Fix

Change the compact-form defaults in `NiMaterialProperty::parse` from `(0.5, 0.5, 0.5)` to `(1.0, 1.0, 1.0)` for both `ambient` and `diffuse`, matching nif.xml's `VEC3_ONE` default and the shader's already-tuned assumption. Add a regression test pinning `bsver=34` (FNV) ambient/diffuse to `(1.0,1.0,1.0)` alongside the existing `#323`/`#938` emissive-mult tests in the same file.

## Completeness Checks
- [ ] **SIBLING**: Check whether any other Bethesda-compact-form property default in `properties.rs` (or elsewhere in `blocks/`) was copied from the same wrong assumption
- [ ] **TESTS**: A regression test pins `bsver=34` ambient/diffuse to `(1.0,1.0,1.0)`, alongside the existing `#323`/`#938` emissive-mult tests

