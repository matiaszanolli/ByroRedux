# #2695: NIFAL-D8-2026-08-12-04: Two independent `BSShaderTextureSet` slot→role tables that already disagree

- **Severity**: MEDIUM
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: `single-boundary`
- **Game Affected**: Skyrim SE, FO4, FO76
- **Location**: `crates/nif/src/import/material/dedicated_shader.rs:97-238` (shader-type-aware) vs `byroredux/src/cell_loader/refr.rs:139-180` (shader-type-agnostic)
- **Status**: NEW
- **Description**: The importer resolves slots 2/4/7 differently per `shader_type`;
  the REFR overlay resolves the same NIF slot indices through one fixed table
  (`0→diffuse, 1→normal, 2→glow, 3→height, 4→env, 5→env_mask, 6→inner,
  7→specular`) and never sees `shader_type`. The two already disagree on slot 6
  (the overlay is the correct one — see D8-01) and on slots 2/4/7 for shader types
  4/5/11.
- **Evidence**: the two `match` blocks side by side; D8-01 measures which is right
  for slot 6.
- **Impact**: An XTXR swap on a FaceTint / SkinTint / MultiLayerParallax placement
  lands in a different canonical role than the same slot read from the mesh's own
  texture set, so an override changes shading semantics rather than just the
  texture — and any fix to one table silently fails to propagate to the other.
- **Related**: D8-01, D8-03.
- **Suggested Fix**: One `slot_to_role(shader_type, slot)` helper in `crates/nif`,
  called by both sites; the overlay gets `shader_type` from the cached import.

---
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-12.md` (finding `NIFAL-D8-04`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

