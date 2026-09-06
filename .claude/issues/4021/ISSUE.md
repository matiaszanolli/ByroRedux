# #4021 — REN-2026-09-06-D20-01: #3628's second pin anchors on a conditionally-executed call and its hazard scan is blind to the file's own barrier idiom

**Labels**: low, renderer, sync, test-gap, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D20-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/context/depth_capture.rs` (`capture_ordering_tests::record_copy_runs_immediately_after_the_depth_history_copy`); subject: `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`'s frame tail), `crates/renderer/src/vulkan/context/post_passes.rs` (`copy_depth_to_history`)
- **Status**: NEW
- **Description**: Pin (a) of `229306ce` genuinely pins what it names (verified
  above). Pin (b) does not — it asserts two adjacent facts rather than the
  ordering invariant it describes.

  1. **Its stated rationale is false.** The assertion message reads
     *"depth_capture_record_copy must come AFTER copy_depth_to_history — it
     documents DEPTH_STENCIL_READ_ONLY_OPTIMAL as its precondition, and that
     layout is only guaranteed once the history copy's own barriers have run."*
     The history copy is wrapped in `if has_effect_soft_material { … }`, so it
     does not run at all on the common path. The actual guarantor is the main
     render pass's own `.final_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)`
     in `create_render_pass` (`crates/renderer/src/vulkan/context/helpers.rs`) —
     which `draw.rs`'s own comment at the call site states correctly
     (*"the render pass leaves the depth image in DEPTH_STENCIL_READ_ONLY_OPTIMAL …
     when the copy is skipped, the layout is already the precondition"*) and
     `copy_depth_to_history`'s doc in `post_passes.rs` also states
     (*"`DEPTH_STENCIL_READ_ONLY_OPTIMAL` (the render pass's final layout)"*).
     Three sites agree; the pin's message is the outlier.
  2. **The guarded window is the wrong one.** The hazard scan covers
     `src[history_copy_pos..record_copy_pos]` — roughly the four lines from
     inside the `if` block to the capture call. The real hazard region is from
     the render pass end to `depth_capture_record_copy`; anything added before
     the `if has_effect_soft_material` block is outside the scan. A `memory_barrier(…)`
     call already sits there today (harmless — it is a global `VkMemoryBarrier`,
     no image layout), which shows the region is actively edited.
  3. **The hazard list misses the codebase's own barrier idiom.** It greps for
     the literal `cmd_pipeline_barrier`, `cmd_copy_image(`,
     `cmd_copy_image_to_buffer(`, `cmd_blit_image(`. `crates/renderer/src/vulkan/descriptors.rs`
     exports `memory_barrier(...)` plus eight `image_barrier_*` builders, and
     `draw.rs` uses `memory_barrier(...)` fifteen lines above the scanned window.
     A layout transition introduced through any of those helpers — the file's
     dominant style — passes the scan unchanged. `cmd_pipeline_barrier2` and
     `cmd_clear_depth_stencil_image` are likewise unlisted.
- **Evidence**: The `if has_effect_soft_material {` wrapper around
  `self.copy_depth_to_history(cmd);` in `draw.rs`; the `final_layout` call in
  `helpers.rs::create_render_pass`; the `memory_barrier` / `image_barrier_*`
  exports in `descriptors.rs` versus the four-string hazard array in the test.
- **Impact**: Documentation-and-test only. The invariant holds at HEAD. But the
  pin is the sole guard on a precondition whose violation surfaces as a
  validation-layer error or garbage `depth.stats` output rather than a build or
  `cargo test` failure — exactly what #3628 was filed to prevent — and it would
  not fire on the most likely way to break it.
- **Related**: #3628, #3308, #2484, #3570; `depth_format_guard_tests` (the
  sibling source-scan in the same file, which *is* tight).
- **Suggested Fix**: Anchor the scan on the render pass end
  (`cmd_end_render_pass`) rather than on the conditional history copy, widen
  the hazard list to include `memory_barrier(`, `image_barrier_`,
  `cmd_pipeline_barrier2`, and `cmd_clear_depth_stencil_image`, and rewrite the
  assertion message to name the render pass's `final_layout` as the guarantor.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
