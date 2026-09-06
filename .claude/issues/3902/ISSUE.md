# #3902: REN-2026-09-05-D7-01: secondary-ray albedo drops every albedo-modifying texture role — RT reflections shade a different colour than raster

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3902 --json state`).*

---

**Audit**: `docs/audits/AUDIT_RENDERER_2026-09-05_DIM6_DIM7.md` (suite preset `texture-roles-deep`, **narrowed run — dims 6 & 7 only**)
**Severity**: MEDIUM · **Dimension**: 7 (material table)

## Description

`rayHitAlbedo` (`crates/renderer/shaders/include/ray_hit.glsl`) applies only the constant diffuse tint. **Five albedo-modifying texture roles that the raster path composes are read by no secondary ray**: `decals[0..3]`, `tint`, `inner_layer`, `dark`, `detail`.

RT reflections, GI and water refraction therefore shade a *different surface colour* than the raster pass shading the same surface.

## Evidence

`rayHitAlbedo` samples the base colour and applies the constant tint, with no sampling of the five supplemental albedo lanes that the main geometry pass composes into `texColor`.

The divergence is **per-game by construction**, which is what makes it a role-unification finding rather than a generic RT gap:
- `dark` — Oblivion / Gamebryo era
- `tint` — Skyrim / FO4 tint family
- `decals[0..3]` — legacy overlays

The decal half additionally modifies `texColor.a`, and `rayHitHasCoverage` derives ray coverage from `baseSample.a` — so the divergence is not purely colour, it reaches coverage too.

Unlike the shader variant stubs that `triangle.frag` explicitly parks with a deferral note, **no deferral note exists here** — so this reads as an oversight rather than a deliberate scope cut.

## Impact

A surface's reflection does not match the surface. On the games where these roles are dense (Oblivion `dark`, Skyrim/FO4 `tint`, legacy decals) a mirror, a polished floor or a water surface shows a visibly different colour than the geometry it reflects. Because the affected roles differ per game, the artifact appears in one title and not another — the signature failure mode the role seam was introduced to prevent.

## Suggested Fix

Either compose the five albedo-modifying roles into `rayHitAlbedo` the way the raster path does, or — if the cost is deliberate — add an explicit deferral note naming the five roles and the reason, matching how `triangle.frag` parks its variant stubs. The coverage half (`baseSample.a` via decals) should be decided together with the colour half.

## Completeness Checks
- [ ] **SIBLING**: Every secondary-ray consumer checked (reflection, GI, water refraction), not just the reflection path
- [ ] **CANONICAL-BOUNDARY**: No per-game branching introduced into the shader — the roles are already game-agnostic at this point. See `/audit-nifal`.
- [ ] **TESTS**: A regression test or shader-source pin covers the albedo lanes reaching the ray path

## Related
- #2712 (the shader-consumption guard — see companion LOW on its coverage)

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
