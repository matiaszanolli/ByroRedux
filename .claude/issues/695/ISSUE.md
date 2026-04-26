# Issue #695: O4-03: NiVertexColorProperty.vertex_mode = Emissive routes vertex color into albedo, not emissive

**Severity**: MEDIUM
**File**: `crates/nif/src/import/material.rs:523-544` (`extract_vertex_colors`)
**Dimension**: Rendering Path

When `vertex_color_mode == Emissive` (`SOURCE_EMISSIVE` — torch flames, glowing signs, bake-driven emissive cards), `extract_vertex_colors` falls through the `use_vertex_colors` branch (gate is `mat.vertex_color_mode == AmbientDiffuse`) and returns the per-material `diffuse_color` constant. **The authored vertex-color emissive payload is silently dropped.**

The shader has a per-vertex `fragColor` lane and a per-instance `emissiveR/G/B` set, but no path for "per-vertex emissive".

**Impact**: Localised — affects flickering torches and a handful of signs.

**Fix**: Emit a second per-vertex attribute, OR pack the emissive contribution into the alpha lane and gate on a flag bit. The renderer already has an emissive_mult guard at `triangle.frag:833`, so the data has somewhere to land.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
