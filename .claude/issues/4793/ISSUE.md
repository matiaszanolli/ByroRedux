# #4793: PERF-D5-2026-09-23b-02: Sealed-interior daytime froxels whose sun ray escapes now pay up to 4 extra closest-hit rim rays, recomputed every frame and not shed by the ray tier

**Severity**: MEDIUM
**Labels**: medium, performance, renderer, shaders, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D5-2026-09-23b-02)

- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**:
  - `crates/renderer/shaders/volumetrics_inject.comp`:
    - `:316-338` `traceArchitectureDistance` (opaque flag only; `while (rayQueryProceedEXT(rq)) {}` is a closest-hit traversal);
    - `:351-380` `hasArchitectureRimAroundSkyRay` (4 probes, 192 BU apart);
    - `:2927-2962`: the sun block;
    - `:2955-2957`: the gate.
  - `crates/renderer/src/vulkan/context/post_passes.rs:584-595`: the interior dust floor.
  - `byroredux/src/render/mod.rs:1306`: `with_sky_aperture_dust_floor`.
  - `byroredux/src/render/sky.rs:73-98,122-160`: the portal sun.
- **Status**: NEW (`0572bfd5a`)
- **Description**:
  - **Before**: a sealed-interior froxel traced one terminate-on-first opaque sun ray, then on a miss one glass ray.
  - **Now**: on an opaque miss with no authored aperture and no architectural glass, it runs up to 4 closest-hit traversals out to the grid far plane, each followed by a `rigidBoundaryNormal` vertex fetch.
  - Opaque misses are the common interior case: Bethesda interiors routinely omit ceilings (per the code comment and `interior-godrays-status.md`). A missing ceiling costs +1 ray per froxel (probe 1 misses and exits); partial shells cost 2–4.
  - The gate doesn't bound it:
    - `dot(scattering_coef,1) > 1e-7` is always true in interiors, because the dust floor forces a uniform medium;
    - `sun_color > 1e-5` holds from sunrise to sunset.
  - The verdict depends only on froxel position, sun direction and static architecture, yet it is recomputed for ~1–2 M froxels every frame.
  - Only `volumetric_light_cap` follows `AdaptiveRayBudget`, so the controller can't shed these rays.
- **Evidence**: The shader header's own worst-case count rose from 10 to 14 rays per froxel (`:19-28`). *est.* extra rays per frame:

  | Render extent | Froxels | Missing ceiling (+1) | Partial rims (+4) |
  |---|---:|---:|---:|
  | 720p | 921,600 | +0.92 M | +3.7 M |
  | 1080p | 2.07 M | +2.07 M | +8.3 M |

- **Impact**:
  - A new GPU cost in daytime interiors that no tier drop can remove. All four interior bench controls (Prospector, BanneredMare, MedTek, Dugout) are affected.
  - It is invisible inside the single `volumetrics_ms` bracket (#4789, worsened).
  - It also worsens #4785: interiors now take a zero-radiance sun at night, yet still trace the opaque ray, the aperture scan and the glass ray.
- **Related**: #4785, #4789, #4787, #2509
- **Suggested Fix**:
  - Wrap the whole interior sun block in a warp-uniform `sun_color.rgb > 0` test (this also closes the interior half of #4785).
  - Amortise the rim verdict per coarse block or per fog cluster, rebuilt only when the sun moves or the grid re-centres.
  - Skip unmarked-opening detection at ray tier 0.
  - Measure noon against midnight in a missing-ceiling interior at a pinned `--rt-test-ray-quality-tier`.
- **Confidence**: High on structure and gating; low-to-medium on magnitude (it depends on the per-cell escape fraction).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
