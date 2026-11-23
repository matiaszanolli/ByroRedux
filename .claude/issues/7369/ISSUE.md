# #7369 — REN-D2-2026-11-23-01: RLT-IBL-004's sky-occlusion ray treats alpha-tested foliage, fences and grates as solid

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D2-01). Reported by reading: nothing here was run on a GPU.

- **Severity**: MEDIUM
- **Dimension**: Ray Queries
- **Location**: `triangle.frag` `main()` (the `exteriorSkyProbe && probeShare > 0.0 && reflectionGlassRayEnabled && !helperLane && !hasAuthoredCubemap` block, calling `traceShadowBinary(..., VISIBILITY_MASK_ALL_OPAQUE)`); `include/shadow_common.glsl` `traceShadowBinary`; contrast `include/shadow_transport.glsl` `traceShadowTransmittanceDetailed`
- **Status**: NEW (introduced by 5f51748d5, RLT-IBL-004 rev 49; the #7269 follow-up did not touch it)
- **Description**: `traceShadowBinary` initialises its ray query with `gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT` and never inspects the hit, so every instance whose visibility layer is in `VISIBILITY_MASK_ALL_OPAQUE` (15 = architecture | static prop | dynamic actor | foliage) is a solid occluder, including cutout cards (alpha-tested leaves, fences, grates). The direct-light path does not do this: `traceShadowTransmittanceDetailed` is "an alpha-aware binary blocker" (`alphaSensitive = hitMat.alphaThreshold > 0.0 || INSTANCE_FLAG_ALPHA_BLEND`, advances past uncovered texels), and `traceWaterRay`/`traceReflection` document the same rule ("Alpha-tested leaves, grates, and other cutouts must not become solid slabs"). The sky ray replaced SSAO, whose depth buffer only contains the *alpha-passing* texels, so under a canopy or beside a fence the old form saw the real aperture and the new one sees a full quad per card.
- **Evidence**: `traceShadowBinary` body (`rayQueryInitializeEXT(rq, topLevelAS, gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, mask, ...)`, returns `rayQueryGetIntersectionTypeEXT != None`); call site passes `VISIBILITY_MASK_ALL_OPAQUE`; `VISIBILITY_LAYER_FOLIAGE` = 8 is inside that mask. The PRD's own A6 contact-sky rung uses rigid casters only; RLT-CON-008's alpha-tested caster is an A5 (direct-shadow) row. The sun ray through the same canopy is dappled while the sky specular is darkened as if the canopy were solid.
- **Impact**: exterior glossy surfaces (wet/metal/rough-dielectric sky probe share) under vegetation, fences and grates lose their sky specular wherever a lobe sample crosses a card's *quad*, not its coverage. Magnitude is bounded by the probe share times `lobeWeight` (about 1.0-1.2 for roughness <= 0.5, up to ~3 at roughness 1; computed from the generated LUT), and is stochastic, so TAA smooths it into a darkened, not noisy, look. Visual only; no corruption or hang.
- **Related**: PRD `docs/prd/reference-lighting.md` RLT-IBL-004 / rev 49; RLT-CON-008; #7269 (same block, authored cubemaps).
- **Suggested Fix**: use the alpha-aware walk the direct path already has (a 2-3 layer cap is enough for a sky ray), or as a cheaper first cut drop `VISIBILITY_LAYER_FOLIAGE` from the mask and add an alpha-tested caster row to the contact-sky A6 rung so the choice is measured. Extend `environment_specular_is_the_exact_split_sum_with_probe_occlusion` to assert whichever is chosen.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **SHADER**: A GLSL edit recompiles the SPIR-V, including `triangle_early.frag.spv` (`scripts/check-shader-artifacts.sh` is the gate)
- [ ] **TESTS**: A regression test pins this specific fix
