# REN-D16-2026-08-07-01: Integrated froxel volume stores slab-BACK-face cumulative state but composite samples it at texel CENTER -- half-slab forward fog bias

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2470
**Finding ID**: REN-D16-2026-08-07-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 16 — Volumetrics
**Location**: `crates/renderer/shaders/volumetrics_integrate.comp:main` / `crates/renderer/shaders/composite.frag:hybridSliceCoordinate`
**Status**: NEW (adjacent to, but distinct from, #1462 — that fix moved the *injection* sample from slice-CENTER to slice-FRONT-EDGE; the integrate→composite texel-center mapping was not touched)

## Description
`volumetrics_integrate.comp` accumulates a slab and then writes the post-slab cumulative state into texel index `slice`: `inscatter_total += inscatter * trans_cumulative * dt; trans_cumulative *= exp(-extinction*dt); imageStore(integrated, ivec3(col, slice), vec4(inscatter_total, trans_cumulative));` The stored value therefore physically lives at normalized depth `u = (slice+1)/N` (the slab's back face — the shader comment says so explicitly). But `composite.frag` fetches with `texture(volumetricFroxel, vec3(fragUV, slice))` where `slice = hybridSliceCoordinate(...)` returns a plain normalized `[0,1]` depth, and a `sampler3D` places texel `k` at `u = (k+0.5)/N`. The lookup for a fragment truly at `u = (k+1)/N` lands halfway between texel `k` and `k+1`, i.e. it returns roughly the cumulative state of a point half a slab deeper than the fragment. There is no `-0.5/N` (or `+0.5/N`) texel-center correction on either side.

## Evidence
- `volumetrics_integrate.comp:101-119` — `sliceFront = slice/size.z`, `sliceBack = (slice+1)/size.z`, `dt = sliceDistance(sliceBack) - sliceDistance(sliceFront)`, store at `ivec3(col, slice)`.
- `composite.frag:492-493` — `float slice = hybridSliceCoordinate(min(worldDist, gridFar)); vec4 vol = texture(volumetricFroxel, vec3(fragUV, slice));`
- `composite.frag:99-109` — `hybridSliceCoordinate` returns exactly the normalized-depth `u`, with no `(u*N - 0.5)/N` re-centering.

## Impact
Systematic over-application of fog: every fragment is attenuated by (and receives inscatter from) roughly half a slab of extra medium. Worst in the near field, where the hybrid-Z distribution is linear and the first `LINEAR_SLICE_FRACTION = 0.125` of slices cover `LINEAR_DEPTH = 350` world units — with 64 slices that is 8 linear slices of ~44 world units each, so near-camera fragments are biased by ~22 world units of medium. Also softens/advances god-shaft boundaries by half a slab along the view ray. Blast radius: every fog-bearing cell; it is a bias, not a crash, so it is invisible to `cargo test`.

## Related
#1462 (inject slice-center → front-edge reconciliation), #928 (`VOLUMETRIC_OUTPUT_CONSUMED`).

## Suggested Fix
Pick one convention and apply it on both ends. Cheapest: in `composite.frag`, convert the normalized depth to a texel-aligned coordinate for the back-face convention — `slice_tc = clamp((u * N - 0.5) / N, 0.0, 1.0)` with `N` the slice count (already plumbed into `IntegrationParams.grid.w`; expose the same to composite). Alternatively have `integrate` store at the slab center so the texel-center fetch is already correct — but that then needs `inject`'s front-edge convention (#1462) re-reconciled.

## Completeness Checks
- [ ] **TESTS**: A regression test pins the corrected texel-alignment convention (or documents the chosen convention with a comment tying inject/integrate/composite together)
