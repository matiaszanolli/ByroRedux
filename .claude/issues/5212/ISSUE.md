# #5212 — REN-D7-2026-10-03-02: the scene-static signal's coverage of renderer-appended combustion lights rests on an unguarded variable shadow in `draw_frame`

**Labels**: low,renderer,test-gap,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` — `draw_frame`, `let lights = frame_lights.as_slice();` immediately before `self.build_and_upload_instances(…, lights, …)`; consumer `build_and_upload_instances.rs` (`caustic_scene_key` light-rig fold → `caustic_scene_static` → `next_svgf_temporal_alpha` and `scene_static_last_build`)
- **Status**: NEW (test gap)
- **Description**: `next_svgf_temporal_alpha`'s `caustic_history_valid` input, and ReSTIR's parked mode 2 through `scene_static_last_build`, see light changes only through the `caustic_scene_key` fold over the `lights` slice handed to `build_and_upload_instances`. That slice is correct today: `draw_frame` shadows the app's `FrameInputs.lights` with `frame_lights`, the merged list after `append_combustion_surface_lights` and the #5055 priority re-sort. So advected/cooling combustion surface lights do break parked accumulation, as they should. Nothing pins this, though. `svgf_temporal_alpha_is_fed_the_combined_camera_and_light_rig_signal` and `scene_static_signal_sees_rigid_instance_set_changes` scan only `build_and_upload_instances.rs`, and no test references `frame_lights` outside its producer. Passing the un-shadowed app slice (an easy slip: #5055 just threaded a parallel `light_ids` through the same call chain) would compile and pass every test. It would also silently drop renderer-derived fire lights from the key, so SVGF would keep its ~1/256 parked α and ReSTIR mode 2 over GI and direct lighting that a burning field keeps changing.
- **Evidence**: `grep -rn "frame_lights" crates/renderer/src` hits only `draw.rs`, `assemble_camera_and_lights.rs`, `mod.rs`, `init.rs`, `telemetry.rs` and `shrink_frame_scratch.rs` production code; there are zero test needles. The key fold is `for light in lights { … position_radius … color_type … direction_angle … params }` in `build_and_upload_instances`.
- **Impact**: No live defect. This is a regression path that no `cargo test` can catch, landing on the HIGH-adjacent SVGF/ReSTIR history signals (parked ghosting = MEDIUM floor).
- **Related**: #4046, #4943, #4942 (closed); #5055 (this window).
- **Suggested Fix**: Add a `production_text` scan of `draw.rs` asserting that the `build_and_upload_instances(` argument list passes the `frame_lights`-derived slice (or rename the shadow, e.g. `merged_lights`, and pin the name). Alternatively, return the merged slice through `CameraAssemblyOutput` and use it directly.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix
