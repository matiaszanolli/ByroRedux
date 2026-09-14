# #4339 — TD1-002: `crates/nif/src/blocks/shader.rs` re-crossed 2000 production LOC; no split axis recorded

**Labels**: low, nif-parser, nif, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4339

- **Severity**: LOW · **Dimension**: 1
- **Location**: `crates/nif/src/blocks/shader.rs` (2058 prod / 2073 total; tests already external in `crates/nif/src/blocks/shader_tests/`) · **Status**: NEW · **Age**: 1975 on 09-11 → crossed via `ae088b2ab`, `797e82124`, `37b2178b6` (all 2026-09-12) · **Effort**: medium · **Kind**: tech-debt
- **Finding**: The file holds four block families behind one shared head: legacy FO3/FNV `BSShader*Property` (`:47-411`), Skyrim+ sky/water + `BSShaderTextureSet` (`:502-636`), `BSLightingShaderProperty` + per-game parsers + `ShaderTypeData` (`:637-1706`, ~1070), and `BSEffectShaderProperty` (`:1707-2046`). No single function exceeds 200 LOC, so this is file cohesion, not function size. Every growth commit this week touched a single family.
- **Suggested Fix**: Split into a *shader/* directory **by block family**: mod (shared head `parse_skyrim_shader_base`, `read_starfield_tail`, `is_material_reference`, the `NiObject` impls, re-exports), legacy, sky_water, lighting, effect. Do not split per game, which would scatter one struct's impl across five files. No `include_str!` scans target the file today.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
