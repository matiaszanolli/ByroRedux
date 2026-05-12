# Audit: Renderer — Dimension 9: RT Ray Queries

**Date**: 2026-05-11
**Depth**: deep (single-dimension `/audit-renderer 9` → `--focus 9`)
**Scope**: `crates/renderer/shaders/triangle.frag` (every `rayQueryEXT` site + supporting helpers — `traceReflection`, `cosineWeightedHemisphere`, `buildOrthoBasis`, `concentricDiskSample`, `interleavedGradientNoise`)

## Executive Summary

**RT ray-query path is healthy at HEAD.** The 2026-05-09 sweep produced four substantive findings in this dimension (#789, #820, #916, #922) plus a follow-up fix (fallback-texture shimmer, d7604f0); all five are committed and verified in-tree today. Every checklist invariant — `TerminateOnFirstHit` flags on shadow + reflection + glass-refraction sites, Frisvad orthonormal basis at every per-fragment basis build, IGN noise seeded by `cameraPos.w` frame counter, `set 1 binding 2` TLAS binding, atomic `GLASS_RAY_COST = 4u` claim, fallback-texture skip in the IOR passthru loop — passes at the cited file:line.

**No new findings.** The dimension is effectively in maintenance mode; future regressions would surface here as the RT pipeline ages forward.

## RT Pipeline Assessment

All four ray-query sites (shadow, metal reflection, glass IOR refraction, GI bounce) are correctly structured:

- **Shadow rays** carry `TerminateOnFirstHit | Opaque` flags, jitter geometry uses concentric-disk sampling for point/spot lights and a 0.020-rad (~1.15°) cone for the directional sun, and `tMin = 0.05` matches the `N_bias × 0.05` origin offset.
- **Metal reflection rays** are gated on `metalness > 0.3 && roughness < 0.6` (PBR-coherent thresholds), use a V-aligned normal flip (#668), and apply roughness-squared cone jitter inside `traceReflection`.
- **GI bounce rays** correctly sample the cosine-weighted hemisphere off the *geometric* normal (not the perturbed bump-normal — comment at :2407 documents the Nellis regression that motivated this), with a 6000-unit tMax + smooth fade 4000→6000.
- **Glass IOR refraction** uses the Frisvad basis (#820), claims 4 atomic ray-budget slots per fragment (#916), skips same-texture and fallback-texture hits in the passthru loop (#789 + d7604f0), and exposes the loop terminus per-fragment via `DBG_VIZ_GLASS_PASSTHRU = 0x80`.

The universal `sceneFlags.x > 0.5` global RT gate fires at the top of every RT-ray block consistently. TLAS binding is canonical (`set 1, binding 2`).

## Rasterization Assessment

Not in scope for D9 — see D4 (Render Pass & G-Buffer), D6 (Shader Correctness), D11 (TAA).

## Findings

No new findings.

## What's NOT a bug

- **Shadow rays**: `crates/renderer/shaders/triangle.frag:2340-2350` — origin `fragWorldPos + N_bias × 0.05`, dir toward jittered light disk (point/spot) or sun cone (directional, 0.020 rad ≈ 1.15°), `concentricDiskSample` + `buildOrthoBasis(L,…)` at :2297-2298, `gl_RayFlagsTerminateOnFirstHitEXT | gl_RayFlagsOpaqueEXT` at :2344, `tMin = 0.05` matches bias. Pass.
- **Reflection rays (metal)**: `triangle.frag:1979-1999` — gated on `metalness > 0.3 && roughness < 0.6`, V-aligned normal flip (#668), `reflect(-V, N_view)` at :1991, roughness-squared cone jitter, `fragWorldPos + N_bias × 0.1` bias, terminate-on-first-hit inside `traceReflection` at :376. Pass.
- **Reflection hit math**: `triangle.frag:389-414` and `getHitUV` at :339-363 — `InstanceCustomIndex` (not `InstanceId`, see :386-388 comment), 25-float stride / UV offset 9, barycentric `w = 1 − u − v`, `texture(textures[nonuniformEXT(hitTexIdx)], hitUV)` at :406. Pass.
- **GI bounce rays**: `triangle.frag:2407-2435` — `cosineWeightedHemisphere(N_geom, n1, n2)` uses the geometric normal (not perturbed; :2407 comment explains the Nellis regression that motivated this choice), `buildOrthoBasis` inside the helper at :332, 6000-unit tMax with smooth fade 4000→6000 at :2376 (audit-spec said 1500 — actual code is 6000; the comment notes the previous 3000 was raised to match the fade end, well-reasoned), `terminateOnFirstHit` at :2422, NaN-free miss path returns `sceneFlags.yzw × 0.5` at :2472. Pass.
- **Glass IOR refraction**: Frisvad confirmed at :1609 (#820 closed), `GLASS_RAY_BUDGET = 8192u` at :1534, `GLASS_RAY_COST = 4u` atomic claim at :1539-1543 (#916 fixed in d8dbf94), passthru loop with `sameTexture` skip at :1684/1707 (#789 closed), `DBG_VIZ_GLASS_PASSTHRU = 0x80u` at :717 and viz block at :1727-1745, `fallbackTexture = (hInst.textureIndex == 0u)` skip at :1697/1755 (d7604f0). Pass.
- **IGN noise seeding**: `cameraPos.w` documented as frame counter at :153; consumed at :1595, :1992, :2083, :2292, :2381, plus per-slot reservoir offset at :2255-2257. Pass.
- **Universal**: TLAS at `set = 1, binding = 2` (:160), `sceneFlags.x > 0.5` global RT gate read into `rtEnabled` at :1032 and gating every RT-ray block (window portal :1426, glass IOR :1540, metal reflect :1979, reservoir shadows :2245, GI :2374). Pass.
- **Already-tracked, all closed/fixed**:
  - #789 (glass-passthru same-texture skip with sky→cell-ambient fallback) — CLOSED.
  - #820 (Frisvad orthonormal basis at IOR refraction) — CLOSED.
  - #916 (`GLASS_RAY_COST = 4u` atomic claim, replaces 2u) — FIXED in commit d8dbf94.
  - #922 (caustic source gate tightened to `material_kind == MATERIAL_KIND_GLASS`) — FIXED in commit 9eb387d.
  - Fallback-texture shimmer in IOR passthru loop — FIXED in commit d7604f0.

## Prioritized Fix Order

N/A — zero findings.
