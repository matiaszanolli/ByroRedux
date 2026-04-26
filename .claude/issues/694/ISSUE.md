# Issue #694: O4-02: NiVertexColorProperty.lighting_mode parsed but never consumed — LIGHTING_E meshes get material colors double-counted

**Severity**: MEDIUM
**File**: `crates/nif/src/blocks/properties.rs:1330-1366` (parser captures both `vertex_mode` + `lighting_mode`); `crates/nif/src/import/material.rs:1031-1033` (importer reads only `vertex_mode`)
**Dimension**: Rendering Path

`NiVertexColorProperty` carries a second enum the importer never reads. `lighting_mode` selects between:
- **LIGHTING_E**: vertex colors REPLACE the material's emissive/ambient/diffuse contribution
- **LIGHTING_E_A_D**: vertex colors are ADDED to the material's emissive/ambient/diffuse contribution (default)

The fragment shader at `triangle.frag:696` (`vec3 albedo = texColor.rgb * fragColor`) unconditionally treats vertex colors as a multiplicative tint, which is the LIGHTING_E_A_D approximation. **LIGHTING_E meshes (rare on Oblivion statics, more common on FX) end up with their material colors double-counted.**

**Fix**: Plumbing only — needs a 1-bit field on `GpuInstance.flags` or a routed enum in the existing `material_kind` ladder.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
