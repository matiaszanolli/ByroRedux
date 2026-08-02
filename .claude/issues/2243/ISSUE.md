# REN-D17-01: disneyDiffuseSplit's sheen weight disagrees by pi between its two call sites

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2243

**Dimension**: 17 (BRDF)
**Location**: `crates/renderer/shaders/include/pbr.glsl:147` (`disneyDiffuseSplit`, doc comment: "diffuse: already /PI'd... sheen: NOT /PI'd"); call sites at `crates/renderer/shaders/include/lighting.glsl:153` and `crates/renderer/shaders/triangle.frag:2305`
**Status**: NEW

**Description**: `disneyDiffuseSplit`'s own contract states `diffuse` is already divided by PI and `sheen` is not, so callers must add them directly. `lighting.glsl:157` does `diffuseBrdf = (dd.diffuse * PI + dd.sheen) * (1.0 - metalness)` — multiplying `dd.diffuse` back up by PI to match that function's non-normalized else-branch convention (`kD * albedo`, no `/PI`), but *not* similarly scaling `dd.sheen`. `triangle.frag:2305`'s direct-sun path does `diffuseBrdf = (dd.diffuse + dd.sheen) * (1.0 - metalness)` with no PI rescaling, matching its own `/PI`-normalized else-branch (`kD * albedo / PI`). Because only one of the two sites rescales `diffuse` by PI without correspondingly rescaling `sheen`, sheen's relative weight against diffuse differs by a factor of PI between the clustered-lighting path (`lighting.glsl`) and the direct-sun path (`triangle.frag`).

**Impact**: Sheen (cloth/fabric edge highlight) renders inconsistently — roughly pi times weaker relative to diffuse under clustered point/spot lights than under direct sunlight, for the same authored `sheen` value.

**Suggested Fix**: pick one convention (either both call sites use the PI-normalized shape, or both use the non-normalized shape) and scale `dd.sheen` consistently with whichever scaling is applied to `dd.diffuse` at each site.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
