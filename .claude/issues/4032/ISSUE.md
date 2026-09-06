# #4032 — REN-2026-09-06-D4-02: `copy_depth_to_history` became conditional under #3667, and neither the authoritative doc nor #3628's ordering pin (landed three days later) says so

**Labels**: low, renderer, shaders, sync, test-gap, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D4-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `docs/engine/shader-pipeline.md` (§"Per-Frame Submission Order",
  steps 7 and 7b) and `crates/renderer/src/vulkan/context/depth_capture.rs`
  (`capture_ordering_tests::record_copy_runs_immediately_after_the_depth_history_copy`
  — its doc block and its first `assert!` message)
- **Status**: NEW — doc + test-rationale wrong, code right
- **Description**: `1bf64187` (#3667, 2026-09-03) put `copy_depth_to_history`
  behind `if has_effect_soft_material`, a per-frame `FrameInputs` boolean. It is
  therefore skipped on most frames of most cells. Two places still describe it
  as unconditional, and both use it as the *source* of the depth image's layout:
  1. `shader-pipeline.md` step 7 lists it as an unconditional pass, and step 7b
     says `depth_capture_record_copy` is *"recorded immediately after step 7,
     **which leaves the depth image back in `DEPTH_STENCIL_READ_ONLY_OPTIMAL`**"*.
  2. #3628's pin asserts *"that layout is only guaranteed once the history
     copy's own barriers have run"*.

  Both are inverted for the common path. The layout comes from
  `create_render_pass`'s depth-attachment `final_layout`
  (`DEPTH_STENCIL_READ_ONLY_OPTIMAL`, `context/helpers.rs`); the history copy
  merely *restores* it when it runs. `draw_frame`'s own inline comment gets this
  right (*"when the copy is skipped, the layout is already the precondition
  `depth_capture_record_copy` requires and restores"*) — the doc and the test
  message do not.

  This is not only cosmetic: the pin's hazard scan
  (`for hazard in ["cmd_pipeline_barrier", "cmd_copy_image(", …]`) is applied to
  `src[history_copy_pos..record_copy_pos]`, a window that *starts at a call that
  usually does not execute*. The window that actually protects
  `depth_capture_record_copy`'s stated precondition starts at the render pass
  end. A depth-image layout transition inserted between `record_geometry_pass`
  and the `if has_effect_soft_material` block — or inside that block ahead of the
  copy — would satisfy the pin and still break the precondition.
- **Evidence**:
  - `crates/renderer/src/vulkan/context/draw.rs` — `self.copy_depth_to_history(cmd);`
    is inside `if has_effect_soft_material { … }`; `self.depth_capture_record_copy(cmd);`
    is outside it. `has_effect_soft_material` is destructured from `FrameInputs`
    at the top of `draw_frame`.
  - `git log -1 --format=%ad --date=short 1bf64187` → `2026-09-03`;
    `229306ce` (#3628) → `2026-09-06`.
  - `grep -n "has_effect_soft_material" docs/engine/shader-pipeline.md` → no hits.
  - `context/helpers.rs::create_render_pass` — depth attachment
    `.final_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)`.
- **Impact**: The one doc `/audit-renderer` designates authoritative for frame
  ordering, and the one test that pins this ordering, both attribute a layout
  guarantee to a pass that usually does not run. A future reader narrowing or
  removing the render pass's depth `final_layout` would find nothing objecting.
  No runtime misbehaviour.
- **Related**: #3667 (the gating change), #3628 (the pin), #2484 (the barrier
  whose src scope the pin's rationale describes), *REN-2026-08-30-D4-01*
  (the earlier, now-fixed gap in the same doc block).
- **Needs RenderDoc**: no.
- **Suggested Fix**: Mark step 7 conditional in `shader-pipeline.md` and move
  the layout attribution in step 7b to the render pass's depth `final_layout`.
  In `capture_ordering_tests`, restate the assert message the same way and
  consider widening the hazard scan's start anchor from
  `self.copy_depth_to_history(cmd);` to the `record_geometry_pass` call, so the
  window matches the precondition it claims to guard. No code change.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
