# #4299: REN-2026-09-14-D3-01: `GpuCamera.exterior_sky_tint.w` became the live sky-cubemap ready flag, but `shader-pipeline.md` and the field's own rustdoc still document it as "reserved"

- **Labels**: low,renderer,shaders,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4299
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (doc-rot)
- **Location**: `docs/engine/shader-pipeline.md` (GpuCamera table, `| 352 | 16 | exterior_sky_tint |` row); `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuCamera::exterior_sky_tint` doc comment); writer `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs`; readers `crates/renderer/shaders/include/raytrace.glsl` / `crates/renderer/shaders/include/lighting.glsl`
- **Status**: NEW
- **Description**: `6db9eac2` repurposed `exterior_sky_tint.w` from "reserved (0)" to the SKYAL ready flag. It is written as `if self.sky_cube.is_some() { 1.0 } else { 0.0 }` and gates every `skyCube` read, because binding 20 is `PARTIALLY_BOUND` and never written when the bake fails to initialise. The GLSL mirror comments were updated. The authoritative layout doc and the Rust field doc were not:
  - `shader-pipeline.md`'s row still reads "xyz = the live exterior's sky zenith colour (#3323); w reserved".
  - `gpu_types.rs` still says "xyz = the **exterior** TOD/weather zenith colour in linear RGB; w reserved (0)".
  - The `CameraUBO` comment in `bindings.glsl` still says the lane is "Read ONLY there" (the window-portal escape), which is now true only of `.xyz`.

  This is the exact class #3989 fixed for `render_debug.w` and `material_flags` bit 10: a live lane advertised as free. `shader_pipeline_doc_does_not_advertise_live_lanes_as_free` exists for this class but pins only those two rows.
- **Evidence**:
  ```text
  docs/engine/shader-pipeline.md:  | 352 | 16 | `exterior_sky_tint` | xyz = ... (#3323); w reserved. ...
  gpu_types.rs:                    /// xyz = the **exterior** TOD/weather zenith colour in linear RGB; w
                                   /// reserved (0).
  assemble_camera_and_lights.rs:   if self.sky_cube.is_some() { 1.0 } else { 0.0 },
  raytrace.glsl:                   missCol = exteriorSkyTint.w > 0.5 ? texture(skyCube, direction).rgb : ...
  ```
- **Impact**: Documentation only today. The hazard is the one #3989 names: a future author allocates the "reserved" `w` for new per-frame state, and every exterior RT miss then samples `skyCube` depending on that unrelated value. When the bake is absent that's an unwritten `PARTIALLY_BOUND` descriptor, which is undefined data rather than a validation error or crash, so `cargo test` would not catch it.
- **Related**: #3989 (same class, same doc, same guard test); #3323 (introduced the lane); REN-2026-09-14-D2-02 (same commit's binding-20 doc omission).
- **Suggested Fix**:
  - Update the `shader-pipeline.md` row and the `gpu_types.rs` rustdoc to "w = sky-cubemap ready flag (1.0 when `SkyCubePipeline` exists; gates every `skyCube` read) — Not a free slot".
  - Correct the `bindings.glsl` "Read ONLY there" sentence to scope it to `.xyz`.
  - Extend `shader_pipeline_doc_does_not_advertise_live_lanes_as_free` with an `exterior_sky_tint` row assertion.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
