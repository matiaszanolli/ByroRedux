# #7391 — REN-D7-2026-11-23-02: `interior_sky_mode`'s "0 = sealed interior" does not describe production; every interior with a `SkyParamsRes` reads mode 2

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D7-02). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW
- **Dimension**: Denoiser/Composite (documentation / test premise)
- **Location**: `interior_sky_mode` (`context/sky_apertures.rs`), `build_sky_params` (`byroredux/src/render/sky.rs`), `interiorSky` comment in `shaders/include/camera_ubo_members.glsl`, test `interior_sky_mode_matches_the_composite_lanes`
- **Status**: NEW
- **Description**: The doc comment says "0 none (an exterior, or a sealed interior), 1 Show Sky, 2 bounded apertures". The function returns 2 whenever `!is_exterior && !interior_show_sky && portal_outdoor_sky.is_some()`. `build_sky_params` sets `portal_outdoor_sky: Some(outdoor.into())` for **every** interior as soon as `SkyParamsRes` exists (the early `None` return is the non-cell-harness case only). So a sealed interior with no marked opening is mode 2 with an empty aperture list, not mode 0; mode 0 in production means "exterior, or no `SkyParamsRes`". The unit test builds its "sealed" case with `portal_outdoor_sky: None`, which production never produces for a loaded cell.
- **Evidence**: `interior_sky_mode`: `if sky_params.is_exterior {0.0} else if sky_params.interior_show_sky {1.0} else if sky_params.portal_outdoor_sky.is_some() {2.0} else {0.0}`; `build_sky_params` interior arm returns `portal_outdoor_sky: Some(outdoor.into())` after `let Some(sky_res) = world.try_resource::<SkyParamsRes>()`.
- **Impact**: No behavioural bug: `pathEnvironmentRadiance` (`lighting.glsl`) takes the mode-2 branch only through `pathCrossesSkyAperture`, which returns false for a zero count, so a sealed room falls through to DALC/ambient exactly as mode 0 would, and `gi_sky_aperture_planes` returns an empty list. The cost is a misleading comment and a test that exercises a state production cannot reach, which will mislead whoever next reasons about "sealed" interiors (for instance when deciding whether the SVGF scene key needs the mode; it does not, because mode is constant per cell).
- **Related**: RLT-IBL-006 (`84104564e`); `fold_svgf_scene_inputs` folds `interior_show_sky` but not the mode-2 bit, which is safe for the same reason.
- **Suggested Fix**: reword the comment to "2: an interior with an outdoor palette (every cell with a `SkyParamsRes`); the sky shows only through marked apertures, none = a sealed room", and add a `build_sky_params`-level assertion that a plain interior reads mode 2 with zero planes.

(No other finding. All other Dim 7 deltas traced clean; see below.)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
