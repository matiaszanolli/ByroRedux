# REN-D8-2026-09-21-01: the glass-caustic occlusion gate's NDC-space depth slop grows ~d²/near — occluders within 23 % of the receiver distance pass at 300 BU

**Labels**: low, renderer, shaders, bug

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: LOW (gate precision; caustic energy lands on occluders) · **Dimension**: Caustics
**Location**: `crates/renderer/shaders/caustic_splat.comp` (~:612-631): `float depthSlop = 1.0e-4 + ndc.z * 5.0e-3; if (depthIsInFront(receiverZ, ndc.z) && abs(receiverZ - ndc.z) > depthSlop) continue;`
**Status**: NEW (introduced by `0418ac768`, the #4545 glass sibling)
**Verified against**: HEAD `f97775ca8`

## Description

`0418ac768` gates glass-caustic deposits on whether the landing pixel is visible. It compares the receiver hit's NDC depth with the opaque depth buffer at the landing pixel, using a slop of `1e-4 + ndc.z * 5e-3` in raw NDC. The renderer uses conventional, not reversed, depth (`BYRO_REVERSED_Z 0`). With conventional depth, NDC z ≈ 1 − near/d, so a fixed NDC slop becomes a view-space tolerance that grows roughly as d²/near. An occluder at view distance d_o in front of a receiver at d is rejected only when (d − d_o)/d_o > slop · d / near.

The comment claims "real occlusion events are orders of magnitude larger in encoded depth". That holds only near the camera.

## Evidence

How far in front of the receiver an occluder must be before the gate rejects it, as a fraction of d (computed from the shader's constants):

| Content | near | d | Occluders missed within |
|---|---|---|---|
| BU content (`NEAR_PLANE_BU_SCALE` = 5) | 5 | 50 BU | ~4 % of d |
| | 5 | 100 BU | ~9 % of d |
| | 5 | 300 BU | ~23 % of d |
| Cornell box | 0.1 | 10 units | ~34 % of d |

On the Cornell box the tolerance is already 34 % at 10 units. The commit's Cornell check could therefore only show that the gate does not over-reject, not that it catches occluders.

## Impact

The motivating case, "a bottle standing between the camera and its own table pool", is not rejected beyond about 50-100 BU. Past that, the bottle's front face receives the caustic deposit meant for the table. Visual only, glass caustics.

## Related

- #4545 (closed): the water-side twin, a camera-visibility ray. REN-D8-2026-09-21-02 (#4589) covers that twin's mask.
- `0418ac768`: "Gate glass caustic deposits on landing-pixel visibility (#4545 sibling)".

## Suggested Fix

Linearize both depths with `depthLinearize(z, nearPlane, farPlane)` (`include/depth_convention.glsl`). Then compare view-space distances with a relative tolerance: reject when the linear depth at the pixel is less than `d_hit * (1 − ε)`, with ε of a few percent plus the ±8 px jitter allowance. Add a CPU mirror test at 50, 300 and 1000 BU.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D8-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: other raw-NDC depth comparisons with a fixed slop checked for the same d²/near growth
- [ ] **SIBLING**: `caustic_splat.comp.spv` recompiled; `scripts/check-shader-artifacts.sh` green
- [ ] **TESTS**: a mirror test that an occluder 5 % in front of the receiver is rejected at 50, 300 and 1000 BU
