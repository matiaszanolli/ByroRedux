# #3579 — REN-2026-08-30-D4-01: the authoritative submission-order doc is missing the #3308 depth-capture copy and never places the Scaleform overlay draw

**Labels**: `low,renderer,sync,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3579 --json state`.

---

- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `docs/engine/shader-pipeline.md` (§ "Per-Frame Submission Order", the fenced block at lines 69–141)
- **Status**: NEW — doc is wrong, code is right.
- **Description**: The audit skill designates this block the authoritative
  per-frame ordering reference, and it enumerates every pass including the
  ones whose only content is a barrier (step 8 "G-buffer →
  `SHADER_READ_ONLY_OPTIMAL`", step 9 "caustic accum atomic-add →
  `SHADER_READ`"). Two things in the current frame graph are absent from it:
  1. **`depth_capture_record_copy` (#3308).** Recorded immediately after step
     7 (`copy_depth_to_history`) in
     `crates/renderer/src/vulkan/context/draw.rs` and it is *not* a no-op
     pass: it performs two `cmd_pipeline_barrier` calls that move
     `self.depth_image` `DEPTH_STENCIL_READ_ONLY_OPTIMAL → TRANSFER_SRC_OPTIMAL
     → DEPTH_STENCIL_READ_ONLY_OPTIMAL` around a `cmd_copy_image_to_buffer`
     (`crates/renderer/src/vulkan/context/depth_capture.rs:205` and `:223`),
     between the depth-history copy and every later depth consumer (SSAO,
     SVGF, composite, FSR). A reader reasoning about depth-image layout from
     this doc will not know that pass exists.
  2. **The Scaleform overlay draw.** The block never places it — not in step 6
     (the main render pass, where it lived until #3426) and not in step 20
     (the presentation pass, where `PresentationPipeline::record_overlay` now
     records it). The shader table at lines 29–30 lists `ui.vert` / `ui.frag`
     with no home.
- **Evidence**:
  - `crates/renderer/src/vulkan/context/draw.rs` calls
    `self.copy_depth_to_history(cmd);` then `self.depth_capture_record_copy(cmd);`
    back to back; the doc's step 7 covers only the first.
  - `grep -n "depth_capture\|#3308" docs/engine/shader-pipeline.md` → no hits.
  - `grep -n "ui\.vert\|ui\.frag\|overlay" docs/engine/shader-pipeline.md` →
    only lines 29–30 (the shader table), nothing in the order block.
  - `crates/renderer/src/vulkan/presentation.rs`
    (`PresentationPipeline::dispatch` → `record_overlay`) is the current
    recording site, pinned by the in-file test
    `ui_overlay_composites_after_the_tone_map_draw`.
- **Impact**: The doc a future barrier investigation is told to trust omits a
  pass that transitions the depth image and misplaces the only draw that was
  relocated across the tone-map boundary in this delta. Nothing misbehaves at
  runtime.
- **Needs RenderDoc**: no
- **Suggested Fix**: Insert a step between 7 and 8 for
  `depth_capture_record_copy` (naming its two depth transitions and that it
  restores `DEPTH_STENCIL_READ_ONLY_OPTIMAL`), and extend step 20 to say the
  Scaleform overlay quad is recorded in the same subpass after the tone-map
  triangle (#3426). No code change.

---
- **Cross-dimension corroboration**: Found independently three times — also as *D0-01* (orchestrator) and *D8-03* (denoiser/composite). Severity arbitrated **down** to LOW: it is documentation-only, and unlike open `#3447` (wrong byte counts in a GPU layout contract, same doc) a missing pass row misleads rather than miscomputes. The `audit-renderer` SKILL's hand-written *"a finding that places the UI quad at the tail of the geometry pass is written against the pre-`#3426` shape"* warning is a maintenance workaround for exactly this gap.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D4-01

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
