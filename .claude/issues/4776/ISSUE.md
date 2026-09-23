# #4776: REN-D8-2026-09-23-04: froxel jitter `fract(rank + frame * R3)` is evaluated in float32 and collapses over a session; exact zero-jitter samples put slice 0 at the camera eye (`normalize(0)`)

**Severity**: MEDIUM
**Labels**: medium, renderer, shaders, bug
**Source**: docs/audits/AUDIT_RENDERER_2026-09-23.md (REN-D8-2026-09-23-04)

- **Severity**: MEDIUM (visual; long-session trigger; the `normalize(0)` part is driver-dependent).
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp` `froxelJitter` (`return fract(rank + frame * vec3(0.754877666, 0.569840296, 0.438289))`) and `main` (`vec3 view_dir = normalize(params.camera_pos.xyz - world_pos)`, ~line 2615). The frame index comes from `volume_params.w = (frame_counter & 0x00ff_ffff) as f32` in `record_volumetrics_pass`.
- **Status**: NEW
- **Description**:
  - The Cranley-Patterson rotation `frame × c` is a float32 product. Its fractional resolution is `ulp(frame × c)`, which coarsens as the frame count grows. The 24-bit mask keeps `frame` exact but not the product.
  - `frame_counter` resets only on resize (`resize.rs`).
  - The same rounding also makes `rank + frame·c` land exactly on an integer on some frames. That gives `jitter.z == 0.0`, so for `coord.z == 0`, `t = mix(0, d₁, 0) = 0` and `world_pos == camera_pos` exactly.
- **Evidence** (float32 simulation over the 64 ranks):
  - Exact `jitter.z == 0` occurs in 0.2 % of (frame, rank) pairs within the first 100 k frames, and in ≥ 25 % at 8 M frames.
  - The x channel has only **8 distinct values** at frame 1.5 M.
  - For `c = 0.7549` the resolution reaches 1/8 at about 1.39 M frames (6.4 h at 60 fps, 2.7 h at 144 fps) and 1.0 at 2²³ frames.
  - With `world_pos == camera_pos`, `normalize(vec3(0))` evaluates to 0·∞. The following `clamp(NaN, …)` in `medium_henyey_greenstein` is implementation-defined (GLSL.std.450 FMin/FMax on NaN operands). NVIDIA returns the non-NaN bound; other drivers may propagate NaN.
  - A NaN in the raw V-buffer would self-sustain: `mix(current, history, historyWeight)` with a NaN weight. It would then poison the column in `volumetrics_integrate.comp` and the composite pixel.
- **Impact**:
  - Long sessions lose the jitter that hides the per-froxel single-ray banding. The Prospector banding M-LIGHT v2 was built to remove comes back: gradually by ~6 h, fully by ~39 h at 60 fps.
  - On non-NVIDIA drivers, there is a latent NaN path from slice-0 froxels at the eye.
- **Related**: #2509 (per-froxel ray budget / banding history). `math_common.glsl`'s IGN (`frameCount * 5.588238`) has the same float-product shape but is outside this suite's scope.
- **Suggested Fix**:
  - Build the rotation from the integer frame index in fixed point, e.g. `float(uint(frame) * 0x9E3779B9u >> 8) * (1.0 / 16777216.0)` per channel, or wrap the frame modulo a period before multiplying.
  - Use `view_dir = -ray_dir` (exact for every `t > 0`; cheaper), or clamp `t` to a positive minimum.

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-23.md` (`/audit-suite --preset volumetrics-deep`, HEAD `2237da9c3`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related passes (other compute passes / history buffers / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
