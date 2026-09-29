# #4986: CONC-D1-2026-09-28-01: egui-ash-renderer's partial texture update transitions the whole egui image from `UNDEFINED`, with no source scope

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: medium,sync,renderer,vulkan,bug
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: MEDIUM.
  - The layout discard is certain from the code. Whether it produces a *visible* artifact depends on the driver, so that part is a HYPOTHESIS.
  - It affects only the debug overlay, and the defect is in a third-party crate. That is why this is not rated HIGH.
- **Dimension**: Vulkan Queue & AS Sync
- **Location**:
  - The call site is `crates/renderer/src/vulkan/egui_pass.rs:250-255`, inside `draw_frame` while `cmd` is recording. It holds the graphics-queue Mutex.
  - The defect is in the dependency `egui-ash-renderer-0.11.0/src/renderer/vulkan.rs:443-534` (`Texture::cmd_update`), reached from `src/renderer/mod.rs:344-365` (`set_textures`, the `delta.pos == Some(..)` arm).
- **Status**: NEW. I found no issue or audit covering it. #1421, #1420, #1713 and #2786 cover the queue lock, the pool and the dependency comments, not this layout.
- **Description**: egui sends partial `ImageDelta`s (`pos: Some([x, y])`) whenever it rasterizes new glyphs into its font atlas. This is routine when a new panel or new text appears. `egui_pass.rs` forwards `output.textures_delta.set` unchanged to `Renderer::set_textures`. For a partial delta, egui-ash-renderer does three things:
  - It records `old_layout = UNDEFINED → TRANSFER_DST_OPTIMAL` over the **whole** subresource, with `src_stage = TOP_OF_PIPE` and `src_access = empty`.
  - It copies **only** the delta rectangle.
  - It transitions the image to `SHADER_READ_ONLY_OPTIMAL`.

  This has two consequences:
  - **Content.** A transition from `UNDEFINED` allows the implementation to discard the image's contents. Every texel outside the delta rectangle, meaning every glyph already in the atlas, becomes undefined.
  - **Sync.** A `TOP_OF_PIPE` source with empty access gives no execution dependency on the previous frame's `FRAGMENT_SHADER` reads of the same image (the egui draw in frame N-1). The WAR ordering exists only because the host all-slots fence wait retired frame N-1 before this submit. That is a host-side edge, not a device edge.
- **Evidence**:
  ```rust
  // egui-ash-renderer-0.11.0/src/renderer/vulkan.rs:461-483
  .old_layout(vk::ImageLayout::UNDEFINED)
  .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
  .src_access_mask(vk::AccessFlags::empty())
  ...
  device.cmd_pipeline_barrier(command_buffer,
      vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::TRANSFER, ...)
  // then cmd_copy_buffer_to_image with image_offset/extent = the delta rect only
  ```
  Call path: `draw_frame` → `EguiPass::dispatch` (`egui_pass.rs:250`, `queue.lock()`) → `set_textures` → `Texture::update` → `execute_one_time_commands` (`vulkan.rs:560-595`: `queue_submit`, then `queue_wait_idle`).
- **Trigger Conditions**: The debug UI overlay is visible and a frame's `textures_delta.set` holds a partial update, for example the first appearance of a new glyph or font size.
- **Impact**: Previously rasterized overlay glyphs may become garbled or black after a partial update, until egui re-uploads the whole atlas. This happens only on drivers or memory layouts that actually discard on an `UNDEFINED` transition, such as compressed color layouts. The engine's own rendering is unaffected.
- **Verification Path**: Not reachable by `cargo test`.
  - Run `BYRO_VALIDATION=1` with the debug overlay open and force new glyphs, for example by typing into a console field. Look for `SYNC-HAZARD-WRITE-AFTER-READ` (or `…-WRITE-AFTER-WRITE` from the layout transition) on the egui texture image in the one-time submit.
  - Visual check: glyph corruption after new text appears. The live runs of 2026-09-27 did not exercise the overlay.
- **Related**: #1713 and #1421 (the queue-lock scope around this call), #2786.
- **Suggested Fix**: Patch the dependency, via a `[patch]` override or upstream, so that a partial update transitions from `SHADER_READ_ONLY_OPTIMAL` with `src = FRAGMENT_SHADER / SHADER_READ`. Keep `UNDEFINED` only for a freshly created image. Alternatively, have `egui_pass.rs` coalesce partial deltas into a full re-upload (`pos: None`). Confirm with the validation run above before and after the change.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D1-2026-09-28-01) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in the other one-time/partial texture paths (texture_registry, dynamic_rgba, overlay uploads)
- [ ] **DROP**: If Vulkan objects change, teardown ordering is still correct (see the three load-bearing orderings in `context/teardown.rs`)
- [ ] **TESTS**: A regression test (or a recorded `BYRO_VALIDATION=1` capture) pins this specific fix
