# #5249: REN-D10-2026-10-05-01: #5192 returns the wrap-excess, back-light and translucency lobes to the unshadowed path for every light, sun and XCLL directional included (regression of #4946)

**Labels**: medium,renderer,shaders,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5249

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-05.md` — `REN-D10-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: MEDIUM
- **Dimension**: Disney BSDF / Soft Shadows
- **Location**:
  - `crates/renderer/shaders/include/lighting.glsl`, `shadowableLightRadiance`: `backSide`, `brdfTransmission`, `out vec3 transmissionRadiance`.
  - `crates/renderer/shaders/triangle.frag`: the ReSTIR finalize (`frameContribution = rad * restirW * visibility + restirSelectedTransmission * restirW`) and the legacy-WRS subtraction.
- **Status**: Regression of #4946. It is a deliberate trade-off in `a4b6a7360` (#5192), and no guard catches it.
- **Description**: #5192 split `shadowableLightRadiance` so that every lobe that is non-zero only where `rawNdotL < 0` leaves through `transmissionRadiance`:
  - the soft-light wrap excess;
  - the Skyrim back-light lobe (`bethesdaBackFactor`);
  - the FO4 `MAT_FLAG_TRANSLUCENCY` SSS lobe.

  Every consumer multiplies traced visibility into the *reflection* half only. The transmission half is never occluded, for any light.

  Before #4946, the same SSS lobe was added from unshadowed radiance and leaked "through walls and terrain shadow". The comment beside the lobe still says so. #4946 moved it into the shadowed function to stop exactly that. #5018 and #5192 then found that tracing it from the light-side origin self-occludes on closed meshes. Of the two fixes #5192's own issue offered, the commit chose "leave it unshadowed" over "trace with the fragment's own instance skipped".

  The commit's rationale is that "most reference-content local lights are not shadow casters". That contradicts the engine's own visibility policy: `VisibilityMask::for_legacy_local_light()` (`crates/core/src/lighting.rs`) returns `FULL` for every legacy light. The rationale also does not cover directional lights at all, and the sun and the interior XCLL key are always traced.
- **Evidence**:
  - The #4946 guard (shader_contract_tests.rs, "translucency lobe must be evaluated in shadowableLightRadiance") pins only the evaluation site.
  - `shadowable_light_radiance_splits_transmission_lobes_from_traced_visibility` pins the unshadowed transmission as intended behaviour.
  - The finalize adds `restirSelectedTransmission * restirW` with no `visibility` factor.
- **Impact**: Back-side transmission from the sun or the XCLL directional reaches surfaces in a real occluder's shadow. This affects:
  - FO4/FO76/Starfield BGSM translucent foliage and thick-translucency skin under a building or in terrain shadow;
  - Skyrim soft-lit (`MAT_FLAG_SOFT_LIGHTING`) and back-lit (`MAT_FLAG_BACK_LIGHTING`) NPCs inside a shadowed interior.

  The effect is visual only, bounded by the lobe magnitudes. The lobes now also feed `pHat` unshadowed, so reservoir selection favours lights behind walls for those materials.
- **Related**: #4946, #5018, #5192 (all closed); REN-D2-2026-10-03-01 / #5191 (front lobes, correctly clamped).
- **Suggested Fix**: Take #5192's documented alternative for the transmission half. Trace it with a candidate-loop query that skips the fragment's own `instanceCustomIndex`, giving up `TerminateOnFirstHit` on those rays only. That keeps the closed-mesh self-occlusion fix and restores real-occluder shadowing. At minimum, shadow the transmission half of directional lights. Gate the change on a before/after capture (see Needs-RenderDoc).

## Publisher note

Filed as a regression of the closed #4946: #5192 (`a4b6a7360`) deliberately moved the transmission lobes back out of traced visibility, re-opening the 'leaking through walls and terrain shadow' symptom #4946 fixed. The #4946 shader-contract guard only pins the evaluation site, so it stayed green.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
