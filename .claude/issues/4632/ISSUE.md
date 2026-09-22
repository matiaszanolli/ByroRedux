# NIFAL-D1-2026-09-21b-01: Skyrim/FO4 .btr distant-terrain normal maps are model-space and authored so, but bound through the tangent-space path

**Issue**: #4632
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIFAL_2026-09-21b.md)

**Severity**: HIGH. Rendering correctness on every Skyrim exterior's distant terrain, always on.
**Dimension**: Material (texture-only lowering). Owner: `/audit-exterior` (EXAL terrain LOD) + nifal
**Tier Violated**: no-leak and no-fabrication
**Game Affected**: Skyrim LE/SE (every worldspace with `.btr`). FO4 DiamondCity (85 `_n`-named flagged quads).
**Location**: `byroredux/src/cell_loader/terrain_lod_btr.rs:121-138` (doc), `:332-338` (bind), `:391` (`normal_has_alpha: false`), `:410-413` (`translate_texture_only_material`, no MSN bit). Shader: `crates/renderer/shaders/triangle.frag:599-628` (#3922 MSN branch). Render path: `byroredux/src/render/static_meshes.rs:534` / `:1019`.

## Description
Every normal-mapped Skyrim/FO4 `.btr` land shape authors `Model_Space_Normals` (SE 9,584/9,584, LE 4,416/4,416, FO4 8,271/8,271 — zero exceptions), but the spawner discards that authored bit: `terrain_lod_btr.rs` documents the map as tangent-space and lowers through `translate_texture_only_material`, which builds `Material` via `..Material::default()` and never touches `effect_shader_flags`. Every Skyrim exterior's distant terrain therefore samples a model-space map through the shader's tangent-space path.

## Evidence
- Texel basis (60 Tamriel level-4 quads): flat vertices read mean z −0.004 / mean |xy| 0.988 (tangent-space would read ~(0,0,1)); steep vertices best-match world (X, up, Y) with cos +0.657 vs +0.142 for tangent-space identity.
- Channel stats (`tamriel.4.0.36_n.dds`): R 141.6±24.8, G 245.8±9.8, B 145.1±27.6 — green-up model-space.
- Pre-existing since `d96110ebd` (2026-08-12, #2371); surfaced by the #4548 census, not by this delta's own commits.

## Impact
Distant terrain's shading normal leans ~69° off true up on flat ground; direct sun N·L swings from ~+0.9 to below zero depending on sun azimuth, across the whole distant landscape.

## Suggested Fix
Carry the `.btr` shape's authored MSN bit into its canonical `Material` (extend `translate_texture_only_material` or lower through `translate_material` proper), and run Phase-2 `resolve_msn_z_source` instead of hardcoding `normal_has_alpha: false`. The #2444 blocker the doc cites no longer applies (`.btr` has carried a `Material` since #3336).

## Related
#3922, #4548, #2371, #2444, #3336 (all closed); #4552 (open, same file). `docs/engine/exal.md:670` is stale (still lists `.btr` normal map as unbound).

## Source
docs/audits/AUDIT_NIFAL_2026-09-21b.md (NIFAL-D1-2026-09-21b-01)
