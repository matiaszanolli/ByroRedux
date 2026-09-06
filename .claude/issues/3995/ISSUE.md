# #3995 — REN-2026-09-06-D8-01: SVGF's camera-static progressive-accumulation flag zeroes the α floor, silently cancelling the `svgf_recovery_frames` window that `signal_temporal_discontinuity` exists to install

**Labels**: medium, renderer, shaders, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D8-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/shaders/svgf_temporal.comp` (the `floorC` / `floorM` assignment inside the `hasHistory` branch); `crates/renderer/src/vulkan/svgf.rs` (`next_svgf_temporal_alpha`, `SvgfPipeline::upload_params`); `crates/renderer/src/vulkan/context/mod.rs` (`signal_temporal_discontinuity`)
- **Status**: NEW (no open issue matches `svgf` / `recovery` / `parked` / `static` in `open_titles.txt`; not present in the 2026-08-30 sweep's Dim 8/13 sections)
- **Description**: `signal_temporal_discontinuity`'s only effect on SVGF is
  `self.svgf_recovery_frames = self.svgf_recovery_frames.max(frames)`, which
  `next_svgf_temporal_alpha` turns into `SVGF_ALPHA_RECOVERY = 0.5` for both
  `alpha_color` and `alpha_moments`. Those land in `params.x` / `params.y`.
  The shader then does:

  ```glsl
  float invN   = 1.0 / (histAge + 1.0);
  float floorC = params.w > 0.5 ? 0.0 : params.x;   // params.w = camera_static
  float alphaC = max(floorC, invN);
  ```

  `params.w` is `camera_static`, computed in `assemble_camera_and_lights` as a
  pure element-wise `view_proj` vs `prev_view_proj` comparison. When the camera
  is parked, `floorC` is **0**, so the host-side recovery α is discarded and the
  blend falls back to `1/(histAge + 1)` — and `histAge` is exactly what a parked
  camera drives to its `min(histAge + 1.0, 255.0)` ceiling, because zero motion
  means every pixel passes the mesh-ID and normal-cone tests every frame.
  A recovery window that is supposed to weight the current frame at 0.5
  therefore weights it at 1/256 ≈ 0.0039 instead — a 128× weaker recovery, and
  the elevated window decrements to zero while having had no effect at all.
  SVGF has no other response to a discontinuity: `signal_temporal_discontinuity`
  does **not** touch `SvgfPipeline::frames_since_creation`, so the `params.z`
  hard-reset path is not an alternative route.
- **Evidence**:
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` passes
    both values to the same call:
    `svgf.upload_params(&self.device, frame, alpha_color, alpha_moments, camera_static)`.
  - `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs`:
    `let camera_static = vp.iter().zip(self.prev_view_proj.iter()).all(|(a, b)| (a - b).abs() < 1.0e-6);`
    — camera-only, no scene or lighting term.
  - Reachable callers that can fire with a parked camera: `save_io.rs` (live
    load-apply, twice), `debug_load.rs` (three sites), `streaming_helpers.rs`
    (three sites), `app_step.rs` (four sites), `context/resize.rs`
    (`RESIZE_RECOVERY_FRAMES` / `SWITCH_RECOVERY_FRAMES`), and now
    `context/post_passes.rs` (`TAA_DISPATCH_FAILURE_RECOVERY_FRAMES`,
    `FSR_DISPATCH_FAILURE_RECOVERY_FRAMES`).
  - **No test covers the interaction.** All four α tests
    (`steady_state_alpha_is_schied_floor`,
    `recovery_window_uses_elevated_alpha_and_decrements`,
    `last_recovery_frame_uses_elevated_alpha_then_reverts`,
    `streaming_recovery_window_runs_full_n_frames_then_reverts`) exercise
    `next_svgf_temporal_alpha` in isolation; every one of them passes while the
    shader throws the returned value away.
  - Side note on the same doc: `signal_temporal_discontinuity`'s comment names
    "cell load, weather flip, fast camera turn" as its triggers, but no weather
    system calls it — `byroredux/src/systems/weather.rs` cross-fades every WTHR
    field continuously (`lerp3` / `lerp1` per key), so no signal is *needed*;
    the trigger list is aspirational, not a missing call.
- **Impact**: Every discontinuity signalled while the camera happens to be
  stationary is a no-op for SVGF colour and moments on every pixel whose
  per-pixel history survived — which is precisely the geometry-unchanged,
  lighting-changed case (live save load onto the same cell, a scripted
  light/imagespace change, a debug reload, a resize/upscaler switch, and the new
  `#3605` path). The visible artefact is the stale bounce term persisting for
  seconds. The direct term is unaffected, which is why it reads as a soft
  "GI didn't notice" rather than a frozen image.
- **Related**: `#674` / `DEN-4` (the recovery window itself); `#3605`
  (`c43cb269`) — its SVGF limb is subject to exactly this cancellation;
  `REN-2026-09-06-D8-02` below (the same flag's other blind spot);
  `REN-2026-09-06-D13-01`.
- **Needs RenderDoc**: no — the whole state machine is host-side plus one
  shader `select`.
- **Suggested Fix**: Make the progressive-accumulation drop conditional on the
  recovery window being closed: pass the floor the host already computed and let
  the shader use `floorC = (params.w > 0.5 && recoveryClosed) ? 0.0 : params.x`,
  or (simpler, no new lane) have `next_svgf_temporal_alpha`'s caller force
  `camera_static = false` while `svgf_recovery_frames > 0`. Add a pure-fn test
  that pins "a live recovery window wins over the camera-static drop", since the
  existing four cannot see this.

---

---

# LOW

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
