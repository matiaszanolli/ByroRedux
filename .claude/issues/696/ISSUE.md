# Issue #696: O4-04: NiSpecularProperty disable on a NiMaterialProperty-bearing mesh leaves specular_color untouched — IOR glass branch silently re-enables spec

**Severity**: MEDIUM
**File**: `crates/nif/src/import/material.rs:1018-1022, 1039-1041`
**Existing issue**: #220 (closed, but the fix is incomplete)
**Dimension**: Rendering Path

When `NiSpecularProperty.flags & 1 == 0`, the importer sets `specular_strength = 0.0` (line 1040) which the fragment shader honors at the BRDF spec term (`triangle.frag:1259, 1362`). However, `specular_color` is left at the `NiMaterialProperty.specular` value, and `glossiness` stays nonzero.

Most paths in the shader gate on `specStrength * specColor`, so the multiply by zero kills the contribution — but **the IOR glass branch at `triangle.frag:970` does `specStrength = max(specStrength, 3.0)`, which silently RE-ENABLES the spec term on glass-classified meshes that explicitly disabled it.**

Realistic exposure: any Oblivion glass shape with `NiSpecularProperty { flags: 0 }`.

**Fix**: Either:
- Also clear `specular_color` to 0 when `!specular_enabled`, OR
- Push a `specular_enabled` bit to `GpuInstance.flags` and gate the `max(..., 3.0)` re-promotion.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
