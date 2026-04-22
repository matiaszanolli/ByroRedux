# #129 Investigation

## Root cause confirmed

`extract_bs_tri_shape` (`crates/nif/src/import/mesh.rs:241+`) duplicated
~130 lines of material property extraction rather than delegating to
`extract_material_info`. Every shader-related capture (texture paths,
UV transform, emissive, specular, ShaderTypeData variants, decal flag,
two-sided flag, BGSM/BGEM material path, implicit effect-shader alpha)
lived in two places. NIF-403 (missing BSEffect two_sided) was one
concrete drift instance; the audit predicted more.

## Drift actually caught by the refactor

Four separate parity gaps surfaced once I replaced the BsTriShape
inline extraction with the shared path and re-ran the tests:

1. **`two_sided` on BSEffectShaderProperty**: shared extractor only
   checked `BSLightingShaderProperty.shader_flags_2 & SF2_DOUBLE_SIDED`.
   BsTriShape's local `bs_tri_shape_two_sided` also checked BSEffect.
   Now folded into the shared path → NiTriShape with a BSEffect shader
   (rare but valid) now picks up the flag.

2. **`is_decal` on BSEffectShaderProperty**: same pattern. Pre-fix only
   BsTriShape's `find_decal_bs` checked both shader variants. Now
   both paths pick it up via `is_decal_from_shader_flags`.

3. **`mat_alpha` from `BSEffectShaderProperty.base_color[3]`**: pre-fix
   only BsTriShape read it. NiTriShape with a BSEffect shader silently
   dropped the BGEM alpha multiplier.

4. **`env_map_scale` and FO4+ `normal_texture` from BSEffect**: same —
   only BsTriShape captured them. Now both paths do.

## Fix shape

1. Extract a refs-based core `extract_material_info_from_refs`
   taking `(shader_property_ref, alpha_property_ref, direct_properties,
   inherited_props)`. Empty slices on the BsTriShape side (Skyrim+
   geometry has no legacy NiProperty chain).
2. `extract_material_info` keeps its existing `&NiTriShape` signature
   as a thin wrapper over the new core — every existing caller
   unchanged.
3. `extract_bs_tri_shape` rewrites from ~130 lines of inline extraction
   to a single `extract_material_info_from_refs` call + field
   mapping onto `ImportedMesh`.
4. Drop the obsolete BsTriShape-only helpers:
   - `bs_tri_shape_two_sided` (`mesh.rs`)
   - `find_texture_path_bs_tri_shape` (`mesh.rs`)
   - `find_material_path_bs_tri_shape` (`mesh.rs`)
   - `find_decal_bs` (`material.rs`)
   - `find_effect_shader_bs` (`material.rs`)
   All their responsibilities are now on `MaterialInfo`.
5. Patch `extract_material_info`'s BSEffect branch to cover the four
   drift gaps listed above so the shared path has full coverage.
6. Migrate the helper-specific tests to drive `extract_bs_tri_shape`
   end-to-end: verify observable `ImportedMesh.two_sided` /
   `ImportedMesh.is_decal` / `ImportedMesh.material_path` rather than
   implementation-detail helper outputs.

## Tests

- 7 tests in `bs_tri_shape_shader_flag_tests` (rewrite of the
  prior `two_sided_lookup_tests`): two_sided, is_decal, null / unrelated
  shader ref, effect-shader payload round-trip.
- 6 tests in `bgsm_path_tests` (rewrite of prior `find_material_*`
  tests): BGSM/BGEM path capture on both shader variants + negative case.
- All 340 existing nif unit tests + 1017 workspace tests pass.
- 100.00% clean parse across the four Skyrim+ games
  (`parse_rate_skyrim_se` + `_fallout_4` + `_fallout_76` +
  `_starfield`) = **142,384 NIFs**, zero failures.

## Files changed

- `crates/nif/src/import/material.rs` — add
  `extract_material_info_from_refs`, delete `find_decal_bs` +
  `find_effect_shader_bs`, patch BSEffect branch with the four drift
  fields.
- `crates/nif/src/import/mesh.rs` — rewrite `extract_bs_tri_shape` to
  call the shared path, delete `bs_tri_shape_two_sided` +
  `find_texture_path_bs_tri_shape` + `find_material_path_bs_tri_shape`,
  migrate two test modules to end-to-end coverage.
