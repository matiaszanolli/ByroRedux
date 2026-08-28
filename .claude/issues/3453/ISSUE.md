# Issue #3453: REN-2026-08-27-D17-02: shadowableLightRadiance's doc block now sits above three unrelated helpers inserted between it and the function it documents

**Filed**: 2026-08-27 via /audit-publish from `docs/audits/AUDIT_RENDERER_2026-08-27.md`

**Severity**: LOW
**Dimension**: Disney BSDF / PBR gating
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-27.md` (REN-2026-08-27-D17-02)
**Status**: NEW — introduced 2026-08-25 alongside the Bethesda lighting lobes.

## Location
`crates/renderer/shaders/include/lighting.glsl:80-92` (the displaced comment block), `:127` (`shadowableLightRadiance`)

## Description
The block beginning *"Direct Cook-Torrance contribution of cluster light `i` at this fragment — exactly the `brdfResult * unshadowedRadiance` the WRS streaming pass accumulates …"* documents `shadowableLightRadiance`, but the three new `bethesdaDiffuseLightFactor` / `bethesdaRimFactor` / `bethesdaBackFactor` helpers were inserted between the comment (ending line 91) and the function (now at line 127). As written, the comment reads as documentation for `bethesdaDiffuseLightFactor`.

## Evidence
`lighting.glsl:91` is the comment's last line; `:92` opens `vec3 bethesdaDiffuseLightFactor(`; `:127` opens `vec3 shadowableLightRadiance(` with no doc block of its own.

## Impact
Documentation only. It matters slightly more than usual because the displaced paragraph is the one stating the bit-for-bit accumulate-then-subtract invariant that gates every future edit to this function — an invariant the audit separately confirmed still holds (all five `shadowableLightRadiance` call sites in `triangle.frag` pass identical `lightingMask` / `backLightingMap` arguments).

## Related
#1369

## Suggested Fix
Move the three helpers above the comment block, or move the block down to immediately precede `shadowableLightRadiance`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other include headers where helpers were inserted mid-file)
- [ ] **SPV**: Comment-only moves must still leave every dependent `.spv` byte-identical after a plain `glslangValidator -V` recompile
