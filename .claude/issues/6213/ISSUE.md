# #6213: REN-H-01: The EyeEnvmap branch mixes render-origin-relative eye centres with the absolute fragment position, so the "iris-centred" reflection collapses to a direction near `renderOrigin`

**Labels**: high,renderer,shaders,bug

**Source**: docs/audits/AUDIT_RENDERER_2026-10-21.md · **Severity**: HIGH · **Dimension**: Camera-Relative Precision / NIFAL Material · **Location**: `crates/renderer/shaders/triangle.frag (if (isEyeEnvmap) block)`

- **Location**: `crates/renderer/shaders/triangle.frag`, the `if (isEyeEnvmap)` block (`leftWS`, `rightWS`, `centerWS`, `eyeR`), from `b4d073454` (#5961)
- **Status**: NEW. It was introduced by the fix for closed #5961; it is not a regression of an earlier fix.
- **Description**:
  - `leftWS = (instances[fragInstanceIndex].model * vec4(mat.eyeLeftCenterX, …, 1.0)).xyz`.
  - Rigid `GpuInstance.model` is render-origin-relative: `rebase_model_matrix` subtracts `render_origin` from the translation in `build_and_upload_instances.rs`.
  - `fragWorldPos` is absolute: `fragWorldPos = fragWorldPosRel + renderOrigin.xyz` at the top of `main()`.
  - The block then takes `distance(fragWorldPos, leftWS)` and `reflect(normalize(fragWorldPos - centerWS), N)`, which mixes the two spaces.
  - `snap_render_origin` floors every axis to 4096, so the origin is essentially never zero. Even a camera at (100, 50, −1) gets origin (0, 0, −4096).
  - The difference is therefore dominated by the ≥4096-unit origin offset. The "view direction from the eye centre" is nearly the same vector for every iris fragment, and the left/right pick is arbitrary.
  - That is the flat, non-coherent reflection the branch exists to avoid.
- **Evidence**:
  - `ray_hit.glsl` does the same reconstruction correctly: `w0 = (hi.model * vec4(v0, 1.0)).xyz + renderOrigin.xyz;`, pinned by the `shader_contract_tests.rs` case near line 745.
  - The eye block has no `renderOrigin` term, and no test mentions `centerWS` or the eye centres' space (`eye_pack_tests` checks only the CPU packing).
- **Impact**: every EyeEnvmap eye (Skyrim / FO4 NPC eyes with an authored cubemap and `eyeCubemapScale > 0`) shows a reflection whose sample direction depends on the camera's 4096-cell rather than the eye geometry. It also jumps when the render origin snaps across a cell boundary. The documented skinned-eye limit (centres track the actor root) is separate and does not cover this.
- **Related**: #5961, #1495/#1496 (coordinate split), #2747 (the `ray_hit.glsl` rebase); SKY-D2-01 (same block, distinct defect: the eye centres reach the shader in Z-up — fix both together, since fixing either alone still leaves a near-constant reflection direction).
- **Suggested Fix**: add `+ renderOrigin.xyz` to `leftWS` and `rightWS`. Alternatively, do the whole computation in relative space (`fragWorldPosRel`). Add a source-shape pin beside the `ray_hit.glsl` one.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
