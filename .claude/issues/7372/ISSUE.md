# #7372 — REN-D10-2026-11-23-01: The environment term's multi-scatter compensation still reads Karis's 2014 fit, not the exact table the same term uses

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D10-01). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW
- **Dimension**: Disney BSDF
- **Location**: `triangle.frag` `main()` environment block (`vec2 envAB = envBRDFLut(NdotV, roughness); ... environmentWeight *= multiScatterEnergyCompensation(F0, NdotV, roughness);`); `include/pbr.glsl` `multiScatterEnergyCompensation` (its `Ess` is `envBRDFApprox`'s `ab.x + ab.y`)
- **Status**: NEW (introduced as a consistency gap by 84104564e, RLT-IBL-003; `gh issue list --state all --search "multi-scatter energy compensation"` and `"multiScatterEnergyCompensation envBRDFApprox"` return nothing)
- **Description**: RLT-IBL-003 replaced the env weight with `F0 * A + B` from the exactly integrated table (separable exact Smith G1, `scripts/gen_env_brdf_lut.py`) because the 2014 fit "measured 0.83-1.16 of the exact weights". The multiple-scatter factor `1 + F0 * (1/Ess - 1)` applied to that same weight still takes `Ess` from the fit. Fdez-Aguera / Filament require `Ess` to be the directional albedo of the SAME single-scatter lobe the weight comes from; with that, a white (F0 = 1) surface integrates to exactly 1. Here the env weight at F0 = 1 is `Ess_exact * (1 / Ess_fit)` (the table's `A + B` times the fit's compensation), i.e. `Ess_exact / Ess_fit`, which is not 1. `envAB` is already in scope, so the exact `Ess = envAB.x + envAB.y` is free at that site.
- **Evidence**: I interpolated `ENV_BRDF_LUT` (the generated `scene_buffer/env_brdf_lut.rs` table, same bilinear read as `envBRDFLut`) against the `envBRDFApprox` constants. White-furnace gain of the env term at F0 = 1, `Ess_exact / Ess_fit` (1.0 = conserving), sampled at NdotV 0.1 / 0.3 / 0.5 / 0.8 / 1.0: roughness 0.1: 1.05 flat; 0.3: 1.05 / 1.13 / 1.17 / 1.18 / 1.19; 0.5: 1.18 / 1.15 / 1.18 / 1.24 / 1.26; 0.7: 1.26 / 1.17 / 1.13 / 1.12 / 1.13; 0.9: 1.26 / 1.11 / 1.00 / 0.89 / 0.84; 1.0: 1.27 / 1.08 / 0.94 / 0.79 / 0.72. So for F0 near 1 (metals) the env specular gains energy by up to ~27 % at moderate roughness and loses ~28 % at roughness 1 head-on. Dielectrics (F0 0.04) move by under 1 %. The light loop's `specular *= multiScatterEnergyCompensation(F0, NdotV, aaRoughness)` shares the same fit but multiplies a different G (Schlick-GGX `k = (r + 1)^2 / 8`, `geometrySmith`; the PRD records that G difference as accepted, rev 47), so there the fit is not clearly wrong and I do not claim it is.
- **Impact**: conductors in exteriors (and any traced/authored-environment metal) read a roughness/angle-dependent few-to-25 % off their energy-conserving environment specular; whole-lobe magnitude, visual only. The A6/A15 conductor sweeps (0.967-0.977) were measured with this in place, so any change must be re-measured.
- **Related**: RLT-IBL-003, `DBG_DISABLE_MULTISCATTER` (RLT-REF-008), PRD `docs/prd/reference-lighting.md` T9 ("`envBRDFApprox` ... used only for multi-scatter compensation").
- **Suggested Fix**: give `multiScatterEnergyCompensation` an overload that takes `Ess` and call it with `envAB.x + envAB.y` in the environment block; leave the light-loop call on the fit until the G difference is decided. Re-run the A6 conductor rows and A15-SPEC before and after; pin the choice in `environment_specular_is_the_exact_split_sum_with_probe_occlusion`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **SHADER**: A GLSL edit recompiles the SPIR-V, including `triangle_early.frag.spv` (`scripts/check-shader-artifacts.sh` is the gate)
- [ ] **TESTS**: A regression test pins this specific fix
