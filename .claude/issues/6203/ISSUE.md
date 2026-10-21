# #6203 — INC-2026-10-20-02: Height-blended splat now feeds the terrain receiver's `d_ground` and Tier-3 detail density, while the blade scatter still evaluates the unblended field — breaks the documented "same density field" invariant

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6203
- **Labels**: medium,terrain-exterior,shaders,bug
- **Filed**: 2026-10-20 by /audit-publish

From `docs/audits/AUDIT_INCREMENTAL_2026-10-20.md` (`/audit-suite --preset quick`, HEAD c4df4c91e).

- **Severity**: MEDIUM (the split is partly deliberate: `triangle.frag:337-342` names Tier-3 as an intended consumer of the blended lanes. What is wrong is the broken invariant and the comments/doc that still state it. Visual-only impact.)
- **Dimension**: exterior Dim 3 (ground-cover pipeline) — silent divergence
- **Location**: `crates/renderer/shaders/triangle.frag:337-349` (in-place rewrite of `terrainSplat`), `:431-432` and `:2899-2900` (Tier-3 detail + canopy receiver read the rewritten lanes); `crates/renderer/shaders/groundcover_scatter.comp:219-224` (scatter density from `byroSampleTerrain`, unblended)
- **Changed in**: `crates/renderer/shaders/triangle.frag` (commit ea45af067)
- **Status**: NEW
- **Description**: `triangle.frag:2875-2880` states that the receiver evaluates "the *same* `byroGcDensityGround` the scatter runs ... so the shadow's density and the density of the sward casting it cannot disagree", and that the interpolated splat weights "are the same numbers `byroSampleTerrain` would return". `exal-groundcover.md:1339` says the same. After ea45af067 the fragment-side lanes are height-reweighted before both density evaluations. `groundcover_scatter.comp`, `groundcover_blade.vert` and `groundcover_models.comp` still sample raw splat weights through `byroSampleTerrain`.
- **Evidence**: `triangle.frag:345-349` calls `byroTerrainHeightBlend(terrainSplat[0], terrainSplat[1], ...)`. `:431` `gcDetailSample.splat0 = terrainSplat[0];`. `:2899` `gcSample.splat0 = terrainSplat[0];`. `groundcover_scatter.comp:219` `TerrainSample s = byroSampleTerrain(...)` with no height blend.
- **Impact**: Where the blend reshuffles shares between a grassy lane and a bare lane (rock poking through dirt, which is the feature's purpose), contact darkening and Tier-3 species detail follow the blended surface while blades are still placed by the painted weights. The result is shadowed patches with no blades, or blades over undarkened ground. The comments and doc above are now false. With -01 this mismatch also inherits the rim seam.
- **Suggested Fix**: Decide which field is canonical. Either feed the receiver/Tier-3 the pre-blend lanes (keep a copy before line 345), or apply the same `byroTerrainHeightBlend` inside `byroSampleTerrain` consumers (scatter/models) so all density evaluations agree. Then correct `triangle.frag:2875-2880` and `exal-groundcover.md` §"How the terrain receiver gets `d_ground`".

## Completeness Checks
- [ ] **SIBLING**: every density consumer agrees — `groundcover_scatter.comp`, `groundcover_models.comp`, `groundcover_blade.vert`, the `triangle.frag` receiver and Tier-3 detail
- [ ] **TESTS**: A source/shader-contract test pins which splat field (blended or raw) each density consumer reads
