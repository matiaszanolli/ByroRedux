# #4870: REN-D3-2026-09-24-06: GPU-struct doc/comment drift — render_origin.w, cone shape, CompositeParams SAFETY, GpuInstance mirror count, flags table, skyTint.w

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D3-2026-09-24-06**._

**Severity**: LOW · **Status**: NEW · every claim below was checked against the code by the auditor and re-checked against the tip of `main` at publish time.

**Dimension**: GPU-Struct Layout

(1) `render_origin.w` is now the open-sky flag but `gpu_types.rs` and `record_volumetrics_pass` still say `is_exterior`; (2) `GpuFogVolume.center_shape` doc omits shape 3 (cone); (3) `CompositeParams` NoUninit SAFETY says every field is `[f32;4]` or `[[f32;4];4]` (false since `sky_aperture_count: [u32;4]` / `sky_apertures`) and the `depth_params.x` doc "1.0 = sky enabled" is stale for Show-Sky interiors; (4) docs count five `GpuInstance` mirrors, there are six; (5) the `shader-pipeline.md` flags table describes bit 8 `DIFFUSE_ALPHA` backwards ("BC1 carries alpha"; `constants.rs` says set for BC2/BC3/BC7/RGBA, clear for BC1), omits bit 9 `LOD_BLOCK`, and says `MAX_TERRAIN_TILES` is 32 B per tile (real 160 B); (6) **Regression of #1089**: `skyTint.w` still says "reserved" in three `CameraUBO` mirrors (`triangle.vert`, `cluster_cull.comp`, `caustic_splat.comp`) while `bindings.glsl` and `GpuCamera` say `w = sun_angular_radius`.

## Completeness Checks
- [ ] **SIBLING**: The same stale claim is checked in the neighbouring docs and code comments (doc moves with the pass)

