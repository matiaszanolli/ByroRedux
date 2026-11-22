# #7268: INC-R-01: The GI path's sky-aperture test compares an absolute ray origin against render-origin-relative planes

- **Severity**: MEDIUM
- **Dimension**: audit-renderer D6/D10 (shaders + lighting); silent divergence
- **Location**: `crates/renderer/shaders/include/lighting.glsl:598-611` (`pathCrossesSkyAperture`); caller `crates/renderer/shaders/triangle.frag:4880-4913`; upload `crates/renderer/src/vulkan/context/sky_apertures.rs:42`
- **Changed in**: 84104564e (RLT-IBL-006)
- **Status**: NEW
- **Description**: The host uploads each aperture plane's centre relative to the render origin. The shader then tests the GI path origin against it, but that origin is an absolute world position. The difference is the render origin, which `snap_render_origin` sets to `floor(camera / 4096) · 4096`. It is non-zero whenever any camera coordinate is negative or ≥ 4096, which is the common case for interiors. The aperture planes are therefore displaced by up to the render origin, and the escape test misses real openings or hits through walls.
- **Evidence**:
  - `sky_apertures.rs:42`: `let rel = centre - render_origin;` → `center: [rel.x, rel.y, rel.z, 0.0]`. `bindings.glsl` and the `docs/engine/shader-pipeline.md` binding-27 row both document the centre as relative to the render origin.
  - `triangle.frag:206`: `vec3 fragWorldPos = fragWorldPosRel + renderOrigin.xyz;`
  - `triangle.frag:4880`: `giOrigin = offsetRayOriginForDirection(fragWorldPos, …)`, then `pathOrigin = giOrigin`, then `pathEnvironmentRadiance(pathOrigin, pathDir)`.
  - `lighting.glsl:602`: `rotateBySkyApertureQuaternion(origin - aperture.center.xyz, …)`.
  - The composite's own aperture code works in absolute `camera − center`.
- **Impact**: In a bounded-aperture interior (interior sky mode 2) whose camera sits outside [0, 4096)³, escaping GI paths read the cell ambient instead of the portal sky, or read sky through solid walls. This is the RLT-IBL-006 behaviour itself. `docs/smoke-tests/m-light-portal-sky.sh` exercises only Show Sky (mode 1, no planes), so mode 2 has no live coverage.
- **Related**: #6604 (M-LIGHT umbrella), RLT-IBL-006
- **Suggested Fix**: In `pathCrossesSkyAperture`, subtract `renderOrigin.xyz` from the origin, or pass the render-relative origin from the caller. Pin the convention with a `shader_contract_tests` string check, and add a mode-2 smoke scenario.

**Found by**: `docs/audits/AUDIT_INCREMENTAL_2026-11-22b.md`

## Completeness Checks
- [ ] **SIBLING**: every shader-side consumer of a render-origin-relative buffer checked for the same space mix
- [ ] **TESTS**: a contract or smoke test pins the aperture plane space (interior sky mode 2)
