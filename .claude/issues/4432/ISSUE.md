# #4432: NIFAL-D8-2026-09-16-05: The per-role colour-space guard pins 13 of the 25 secondary roles — `wrinkle`, `flow`, `lighting` and `reflectance` (all data maps) could flip to sRGB with no failing test

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4432
- **Labels**: low,nifal,test-gap,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW (test gap; the current table looks deliberate)
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: harness-coverage
- **Game Affected**: FO4/FO76 (wrinkle, flow, lighting, reflectance), all (tint, inner layer, decals)
- **Location**: `byroredux/src/asset_provider/texture.rs:906-986` (`common_material_texture_walk_covers_every_secondary_role_once`); table at `:743-782`
- **Status**: NEW
- **Description**: The test checks that every secondary role is visited once and that only `environment` is a cubemap. Colour space is asserted for only 7 Linear roles (`normal`, `smooth_spec`, `height`, `environment_mask`, `specular`, `lighting_mask`, `glass_roughness_scratch`) and 6 sRGB roles (`emissive`, `detail`, `dark`, `back_lighting`, `glass_dirt_overlay`, `decal_0`). `tint`, `inner_layer`, `lighting`, `flow`, `wrinkle`, `greyscale_lut`, `reflectance`, `emittance_gradient` and `decal_1..3` are unpinned.
  - `wrinkle` is an `_n` normal map (#2999) and `flow` is a vector field. `resolve_linear_texture`'s new doc names both as data textures.
  - #3814 already fixed the same shape of gap for the GPU lanes, so this is the remaining unpinned half.
- **Evidence**: the two `for role in [...]` lists at `:958-986`.
- **Impact**: A role-table edit that swaps a data map to sRGB would silently corrupt wrinkle normals or flow vectors on FO4 heads and water.
- **Related**: #3814, #2999, `60cef1c64`
- **Suggested Fix**: Assert the full 25-role colour-space table (exhaustive match on role name), so that adding a role without choosing its colour space fails the test.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
