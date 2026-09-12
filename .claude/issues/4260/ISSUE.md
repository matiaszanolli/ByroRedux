# OB-D4-01: APPLY_HILIGHT2 binds the normal map into the height slot unconditionally, but the alpha-channel gate only withholds a flag bit instead of undoing the binding — BC1 normals make POM read normal.r as height

**Issue**: #4260 — https://github.com/matiaszanolli/ByroRedux/issues/4260
**Labels**: high,renderer,game:oblivion,legacy-compat,bug

**Severity**: HIGH
**Dimension**: Dimension 4 — Rendering Path for Oblivion Shaders
**Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:289-294; byroredux/src/scene/nif_loader.rs:1067; byroredux/src/render/static_meshes.rs:480-486`
**Status**: NEW

## Description
`APPLY_HILIGHT2` (Oblivion's parallax route, #3596) binds `textures.height = textures.normal` unconditionally whenever `material.parallax_height_in_alpha` is set, in both `mesh_instance.rs` and `nif_loader.rs`. The only format check exists downstream at `static_meshes.rs:480-486`, which gates the `PARALLAX_ALPHA_HEIGHT_BIT` on `normal_has_alpha` — but when the normal map's format lacks alpha (BC1/DXT1), the code only withholds that bit; it does not zero `parallax_map_index`, so parallax occlusion mapping (POM) still runs and reads the normal map's X (red) channel as a height field.

## Evidence
Measured over the real Oblivion + Shivering Isles mesh archives: 1,274 `APPLY_HILIGHT2` properties total; 100 of them (7.8%) have a DXT1 (BC1, no alpha channel) `_n.dds` normal-map sibling and hit this defect. This is the same visual-artifact class (grazing-angle UV swimming under POM) that `#3562` was written to prevent for the reachability-gate half of this feature, now recurring at lower amplitude through a different code path (the binding itself, not the reachability gate).

## Impact
Visible parallax-mapping artifact (UV swimming at grazing angles) on ~7.8% of Oblivion's `APPLY_HILIGHT2`-authored materials whenever the mesh's normal map is BC1/DXT1 with no alpha channel.

## Related
Adjacent to #3562 (alpha-presence gate) and #3596 (APPLY_HILIGHT2 reachability fix), both of which remain correctly in place — this is a residual, distinct defect in the same area.

## Suggested Fix
When `parallax_height_in_alpha && !normal_has_alpha`, zero `parallax_map_index` entirely at the render site (`static_meshes.rs`) rather than only withholding the `PARALLAX_ALPHA_HEIGHT_BIT`, so POM does not run at all when there is no real height data to sample.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_OBLIVION_2026-09-11.md — findings verified against live code during this publish run.*
