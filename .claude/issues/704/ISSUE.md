# Issue #704: O4-06: specular gloss applied as specStrength *= glossSample.r rather than authored Phong-exponent multiplier

**Severity**: LOW (defer behind PBR pipeline cleanup)
**File**: `crates/renderer/shaders/triangle.frag:728-734`
**Dimension**: Rendering Path

Gamebryo `NiTexturingProperty` slot 3 (gloss) is documented as a per-texel multiplier on `NiMaterialProperty.shininess` (i.e. the Phong exponent), NOT on the specular strength.

The current shader reads `glossSample.r` and multiplies `specStrength`. Effect: gloss-masked surfaces (polished armor inserts on dull leather backings) come out with the right ON/OFF mask but slightly wrong roughness profile.

Not visible on most Oblivion content because `glossiness` typically defaults to 80 and the mask binarises to 0/1 anyway.

**Fix**: Route `glossSample.r` to a per-pixel `roughness` modulation. Defer behind PBR roughness-pipeline cleanup.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
