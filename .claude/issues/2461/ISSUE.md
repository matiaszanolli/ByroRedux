# REN-D2-2026-08-07-01: GI hemisphere axis is not viewer-flipped for two-sided back faces, while its ray origin is

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2461
**Finding ID**: REN-D2-2026-08-07-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 2 — Ray Queries / RT Shading
**Location**: `crates/renderer/shaders/triangle.frag` — one-bounce GI block (`N_geom` / `giDir` / `giOrigin`, guarded by `rtLOD < RT_LOD_GI`); interacts with the `gl_FrontFacing` flip applied to `N` near `terrainGeometryNormal` and with `N_bias`
**Status**: NEW

## Description
The two-sided back-face flip is applied only to the shading normal `N`:
```glsl
vec3 N = normalize(fragNormalEffective);
if (!gl_FrontFacing) { N = -N; }          // flips N only
vec3 terrainGeometryNormal = N;
...
vec3 N_bias = dot(N, V) < 0.0 ? -N : N;   // always viewer-facing
```
`fragNormalEffective` itself is never re-oriented. The GI path then builds its hemisphere around the *unflipped* value while biasing the origin along the *flipped* one:
```glsl
vec3 N_geom  = normalize(fragNormalEffective);       // NOT viewer-flipped
vec3 giDir   = cosineWeightedHemisphere(N_geom, n1, n2);
vec3 giOrigin = fragWorldPos + N_bias * 0.1;         // viewer-flipped
```
For a fragment of a two-sided draw (foliage/vine/grass cards, curtains, some architecture) rendered from its back side, `gl_FrontFacing == false`, so `N_bias` points toward the viewer while `N_geom` points away. Every cosine-weighted `giDir` therefore has a positive component *through* the surface plane, starting from an origin offset 0.1 units on the opposite side. Every *other* `fragNormalEffective` consumer in this shader applies the viewer-orientation flip locally — the fire-refraction branch (`macroN`) and the glass branch (`glassViewNormal`) — the GI site is the one that does not.

## Evidence
With tMin `0.05` and an origin `0.1` off the plane, the plane crossing occurs at `t ≈ 0.1 / dot(giDir, planeNormal)` — typically `t ≈ 0.15`, comfortably inside `[tMin, 6000]`. The fragment's own triangle is in the TLAS (two-sided draws are not excluded), so it is a committable candidate. On commit `rtAO = mix(0.3, 1.0, smoothstep(60.0, 500.0, pathDistance))` with `pathDistance ≈ 0.15` pins `rtAO` at its 0.3 floor, and `pathRadiance` accumulates the back side of the same card instead of the room.

## Impact
Back-facing fragments of two-sided draws get indirect light gathered from the wrong hemisphere and an AO term clamped near the 0.3 floor. Symptom class: darkened/AO-crushed back faces on vines, grass cards, curtains and two-sided architecture, and a front/back GI discontinuity on the same card. Blast radius is limited to two-sided draws, which is why this has survived: single-sided geometry is back-face culled so `gl_FrontFacing` is always true and the code path is a no-op there. Magnitude on real cells **needs RenderDoc / visual verification** — the logic inconsistency is definite, the pixel-level severity is not measured.

## Related
Same normal-orientation family as #668 (RT-3, V-aligned flip on metal reflection), #733 (RT-11, hoisting `N_bias`), #821 / REN-D9-NEW-02 (documented intentional asymmetry for the window-portal ray — that one is deliberate; this one is not documented as such).

## Suggested Fix
Orient the GI hemisphere axis toward the viewer at the GI site, mirroring the fire-refraction and glass branches: `vec3 N_geom = normalize(fragNormalEffective); if (dot(N_geom, V) < 0.0) N_geom = -N_geom;` (or hoist a single `fragNormalEffectiveView` next to `N_bias` and switch all four consumers to it). Leave the ReSTIR `geomN` / `rc.pad0` pair alone unless both sides change together.

## Completeness Checks
- [ ] **SIBLING**: Confirm the fire-refraction and glass branches' flip logic is the correct pattern to mirror, and check no other `fragNormalEffective` consumer has the same gap
- [ ] **TESTS**: Needs RenderDoc capture of a two-sided back-facing GI fragment before/after; not unit-testable
