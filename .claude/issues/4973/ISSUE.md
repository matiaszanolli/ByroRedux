# #4973: REN-D10-2026-09-27-04: `dielectricF0FromIor`'s `eta` floor does not prevent the mirror-class F0 its comment says it prevents, and four IOR floors disagree

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4973
- **Labels**: low,renderer,shaders,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D10-2026-09-27-04**._

- **Severity**: LOW
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/include/pbr.glsl` `dielectricF0FromIor` (`float e = max(eta, 1e-3);`, L150). The other floors are:
  - `evaluatePathBsdf` / `pathSpecularProbability` (`max(ior, 1e-3)`);
  - `triangle.frag` `GLASS_IOR = max(mat.ior, 1e-3)`;
  - `shadow_transport.glsl` `max(hitMat.ior, 1.0)` (L145);
  - `triangle.frag` GI glass `max(hitMat.ior, 1.001)`.
- **Status**: NEW (hardening; the #1253 guard exists but is ineffective).
- **Description**: The #1253 comment says the clamp keeps an uninitialised `mat.ior = 0` from yielding `F0 = 1.0`. But `((1-e)/(1+e))²` at `e = 1e-3` is 0.996, still mirror-class. The unclamped `eta = 0` was never a divide-by-zero either: the singularity is at `eta = -1`. The formula is also symmetric under `eta ↔ 1/eta`, so any floor below 1 maps small IOR values back up to high F0. Current reachability is nil in production:
  - `material_optical_scalar` returns `DEFAULT_DIELECTRIC_IOR` for every non-fire kind;
  - fire-refraction, the one kind whose `ior` lane is a 0–1 strength, returns before the F0 line and is excluded from the TLAS (`draw_command_eligible_for_tlas`).

  It is reachable only via `mat.set` or Cornell.
- **Impact**: Defence-in-depth that does not defend; inconsistent floors for one field.
- **Suggested Fix**: Floor at 1.0 (vacuum → F0 = 0) in `dielectricF0FromIor` itself, and drop the per-caller `max(…, 1e-3)`. Correct the comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
