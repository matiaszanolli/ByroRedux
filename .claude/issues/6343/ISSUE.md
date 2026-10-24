# REN-D5-01: Failed one-time layout walk leaves G-buffer, bloom and SVGF/TAA/caustic resize paths running with UNDEFINED images

- **Issue**: #6343 (snapshot as filed; GitHub is authoritative for live state)
- **Labels**: high, renderer, vulkan, sync, bug
- **Source**: docs/audits/AUDIT_RENDERER_2026-10-24.md

- **Severity**: HIGH (Vulkan spec violation floor; error path only — the trigger is a failed command-buffer allocation, begin/end, or a `vkQueueSubmit` OOM; downgrade to MEDIUM if likelihood is weighed over the floor)
- **Dimension**: Memory/Lifecycle (found independently by Dim 4 as REN-D4-01, Sync/Barriers; merged here)
- **Location**:
  - `VulkanContext::new` (`crates/renderer/src/vulkan/context/init.rs`): the G-buffer arm ("G-buffer layout init failed") and the bloom arm ("Bloom pyramid layout init failed", then `Some(b)`).
  - `recreate_swapchain` helpers in `context/resize.rs`: "G-buffer post-resize layout init failed" and "Bloom layout re-init after resize failed".
  - `TaaPipeline::recreate_on_resize` (`taa.rs`), `SvgfPipeline::recreate_on_resize` (`svgf.rs`), `CausticPipeline::recreate_on_resize` (`caustic.rs`): `if let Err(e) = unsafe { self.initialize_layouts(...) } { log::warn!("... layout re-init after resize failed: {e}"); } Ok(())`.
- **Status**: NEW. Sibling of closed #1389 ("Volumetrics survives failed initialize_layouts leaving froxel images UNDEFINED"), which fixed volumetrics only. #6146 (closed) fixed the *destroy/leak* half of these callers; these *keep* arms consume no in-flight class and were out of its scope. #677 and #1031 (closed) added the resize walks but left the failure arm log-only.
- **Description**: `submit_initial_layouts` (`texture.rs`) is the only thing that moves these images out of `UNDEFINED`. When it fails with `OneTimeCommandError::NotSubmitted` (allocate, begin, end, reset-fence or create-fence failure) the command buffer was never queued; a `vkQueueSubmit` OOM classed `MaybeInFlight` has the same effect, because a failed submit leaves the referenced resources unaffected. The listed arms only `log::warn!` and keep the pass. The next frame's commands then assume the walked layout:
  - bloom's per-dispatch barriers use `.old_layout(vk::ImageLayout::GENERAL)` (`bloom.rs`) — an image actually in `UNDEFINED` breaks VUID-VkImageMemoryBarrier-oldLayout-01197;
  - SVGF samples the previous-frame G-buffer slot as `SHADER_READ_ONLY_OPTIMAL` (the doc comment on `GBuffer::initialize_layouts` names this exact validation error);
  - SVGF, TAA and caustic bind their history/accumulator images as `GENERAL` storage.
  The policy is also asymmetric: the init arms for SVGF and TAA disable the pass on the same failure (`svgf.take()` / `taa.take()` + `release_after_failed_submit`), volumetrics follows the #1389 rule, and caustic init returns `Err` — but the resize arms inside those same passes keep running and report `Ok(())`, so the context cannot tell.
- **Evidence**: orchestrator re-read the warn-and-continue arms in `init.rs` (bloom at "Bloom pyramid layout init failed", G-buffer at "G-buffer layout init failed"), `resize.rs` ("G-buffer post-resize layout init failed") and `taa.rs` (`recreate_on_resize`); the contrast arms are the TAA init arm and the volumetrics init arm.
- **Impact**: after a transient one-time-submit failure, every later frame records layout-mismatched barriers and descriptor reads for these passes: validation errors and undefined image contents — garbage history, bloom and indirect, not a crash. No use-after-free: nothing is destroyed on these arms, so the #6146 in-flight policy is not violated. Composite binds bloom's views (binding 7) and SVGF/TAA history, so bloom and the G-buffer cannot simply be dropped.
- **Related**: #1389, #6146, #6063, #677, #1031, #905 (bloom binding 7 must not dangle), REN-D5-02.
- **Suggested Fix**: a failure-policy change, not a barrier edit. Make the pass-internal `recreate_on_resize` bodies return the layout error so `recreate_swapchain` can apply `release_after_failed_submit` + `None` like its TAA/water-caustic arms; for bloom and the G-buffer (mandatory for composite) propagate `Err` as caustic init already does rather than run with `UNDEFINED` images. **NEEDS-RENDERDOC / `BYRO_VALIDATION=1`**: confirming the VUID needs a fault-injected one-time-submit failure during resize; no such hook exists (it would need an env switch in the style of `BYRO_FSR_FORCE_DISPATCH_FAIL`).


## Completeness Checks
- [ ] **SIBLING**: every `initialize_layouts` caller is covered — bloom and G-buffer (init + resize), and the `recreate_on_resize` bodies of SVGF, TAA and caustic; the shadow-filter, water-caustic and volumetrics arms keep their existing policy
- [ ] **DROP**: where a pass is dropped on failure it goes through `release_after_failed_submit` (destroy on NotSubmitted, leak on MaybeInFlight); reverse-order teardown and the `partial` cleanup order stay correct
- [ ] **RESIZE**: `recreate_swapchain` rebinds every descriptor naming a dropped pass's views (composite binding 7 for bloom must not dangle, #905)
- [ ] **VALIDATION**: a fault-injected one-time-submit failure under `BYRO_VALIDATION=1` shows the layout VUID before the fix and not after (no hook exists; add one in the style of `BYRO_FSR_FORCE_DISPATCH_FAIL`) — NEEDS-RENDERDOC, no barrier edit proposed
- [ ] **TESTS**: a regression pin per arm (see REN-D5-02 for the scan-coverage gap)

_Filed from `docs/audits/AUDIT_RENDERER_2026-10-24.md` (static review; no engine or Vulkan device was launched). Failures invisible to `cargo test` are flagged NEEDS-RENDERDOC in the report._
