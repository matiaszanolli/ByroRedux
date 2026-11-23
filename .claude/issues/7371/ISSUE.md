# #7371 — REN-D2-2026-11-23-02: The environment-specular term no longer reads the material's `fresnelPower`; a dead `F` local remains

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D2-02). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW
- **Dimension**: Ray Queries (shading consistency)
- **Location**: `triangle.frag` `main()` environment block (`vec3 F = fresnelSchlickPower(NdotV, F0, mat.fresnelPower);` then `environmentWeight = F0 * envAB.x + envAB.y`); `include/lighting.glsl` `shadowableLightRadiance` (still uses `mat.fresnelPower`), `triangle.frag` `primaryDiffuseWeight` (still uses it)
- **Status**: NEW (introduced by 84104564e, RLT-IBL-003; owner-approved split sum, but the fresnelPower consequence is not recorded in the PRD, `grep fresnelPower docs/prd` is empty)
- **Description**: before RLT-IBL-003 the env term was `envColor * F * (1 - roughness)` with `F = fresnelSchlickPower(NdotV, F0, mat.fresnelPower)`. The split sum bakes the standard (1-VH)^5 Schlick into the LUT, so an authored `fresnel_power` (BGSM / BSLightingShaderProperty / material translate carry non-default values, e.g. 3.5, 4.5; default 5.0) now changes the light-loop specular and the diffuse weight but not the environment specular. `F` at the top of the block is dead (the compiler drops it) and reads as if it still mattered.
- **Evidence**: `fresnelPower` consumers at HEAD: `lighting.glsl` light loop, `triangle.frag` glass scalar Fresnel, eye Fresnel, `primaryDiffuseWeight`; absent from the environment `Lo +=`.
- **Impact**: for materials with non-default power the three lobes (diffuse weight `1 - F(p)`, light-loop spec F(p), env spec F(5)) stop agreeing on one Fresnel curve. Small and only off-default; also a silent change to authored-content response.
- **Related**: RLT-IBL-003; `docs/engine/nifal.md` lists `fresnel_power` as carried.
- **Suggested Fix**: owner decision: either record "authored fresnel power does not reach the environment split sum" in the PRD / nifal.md and delete the dead `F`, or fold it in (e.g. blend the LUT `B` term by the power). Measure on a non-default-power FO4 BGSM first.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **SHADER**: A GLSL edit recompiles the SPIR-V, including `triangle_early.frag.spv` (`scripts/check-shader-artifacts.sh` is the gate)
- [ ] **TESTS**: A regression test pins this specific fix
