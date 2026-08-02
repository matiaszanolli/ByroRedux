# REN-D15-01: Water-side caustic splat refracts through the flat plane normal, not the wave normal

Severity: high
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2223

**Dimension**: 15 (Water)
**Location**: `crates/renderer/shaders/water.frag` — the `#1256` caustic block (`refract(-sunDir, Nsurface, 1.0 / 1.33)`, line ~669)
**Status**: NEW

**Description**: Caustics require refraction through *curved* (perturbed) geometry to focus
light; the block uses `Nsurface` (constant `(0,1,0)` for every fragment of a
flat water plane), not `Nperturbed`. The result is a rigid, structureless
translation of the water plane's screen footprint onto the floor — visually
indistinguishable from a lighting bug. The code's own header comment claims
it refracts through "the bumped water normal," which it does not.

**Evidence**: verified directly — `water.frag` defines both `Nsurface = normalize(vWorldNormal)` (line 396) and `Nperturbed = normalize(TBN * nMix)` (line 456, the wave-perturbed normal used everywhere else in the shader for reflection/refraction), but the caustic block's `refract()` call at line 669 passes `Nsurface`, not `Nperturbed`.

**Impact**: No caustic pattern can ever form under water — the floor projection is a rigid, undistorted copy of the water plane's silhouette instead of the characteristic focused-light caustic mesh.

**Related**: caustic_splat.comp (consumer of this projection)

**Suggested Fix**: swap `Nsurface` for `Nperturbed` in the `refract()` call and the Lambert weight; keep `Nsurface` only for the origin bias/side convention.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
