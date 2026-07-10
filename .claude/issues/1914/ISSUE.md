# REN-D2-01: RL-03 per-light ambient fill is missing its stated point/spot gate — the exterior sun injects an unshadowed, normal-independent fill on every fragment

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1914

**Severity**: medium
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/shaders/triangle.frag:2191-2197` (`LIGHT_AMBIENT_FILL_FACTOR` site, cluster-light loop)
**Status**: NEW

## Description
The RL-03 fill's contract comment says "point/spot only; ... a true directional sun has no 'ambient' component in the Gamebryo model", but the code has no `lightType` gate. For the exterior sun (type 2.0, radius 0.0, not the interior `-1` sentinel): `cluster_cull.comp` inserts directional lights into every cluster; in the fragment loop the type-2 arm sets `atten = 1.0`, `isInteriorFill = radius < 0.0` is false (0.0 ≥ 0), so execution falls into `lightAmbientFill = max(..., lightColor * atten * albedo * vec3(mat.ambientR/G/B) * 0.15)`. `mat.ambient*` defaults to `[1,1,1]`, so every exterior fragment receives an unshadowed `sunColor × albedo × 0.15` term added directly to `Lo` — it bypasses the entire ReSTIR/WRS shadow machinery and ignores N·L.

## Evidence
`triangle.frag:2102-2107` (directional arm, `atten = 1.0`) → `:2133` (`isInteriorFill = radius < 0.0`) → `:2191-2197` (fill, no `lightType < 1.5` test) vs. the contract comment at `:2159-2162`. Introduced by commit `977eb95a` (mislabeled "Add Scripting Subsystem Audit report").

## Impact
Exterior daytime scenes get all RT sun shadows lifted by ~15% of sun radiance × albedo (unshadowed, view/normal-independent), washing out shadow contrast and double-counting with the DALC/sky ambient. Interiors unaffected (directional `continue`s at :2156).

## Related
RL-03 (this commit's own tag); prior per-light fill removal note at `triangle.frag:2276`

## Suggested Fix
Add the gate the comment already promises — skip the fill for `lightType >= 1.5` (or hoist it inside the point/spot arms). One-line shader change + recompile; confirm on an exterior bench (`--grid`) A/B.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
