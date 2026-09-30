# #4877: REN-D10-2026-09-24-05: lighting doc rot — 'fixed-prefix GI scan' comments and the exterior_sky_tint row

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D10-2026-09-24-05**._

**Severity**: LOW · **Status**: NEW · every claim below was checked against the code by the auditor and re-checked against the tip of `main` at publish time.

**Dimension**: Light/Sky

"fixed-prefix GI light scan" in `gpu_types.rs` `GpuLight::gi_priority_score` and in `assemble_camera_and_lights.rs` (#4017 removed that consumer; the score now serves the `MAX_LIGHTS` tail clamp and the post-combustion re-sort); the `shader-pipeline.md` `exterior_sky_tint` row says "read only by the window-portal escape" and "falls back to `SkyParams::default().zenith_color`", false since `0572bfd5a`.

## Completeness Checks
- [ ] **SIBLING**: The same stale claim is checked in the neighbouring docs and code comments (doc moves with the pass)

