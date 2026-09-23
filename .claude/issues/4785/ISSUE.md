# #4785: PERF-D5-2026-09-23-03: The sun visibility ray (and its transported-soot march) is traced for every froxel even when the sun radiance is zero, or the froxel's medium is exactly zero

**Severity**: MEDIUM
**Labels**: medium, performance, renderer, shaders, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23.md (PERF-D5-2026-09-23-03)

- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**:
  - host: `crates/renderer/src/vulkan/context/post_passes.rs:669-678` (`sun_radiance = [0,0,0,fog_far]` when `sun_intensity <= 0`);
  - source of the zero: `byroredux/src/systems/weather.rs:146-155` (`sun_intensity = 0.0` outside `[sunrise_begin, sunset_end]`);
  - shader: `volumetrics_inject.comp:2738-2762` (unconditional `traceShadowBinary` to `DIRECTIONAL_SHADOW_TRACE_DISTANCE`, a glass-only second ray on interior misses, and the `transportedCombustionTransmittance` march) and the local-light loop `:2804-2888`.
- **Status**: NEW
- **Description**:
  - `inscatter += scattering_coef * phase * params.sun_color.rgb * visibility`. When `sun_color.rgb == 0` (every exterior night frame) the visibility term is multiplied by zero, but it is still computed: one ray query per froxel, and in interiors a second glass ray wherever the opaque ray escapes.
  - Froxels whose `scattering_coef` is exactly zero also trace the sun ray and every admitted local light's rays. Examples: `FogMedium::DISABLED` weather with the pass running only for fog volumes, or froxels outside every local medium.
- **Evidence**: `sun_color` is a per-dispatch uniform, so a `if (any(greaterThan(params.sun_color.rgb, vec3(0.0))))` guard is warp-uniform and free. `scattering_coef` is a per-froxel value that is already computed before the traces (`:2691`).
- **Impact**:
  - *est.* 0.92 M (720p render) to 2.07 M (1080p) wasted ray queries per frame on every exterior night frame, which is roughly 35–40 % of the in-game day in FNV/FO3/Skyrim time-of-day tables, plus the 8-step soot march in transported froxels.
  - A coherent terminate-on-first-hit shadow ray against an exterior TLAS is on the order of 0.1–0.4 ms per 2 M rays on this GPU (*est.*, not measured).
  - It is tier-invariant.
  - Until REN-D8-2026-09-23-01 is fixed, the medium above about 1 m is nearly zero, so most daytime sun and light rays are also near-worthless. Fixing that swap makes those rays productive again, so this finding is about the zero-radiance and zero-medium cases, not the swap.
- **Related**: REN-D8-2026-09-23-01 (argument swap), #1022 (interior sun policy; interiors use `SkyParams::default()`, `sun_intensity` 5.0, deliberately).
- **Suggested Fix**:
  - Wrap the sun block (opaque ray, glass ray and transmittance march) in a uniform `sun_color.rgb > 0` test, or pass a host flag in a spare UBO lane.
  - Skip the sun and local-light visibility work for a froxel when `max3(scattering_coef) == 0.0` exactly. Emission and authored inscatter do not need visibility, so the result is bit-identical.

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
