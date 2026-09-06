# #4005 — REN-2026-09-06-D11-03: three stale prose statements on the pipeline / render-pass surface

**Labels**: low, pipeline, renderer, shaders, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D11-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs` (`create_render_pass`'s attachment-3 comment), `crates/renderer/src/vulkan/pipeline.rs` (`create_ui_pipeline`'s doc comment), `docs/engine/shader-pipeline.md` (§"G-Buffer Layout", the sentence after the table)
- **Status**: NEW
- **Description**: Three independent prose claims on this surface no longer
  match the code they describe. Each is small; they are grouped because they
  are one edit's worth of work and all three would mislead the next reader of
  this exact dimension.
  1. `create_render_pass`'s mesh-ID attachment comment says overflow "is
     handled by the warn-once `log::error!` + clamp in `draw.rs::draw_frame` +
     `upload.rs`". The clamp is still in `scene_buffer/upload.rs`, but the
     warn-once "RP-1" `log::error!` moved out of `draw_frame` into
     `context/build_and_upload_instances.rs` under the #3282 phase split.
     `draw.rs`'s only surviving "RP-1" mention is the *indirect-draw* ceiling
     policy in `should_use_indirect_draws`'s doc — a different overflow, on a
     different buffer; the instance-overflow `log::error!` this comment sends
     the reader to `draw_frame` for is not there. Same class of stale pointer
     #3881 just fixed one
     comment above it (the "search `0x80000000u` in `triangle.frag`"
     instruction).
  2. `create_ui_pipeline`'s doc says "water uses its own 128-byte
     push-constant layout on a separate pipeline layout". The separate layout
     is still true; the size is not — `WaterPush` is **16 bytes**, held there
     by `const _: () = assert!(size_of::<WaterPush>() == 16)`, since the
     per-draw payload moved into the `GpuWaterParams[]` SSBO and the push
     block became a compact `{ uint waterIndex; uvec3 _reserved; }` index.
  3. `docs/engine/shader-pipeline.md` §"G-Buffer Layout" ends with "After
     `vkCmdEndRenderPass` all attachments transition to
     `SHADER_READ_ONLY_OPTIMAL`." The depth attachment's `final_layout` is
     `DEPTH_STENCIL_READ_ONLY_OPTIMAL`, not `SHADER_READ_ONLY_OPTIMAL` — and
     that distinction is load-bearing three paragraphs later, where the same
     doc correctly names `DEPTH_STENCIL_READ_ONLY_OPTIMAL` as
     `copy_depth_to_history`'s precondition, and again in
     `depth_capture_record_copy`'s contract (#3628). The doc contradicts
     itself; the code is right.
- **Evidence**: as cited per item above.
- **Impact**: Documentation only. Item 3 is the one worth prioritising — it
  sits in the file the audit skill designates authoritative for G-buffer
  layout, and it disagrees with a layout precondition two other subsystems
  now depend on by name.
- **Related**: #3881 (`bf8ded3d`, which fixed the sibling stale pointer in the
  same comment block), #3282 (the split that moved RP-1), #3628 (the depth
  layout contract), #2757 (the "line numbers rot, name the symbol" rule this
  keeps re-proving).
- **Suggested Fix**: Point item 1 at `build_and_upload_instances`; change item
  2's "128-byte" to "16-byte" (or drop the size and name `WaterPush`); qualify
  item 3 to "all eight colour attachments transition to
  `SHADER_READ_ONLY_OPTIMAL`; depth transitions to
  `DEPTH_STENCIL_READ_ONLY_OPTIMAL`".

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
