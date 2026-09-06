# #4046 — REN-2026-09-06-D8-02: SVGF's progressive-accumulation flag ignores the light rig, although the engine already computes exactly that signal one scope away and spends it only on the caustic accumulator

**Labels**: low, renderer, shaders, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D8-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs` (`camera_static`); `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (`caustic_scene_key`, `caustic_history_valid`); `crates/renderer/shaders/svgf_temporal.comp` (the `params.w` branch)
- **Status**: NEW
- **Description**: `#2468` built a per-frame "the scene that determines splat
  landing points is unchanged" signal: `caustic_scene_key` folds every
  caustic-source model matrix **and** every visible light's
  `position_radius` / `color_type` / `direction_angle` / `params`, and
  `caustic_scene_static` additionally requires `!rigid_instance_moved &&
  pose_dirty.is_empty()`. That composite signal is spent entirely on
  `caustic_history_valid`. SVGF is handed the camera-only half. The stated
  reason (`record_post_passes`' parameter comment) is that *"SVGF and TAA reject
  stale history per pixel"* — but the per-pixel rejection is purely geometric
  (`stableMeshIdsMatch` plus a normal cone). It cannot see a light that changed
  colour, intensity, radius, or position while the surface stayed put, which is
  precisely what the light half of `caustic_scene_key` was built to detect.
- **Evidence**:
  - `caustic_scene_key = fold_caustic_key_f32(caustic_scene_key, lights.len() as f32)`
    then a fold over each light's four `vec4`s, with the comment *"a lantern
    being carried, a light being coloured / dimmed by a weather or script
    change, or a light entering or leaving the visible set all move the pool."*
  - `let caustic_history_valid = camera_static && caustic_scene_static;` — the
    only consumer.
  - `svgf.upload_params(..., camera_static)` — SVGF gets the bare camera flag.
  - With `floorC = 0` the blend is `1/(histAge + 1)` and `histAge` saturates at
    255, i.e. a ~256-frame (~4.3 s at 60 FPS) time constant on the GI response.
  - Live animated-light producers exist: `byroredux/src/systems/light_anim.rs`
    and `append_combustion_surface_lights`
    (`crates/renderer/src/vulkan/volumetrics.rs`), whose sole caller is
    `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs` — the
    same function that computes `camera_static`, so both signals are already in
    scope together.
- **Impact**: Standing still in a torch-lit Bethesda interior, the *direct*
  flicker is correct (it is not denoised) but the bounce/GI response lags by up
  to ~4 s. The code comment calls the trade-off *"acceptable for the quality
  win"*, which is a considered decision — the finding is that it was taken
  without noticing the discriminating signal already exists and is free.
- **Related**: `#2468`; `REN-2026-09-06-D8-01` (same flag, sharper consequence).
- **Needs RenderDoc**: no.
- **Suggested Fix**: Thread the existing `caustic_scene_static` (or a
  lights-only sub-key) into the SVGF `params.w` decision so progressive
  accumulation only engages when the camera *and* the light rig are both
  unchanged; the value is already computed in the same function that calls
  `svgf.upload_params`.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
