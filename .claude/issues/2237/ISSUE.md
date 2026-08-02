# REN-D12-02: Fire-refraction's composition-phase sort key globally inverts back-to-front order against unrelated transparents

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2237

**Dimension**: 12 (Draw sort/composition)
**Location**: `byroredux/src/render/mod.rs` (`draw_sort_key`, line 209; `MATERIAL_KIND_FIRE_REFRACTION` special-case at line 224); `byroredux/src/render/static_meshes.rs:223`
**Status**: NEW

**Description**: `draw_sort_key`'s special-case for `MATERIAL_KIND_FIRE_REFRACTION` inverts sort order for the proxy relative to the rest of the alpha-over transparent set, rather than only relative to other fire-refraction proxies. Any unrelated alpha-blended transparent behind a fire-refraction proxy (e.g. smoke, glass) can be drawn in the wrong order relative to it.

**Impact**: Visually incorrect back-to-front compositing whenever a fire-refraction proxy shares screen space with unrelated alpha-over transparents.

**Related**: REN-D2-01 / REN-D11-02 (same material kind's design gaps)

**Suggested Fix**: scope the fire-refraction sort-key adjustment so it only reorders relative to other fire-refraction proxies, preserving standard back-to-front order against unrelated transparents.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
