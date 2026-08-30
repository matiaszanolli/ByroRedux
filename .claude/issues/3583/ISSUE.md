# #3583 — REN-2026-08-30-D4-04: `screenshot_record_copy`'s `# Safety` contract still names composite as the swapchain writer — the same stale attribution #2786 fixed next door in `egui_pass.rs`

**Labels**: `low,renderer,sync,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3583 --json state`.

---

- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/context/screenshot.rs:144–153` (`VulkanContext::screenshot_record_copy` doc + `# Safety` block)
- **Status**: NEW — doc-in-code is wrong, code is right.
- **Description**: The function's prose says "Called in `draw_frame()` **after
  composite dispatch** … The swapchain image is in `PRESENT_SRC_KHR` layout
  **after the composite pass**", and the `# Safety` clause says the image
  "must currently be in `PRESENT_SRC_KHR` layout (**this frame's composite
  pass output**)". Since the FSR tail landed, composite writes a
  render-resolution HDR intermediate and never touches the swapchain; the
  swapchain writer is `PresentationPipeline` (`presentation.rs`, attachment
  `UNDEFINED → PRESENT_SRC_KHR`), or `EguiPass` (`LOAD` op,
  `PRESENT_SRC_KHR → PRESENT_SRC_KHR`) when the debug overlay is active — and
  `screenshot_record_copy` is called after *both*, at the tail of
  `draw_frame`'s `unsafe` block. The *layout* half of the contract is still
  correct; only the attribution is wrong. `#2786` fixed precisely this stale
  "composite writes the swapchain" claim in `egui_pass.rs` and did not sweep
  this sibling.
- **Evidence**:
  - `crates/renderer/src/vulkan/presentation.rs` — attachment
    `.initial_layout(vk::ImageLayout::UNDEFINED)
    .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)`.
  - `crates/renderer/src/vulkan/egui_pass.rs:353–358` — `LOAD` / `STORE`,
    `PRESENT_SRC_KHR` on both ends; and its `in_dep` comment, corrected under
    #2786, spelling out "that pass is `PresentationPipeline`
    (`presentation.rs`), not composite".
  - `crates/renderer/src/vulkan/context/draw.rs` records, in order,
    `record_post_passes(...)` (which ends in `record_presentation_pass`), then
    the egui `pass.dispatch(...)`, then `self.screenshot_record_copy(cmd,
    swapchain_image);`.
- **Impact**: The `# Safety` precondition of an `unsafe fn` names the wrong
  producer. A future reader auditing whether the `PRESENT_SRC_KHR` precondition
  still holds will go and check composite, which is now irrelevant to it.
- **Needs RenderDoc**: no
- **Suggested Fix**: Reword both the prose line and the `# Safety` clause to
  name the presentation pass (and egui when active) as the producer, mirroring
  the #2786 wording in `egui_pass.rs`. No code change.

---
- **Cross-dimension corroboration**: Found independently twice — also as *D20-02*, which lists three further sites in the egui overlay path carrying the same stale "composite writes the swapchain" attribution.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D4-04

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
