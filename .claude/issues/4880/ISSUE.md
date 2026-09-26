# REN-D5-2026-09-26-02: `record_dds_upload` — a failing `create_image_view` or `allocate` leaks the image, and on the batched flush path a post-record failure submits a command buffer that reads a destroyed staging buffer

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4880

**Labels**: high,renderer,vulkan,memory,bug

**Relationship to #4854 (open):** #4854 covers only the `create_image_view` failure arm (leak of the image and its allocation). This issue files the parts it does not describe: (b) on the batched flush path a post-record failure submits a command buffer that copies from a destroyed staging buffer, and (c) the `allocate` failure arm also leaks the `VkImage`. Fix all three arms together; #4854 can be closed by the same change.

- **Severity**: HIGH (a Vulkan spec violation is at least HIGH. It is failure-path only, but the failure is the VRAM-OOM the engine is most likely to meet.)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/texture.rs` (`Texture::record_dds_upload`: `create_image(..)?`, `.allocate(..).context("Failed to allocate DDS texture image memory")?`, the bind arm, the tail `create_image_view(..).context("Failed to create DDS texture image view")?`), consumer `crates/renderer/src/texture_registry/upload.rs` (`flush_upload_batch` record closure)
- **Status**: **Existing #4854** (the view-arm leak; verified still current) **+ NEW** (the submit-after-free half and the allocate-arm image leak)
- **Description / Evidence**:
  - (a) #4854 holds. After `bind_image_memory` succeeds, a `create_image_view` failure returns via `?` with the `VkImage` and its allocation un-owned.
  - (b) NEW. In the function body the order is: bind → `cmd_pipeline_barrier` → `cmd_copy_buffer_to_image` → `cmd_pipeline_barrier` → `create_image_view(..)?`.
    - The early return drops the local `StagingGuard`, which destroys the staging buffer and frees its allocation.
    - In `flush_upload_batch` the record closure logs "dropping queued upload" and `continue`s, then returns `Ok(())`, so `with_one_time_commands_reuse_fence` submits the command buffer.
    - That buffer still contains a `vkCmdCopyBufferToImage` from the destroyed buffer (VUID-vkQueueSubmit-pCommandBuffers-00070 class). Once the sub-allocation is freed, the GPU can read released device memory.
    - The synchronous `from_dds_with_mip_chain` path is safe: `with_one_time_commands` frees a command buffer whose closure returned `Err` without submitting. Only the batched path reaches the hazard.
  - (c) NEW. The `allocate(..)?` failure (VRAM OOM, the most plausible failure in this function) leaks the just-created `VkImage`. The #2178 fix covered only the bind arm.
  - `image::tests::no_file_outside_this_module_rolls_its_own_image_chain` allow-lists `texture.rs` as a documented specialisation, and no test covers its error arms.
- **Impact**: One leaked `VkImage` handle per failed upload, plus a GPU read of freed memory on the batched path. Trigger is OOM-class, so likelihood is low and impact is not.
- **Related**: #2178, #4854, #2164 (the staging-side analog, fixed in `create_staging_buffer`).
- **Suggested Fix**: Fix by reordering rather than adding cleanup. Create the image view (CPU-only) immediately after bind and before recording, with one unwind (view → image → allocation) for every arm, so nothing is recorded until every fallible step has succeeded. Better still, route image creation through `GpuImage::create` in `crates/renderer/src/vulkan/image.rs`, which already implements the create → allocate → bind → view chain with the #2178/#4089 unwinding. Pin it with a source-scan asserting no `?` after the first `cmd_pipeline_barrier`. **Needs a `BYRO_VALIDATION=1` fault-injection run** (forced `vkCreateImageView` failure) to observe the submit-with-destroyed-buffer error.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **DROP**: If Vulkan objects change, the Drop impl and teardown ordering are still correct
- [ ] **SIBLING**: Same pattern checked in related files (sibling constructors / upload paths / docs)
- [ ] **TESTS**: A regression test pins this specific fix
