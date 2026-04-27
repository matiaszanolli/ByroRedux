# NIF-D3-02: 4 more shader types still aliased to PPLighting parser (silent over-read)

URL: https://github.com/matiaszanolli/ByroRedux/issues/717
Labels: bug, nif-parser, medium

---

## Severity: MEDIUM

## Game Affected
FO3, FNV, Skyrim — latent (no vanilla content uses these, but mod-compat trap)

## Location
- `crates/nif/src/blocks/mod.rs:293-300`

## Description
The same alias arm flagged in NIF-D3-01 catches four more types whose nif.xml inheritance does **not** include `BSShaderLightingProperty`:
- `HairShaderProperty` (`inherit="BSShaderProperty"`, `versions="#BETHESDA#"`, no fields — nif.xml 6363)
- `VolumetricFogShaderProperty` (`inherit="BSShaderProperty"`, `versions="#FO3_AND_LATER#"`, no fields — nif.xml 6359)
- `DistantLODShaderProperty` (`inherit="BSShaderProperty"`, `versions="#BETHESDA#"`, no fields — nif.xml 6346)
- `BSDistantTreeShaderProperty` (`inherit="BSShaderProperty"`, `versions="#FO3_AND_LATER#"`, no fields — nif.xml 6350)

All four are empty BSShaderProperty subclasses on the wire. The PPLighting alias over-reads:
`texture_clamp_mode (4) + texture_set_ref (4) + refraction_strength (4) + refraction_fire_period (4) + parallax_max_passes (4) + parallax_scale (4)` = up to 24 bytes for `bsver >= 24` (the typical FO3+ case).

None of these appear in the vanilla mesh BSAs we tested, so corpus rate is unaffected today. The lurking bug is that any modder-shipped NIF that includes one of these types will silently drift on parse, with `block_sizes` recovering at the next block.

## Evidence
- Dispatch arm body at `blocks/mod.rs:293-300`
- nif.xml lines 6346, 6350, 6359, 6363 (each is a 0-field `<niobject>`)

## Impact
Hidden stream drift on any NIF that uses these blocks. No vanilla impact today; minor risk to mod compat (e.g., ENB-style custom volumetric fog shaders).

## Suggested Fix
Move the four names into a single `parse_base`-only arm:
```rust
"HairShaderProperty"
| "VolumetricFogShaderProperty"
| "DistantLODShaderProperty"
| "BSDistantTreeShaderProperty" =>
    Ok(Box::new(BSShaderPropertyBaseOnly::parse(stream, type_name)?)),
```
Implementation reuses `BSShaderPropertyData::parse_base` (already present in `base.rs:199`, covers `WaterShaderProperty` / `TallGrassShaderProperty`).

**Important**: do NOT remove `Lighting30ShaderProperty` from the alias arm — per nif.xml line 6367 it actually inherits `BSShaderPPLightingProperty`. Add inline comment to prevent future cleanup confusion (NIF-D3-03).

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D3-02 / NIF-D3-03)
- Sibling: #713 (NIF-D3-01 BSSkyShaderProperty / BSWaterShaderProperty in same alias arm)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Bundle with #713 (NIF-D3-01) — both fix the same dispatch arm
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Byte-exact dispatch test for each of the 4 types
- [ ] **DOC**: Add inline comment that `Lighting30ShaderProperty` legitimately needs PP layout (NIF-D3-03)
