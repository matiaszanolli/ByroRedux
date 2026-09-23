# #4775: REN-D8-2026-09-23-03: after the combustion linger window, `simulationDt = 0` freezes the ≤ 3 % soot residual instead of decaying or clearing it, whenever the pass keeps dispatching

**Severity**: MEDIUM
**Labels**: medium, renderer, shaders, bug
**Source**: docs/audits/AUDIT_RENDERER_2026-09-23.md (REN-D8-2026-09-23-03)

- **Severity**: MEDIUM (visual residue, plus ongoing per-froxel transmittance-march cost).
- **Dimension**: Volumetrics
- **Location**:
  - `crates/renderer/src/vulkan/volumetrics.rs`: `VolumetricsPipeline::dispatch` (`frame_params.fog_reference[3] = if combustion_active { simulation_dt } else { 0.0 }`), `combustion_transport_active` and `requires_dispatch`.
  - `crates/renderer/shaders/volumetrics_inject.comp`: `transportCombustion` (the `else { chemistry = hadHistory ? probeChemistry : … }` arm and the `if (dt > 0.0)` decay block).
- **Status**: NEW (not covered by #3131, which introduced the `dt = 0` gate as a performance optimisation).
- **Description**:
  - `AEROSOL_LINGER_SECONDS` (78 s) is sized so that `exp(−0.045·78)` ≤ `AEROSOL_LINGER_CUTOFF_FRACTION` (3 %). The design assumes the renderer *stops* at that point: "avoiding a visible tail cut when the renderer stops the otherwise-idle transport dispatch".
  - It stops only when `requires_dispatch` is false, which then triggers a neutral clear and `history_valid = false`.
  - `requires_dispatch` stays true whenever `scatter_coef > 0`. That is every fogged exterior, and every interior with a local emitter, because `INTERIOR_DUST_EXTINCTION_PER_METER` forces 0.006/m.
  - In those scenes the shader keeps running with `dt = 0`. `transportCombustion` then copies `probeChemistry` / `probeOptical` forward unchanged, and every dissipation term (`AEROSOL_DISSIPATION`, fuel removal, radiance removal, cooling) is multiplied by `dt` inside `if (dt > 0.0)`, so nothing decays.
- **Evidence**:
  - `combustion_transport_active` returns false once `simulation_time > combustion_active_until_seconds`.
  - `requires_dispatch` returns `has_global_medium || !fog_volumes.is_empty() || …`.
  - In the shader, `simulationDt = clamp(params.fog_reference.w, 0, 1/15)` → `transportCombustion(..., dt = 0)` → the `else` arm carries history.
  - `combustionMedium` treats any `sigmaT > 1e-7` as active medium.
- **Impact**:
  - Up to 3 % of an explosion or smoke cloud's extinction and scattering persists indefinitely as a static, world-anchored haze. It also keeps drifting per REN-D8-02 until it leaves the frustum or a discontinuity or resize resets history.
  - `transportedMediumActive` stays true for those froxels, so each keeps paying `transportedCombustionTransmittance` for the sun and for every admitted local light.
- **Related**: #3131 (the `dt = 0` gate), REN-D8-2026-09-23-02.
- **Suggested Fix**: When transport lapses while history is valid, either:
  - (a) keep feeding a real `dt` to the dissipation terms only (skip the advection stencil, which is what #3131 wanted to save); or
  - (b) have the shader zero the transported fields when `dt == 0` and the linger has expired. For example, pass an "expired" flag and write the empty state, so the residual is dropped exactly as it is in the no-medium path.

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
