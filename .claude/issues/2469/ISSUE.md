# REN-D15-NEW-02: foamFlowStreaks still hashes absolute world coordinates -- the #1502/#1997 precision rebase covered only sampleScrollingNormal

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2469
**Finding ID**: REN-D15-NEW-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 15 — Water
**Location**: `crates/renderer/shaders/water.frag:376-390` (`foamFlowStreaks`), called at `:606` / `:609` / `:616`; contrast with `sampleScrollingNormal` (`:179-227`) and the `uvOrigin` plumbing at `:434-449`
**Status**: NEW (sibling gap left by the #1997 fix for #1502)

## Description
#1997 fixed the documented #1502 precision bound by threading a render-origin offset (`uvOrigin`) into `sampleScrollingNormal` and subtracting it before the `hash21` lattice: `vec2 uv = (uvBase - originOffset) * scale + scroll * time;`. The *other* absolute-world consumer of the same `valueNoise`/`hash21` lattice — `foamFlowStreaks` — was not rebased. It is called with the raw absolute `vWorldPos` and projects it onto the flow axis:
```glsl
float u = dot(worldPos, flowDir) - speed * time;   // worldPos ABSOLUTE
float v = dot(worldPos, perp);
float streak = valueNoise(vec2(u * 0.04, v * 0.18));
```
`hash21` does `p = fract(p * vec2(443.897, 441.423))`. At Tamriel-scale coordinates (~±233k units) `u * 0.04 ≈ 9.3e3`, so `p.x * 443.897 ≈ 4.1e6` — beyond fp32's 24-bit mantissa for a meaningful fractional part (resolution ≈ 0.25), and `fract()` collapses to a handful of discrete values. At FNV Mojave far cells (up to ~57k) the product is ~1.0e6, resolution ≈ 0.06 — already visibly degraded. The `uvOrigin` value needed for the fix is already computed in `main()` at `:434-449` for both the flat-plane and waterfall branches; it simply isn't passed to `foamFlowStreaks`.

## Evidence
`foamFlowStreaks(vec3 worldPos, float time)` takes no origin parameter; all three call sites pass `vWorldPos` (absolute). Reachability is real: `byroredux/src/env_translate.rs:140/147` assigns `WaterKind::Rapids` and `WaterKind::River` from WATR classification, and both branches call `foamFlowStreaks`; the waterfall branch calls it too.

## Impact
Visual-only — rivers/rapids/waterfalls in distant exterior cells lose their animated whitewater streaks (frozen or blocky mask). No NaN, no crash, no CPU-side effect. Reachable in exactly the worldspaces where rivers are most common.

## Related
#1502, #1997; `AUDIT_RENDERER_2026-07-15_DIM15.md` REN-D15-01 (the finding that produced the `sampleScrollingNormal` half of the fix).

## Suggested Fix
Add an `originOffset` (or `vec3 renderOriginXYZ`) parameter to `foamFlowStreaks` and subtract it from `worldPos` before the `dot(...)` projections. Same one-line pattern as the `sampleScrollingNormal` rebase; the origin is already in the camera UBO.

## Completeness Checks
- [ ] **TESTS**: A precision regression test (mirroring the `sampleScrollingNormal` rebase's guard) confirms `foamFlowStreaks` doesn't collapse at Tamriel-scale coordinates
