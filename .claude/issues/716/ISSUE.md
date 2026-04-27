# NIF-D2-03: BSShaderPPLightingProperty.Emissive Color field never parsed

URL: https://github.com/matiaszanolli/ByroRedux/issues/716
Labels: bug, nif-parser, medium

---

## Severity: MEDIUM

## Game Affected
Skyrim-era PPLighting blocks (rare cross-version mod content). FO3/FNV unaffected (BSVER ≤ 34, the strict `#BS_GT_FO3#` gate excludes them).

## Location
- `crates/nif/src/blocks/shader.rs:55-87` (`BSShaderPPLightingProperty::parse`)

## Description
`BSShaderPPLightingProperty::parse` reads `texture_set_ref`, `refraction_*`, `parallax_*`, but never reads the trailing `Emissive Color: Color4` (16 bytes) gated by `vercond="#BS_GT_FO3#"` per nif.xml line 6250. Pre-Skyrim files (Bethesda streams 21-34) don't carry the field, so FO3 + FNV are unaffected. But cross-version mods that ship a Skyrim-era PPLighting block (rare but seen in Oldrim mod content) would have this field on disk; the parser would leave 16 bytes unread, masked by `block_sizes` recovery on 20.2.0.5+.

## Evidence
- nif.xml:6250 — `<field name="Emissive Color" type="Color4" vercond="#BS_GT_FO3#">Glow color and alpha</field>`
- nif.xml:18 — `#BS_GT_FO3# = (#BSVER# #GT# 34)` (strict)
- shader.rs:76-86 — `Ok(Self { net, shader, texture_clamp_mode, texture_set_ref, refraction_strength, refraction_fire_period, parallax_max_passes, parallax_scale })` — no emissive read.

## Impact
16-byte under-read on any BSShaderPPLightingProperty with bsver > 34. Block-size recovery on 20.2.0.5+ files will skip the trailing bytes silently. Means we drop the emissive channel from any Skyrim-era PPLighting block. BSLightingShaderProperty (the typical Skyrim shader) has its own correct emissive_color path at `shader.rs:480` — this only affects the rare PPLighting fallback.

## Suggested Fix
After `parallax_scale`, add:
```rust
let emissive_color = if stream.bsver() > 34 {
    [stream.read_f32_le()?, stream.read_f32_le()?, stream.read_f32_le()?, stream.read_f32_le()?]
} else {
    [0.0, 0.0, 0.0, 1.0]
};
```
and route into a new field in the struct. Cross-check by removing `block_sizes` recovery temporarily and parsing a Skyrim PPLighting test fixture.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D2-03)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify BSShaderNoLightingProperty doesn't have a similar trailing emissive
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Byte-exact regression with a Skyrim-era PPLighting block fixture
