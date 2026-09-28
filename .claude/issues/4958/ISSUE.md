# #4958: REN-D4-2026-09-27-01: `shader-pipeline.md`'s submission order omits the two new top-of-frame recordings and the early-test pipeline

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4958
- **Labels**: low,renderer,documentation,doc-rot

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D4-2026-09-27-01**._

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `docs/engine/shader-pipeline.md` ("Per-Frame Submission Order", "G-Buffer Layout"). Code: `crates/renderer/src/vulkan/context/begin_frame_recording.rs` (`begin_frame_recording`), `crates/renderer/src/texture_registry/dynamic_rgba.rs` (`TextureRegistry::record_pending_rgba_uploads`), `crates/renderer/src/vulkan/restir.rs` (`ReservoirBuffers::begin_frame`), `crates/renderer/src/vulkan/context/geometry_pass.rs` (`PipelineKey::Opaque { early_tests: true, .. } => self.pipeline_early`).
- **Status**: NEW (window commits `e2f99ad55` and `186234944`, both after the 09-24 audit). Related: open #4871 (earlier frame-order drift). #4871's groundcover_models step and #4602 flush-edge items have since landed as rows 5d and 22b.
- **Description**: `begin_frame_recording` now records two command sequences before step 2 (skin palette), and the doc names neither:
  1. **Dynamic-RGBA copies.** Per dirty texture: `ALL_COMMANDS → TRANSFER` with `SHADER_READ_ONLY → TRANSFER_DST` (src access `MEMORY_READ|MEMORY_WRITE`), then `cmd_copy_buffer_to_image`, then `TRANSFER → ALL_COMMANDS` back to `SHADER_READ_ONLY`. These are the HUD, Scaleform and ground-cover atlas updates that used to be one-shot submits (#3429).
  2. **Reservoir history clear.** `FRAGMENT|TRANSFER → TRANSFER|FRAGMENT` buffer barriers on the curr/prev reservoirs, a `cmd_fill_buffer` of the current slot, then `TRANSFER → FRAGMENT`.

  There are three smaller gaps in the same doc:
  - Step 6 lists `triangle.vert / .frag` only. Certified opaque draws now run `triangle_early.frag.spv` (`layout(early_fragment_tests)`) through `pipeline_early`, selected by `DrawCommand::allows_early_fragment_tests`.
  - Step 5d does not state the model tier's barrier (`COMPUTE → DRAW_INDIRECT|VERTEX|FRAGMENT|TRANSFER`), nor that it also writes the previous-model buffer's tail.
  - The G-buffer section says the pass is "Written by … (`triangle.frag` + `water.frag`)". It omits `groundcover_blade.frag`, which writes attachments 0/2/5/6/7 and masks off 1/3/4.
- **Evidence**:
  - `begin_frame_recording`: `self.texture_registry.record_pending_rgba_uploads(…)` then `self.reservoir_buffers.begin_frame(&self.device, cmd, frame);`.
  - `grep -n "record_pending_rgba_uploads\|begin_frame\|pipeline_early\|early_fragment" docs/engine/shader-pipeline.md` returns nothing.
- **Impact**: This doc is the authoritative barrier inventory that audits and sync reviews diff against. Two cross-submission image and buffer hazards handled at the top of the frame are invisible to it. This is the fourth consecutive audit where the doc did not move with a new recorded pass.
- **Related**: #4871, #4586, #4525.
- **Suggested Fix**: Add rows 1b (RGBA copies) and 1c (reservoir clear) with their barrier masks, name `pipeline_early` in step 6, and extend 5d and the G-buffer writer list. Widen `shader_pipeline_documents_every_record_pass_helper` beyond `post_passes.rs`, for example to every `record_*` / `begin_frame` recorder called from `draw_frame` and its phase files.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
