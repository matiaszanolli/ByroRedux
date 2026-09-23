# #4774: REN-D8-2026-09-23-02: transported combustion field is written at the arithmetic-mean slab distance but sampled at the geometric-mean texel centre, a constant camera-ward drift of 0.0072 slices/frame

**Severity**: MEDIUM
**Labels**: medium, renderer, shaders, bug
**Source**: docs/audits/AUDIT_RENDERER_2026-09-23.md (REN-D8-2026-09-23-02)

- **Severity**: MEDIUM (visual; affects all combustion content such as fire, smoke and explosions).
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp`:
  - `main` (`float fieldT = 0.5 * (fieldFrontT + fieldBackT)`, ~line 2574);
  - `reprojectHistory` (`sliceCoordinate(previousDistance)`);
  - `samplePreviousTransport` (`texture(previousCombustion*, previousUvw)` through the LINEAR `transport_sampler`).
- **Status**: NEW
- **Description**:
  - Texel *i* of `combustionState` / `Dynamics` / `Optical` is written for world position `froxel_to_world(fieldUv, fieldT)`, where `fieldT = (d_i + d_{i+1}) / 2`.
  - The next frame reads the field with `texture()` at `uvw.z = froxelSliceCoordinate(d)`. That places texel *i*'s centre at `froxelSliceDistance((i+0.5)/N)`, which in the exponential region (> 5 m) is the geometric mean `√(d_i·d_{i+1})`.
  - The two positions differ. A destination froxel that asks for its own position lands `δ = ln((1+r)/(2√r)) / ln r` texels too far, with `r = (far/linear)^(1/((1−f)N))`.
- **Evidence**: an exact evaluation with the default `VolumetricsConfig` (64 slices, 128 m far, 5 m linear floor, fraction 0.125):
  - δ = **+0.0072 texel** for every slice ≥ 8;
  - 0 in the linear region.

  Every carried value therefore becomes `(1−c)·T_i + c·T_{i+1}` with c = 0.0072 per frame. That is upwind advection toward the camera, independent of velocity and of `dt`: the `dt = 0` carry path at `chemistry = hadHistory ? probeChemistry : …` resamples the same way.
  - At 33 m (slice 40, slab 1.9 m) this is about 0.8 m/s at 60 fps, and frame-rate dependent (about 2 m/s at 144 fps).
  - A parked camera and a lingering cloud give `d(t) ≈ d₀·e^(−0.026 t)` at 60 fps.
  - The single-pass BFECC does not remove it. `probeChemistry` and the round-trip `errorChemistry` carry the same mapping bias, so their difference excludes it.
- **Impact**:
  - Transported smoke, fire plumes and explosion clouds lean and creep toward the viewer, faster at higher frame rates.
  - The resampling adds extra numerical diffusion along Z.
  - Emission sources re-inject at the correct place every frame, so the artefact is strongest on lingering (unsourced) media.
- **Related**: REN-D8-2026-09-23-03 (the frozen residual also drifts); #2470 (the analogous half-slab convention fix for the integrated volume, closed).
- **Suggested Fix**: Store the field at the texel-centre distance, `fieldT = sliceDistance((z + 0.5) / N)`, and derive the slab endpoints the same way, so that writing and sampling share one convention. Confirm on device: a static-camera A/B of a lingering explosion cloud at 20–40 m.

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
