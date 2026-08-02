# REN-D19-01: perturbNormal's screen-space derivative fallback double-flips handedness on mirrored UVs

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2245

**Dimension**: 19 (Tangent space)
**Location**: `crates/renderer/shaders/include/material_sampling.glsl:134` (`perturbNormal`, Path 2 screen-space fallback, line ~170-193)
**Status**: NEW

**Description**: The Path-2 (no-authored-tangent) fallback derives `T` directly from position/UV screen-space derivatives (`T = normalize(dPdx * dUVdy.y - dPdy * dUVdx.y)`, the un-divided numerator of the standard tangent formula, whose own sign already implicitly reflects the UV-Jacobian determinant sign), then separately computes `screenSign = sign(dUVdx.x * dUVdy.y - dUVdx.y * dUVdy.x)` and applies it again via `B = screenSign * cross(N, T)`. Because `T`'s own derivation already carries the determinant's sign (it isn't divided out), applying `screenSign` a second time when deriving `B` double-counts the mirrored-UV handedness correction, per the same class of defect `#1104` (REN-D16-002) fixed for the authored-tangent path. Affects terrain and every renderer-synthetic-tangent mesh — critical for Starfield, since `BSGeometry` tangents are empty until #1086 lands an extractor, so every Starfield mesh reaches this fallback.

**Impact**: Normal-mapped detail on mirrored-UV regions of terrain and any renderer-synthesized-tangent geometry (all current Starfield meshes) has incorrect tangent-space handedness, producing inverted-looking bump/normal detail on the mirrored half of a symmetric UV layout.

**Related**: #1104 / REN-D16-002 (the analogous, already-fixed defect on the authored-tangent path)

**Suggested Fix**: verify whether the raw derivative `T` already needs no additional `screenSign` correction (since its own sign encodes the determinant), and remove the redundant second application if so — matching whatever resolution #1104 used for the authored-tangent path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
