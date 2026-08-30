# #3631 — REN-2026-08-30-D23-01: both authoritative FSR docs still carry "UI composited before upscale" as open scope after #3426 closed it

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3631 --json state`.

---

- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**: `docs/engine/fsr3-troubleshooting.md` (lines 74–79), `docs/engine/fsr3-upscaler-integration-plan.md` (lines 3–7, 30–33, 137, 152, 158, 641, 735–738)
- **Status**: NEW
- **Description**: Commit `b28acb0c` (#3426) moved the Scaleform/Ruffle overlay draw out
  of the geometry pass and into `PresentationPipeline::dispatch`, i.e. after tone-map and
  after upscale, at output resolution. Both documents the audit skill names as
  authoritative for this dimension still describe the pre-#3426 world, including an
  operator-facing troubleshooting entry that tells the reader the ghosting is expected and
  "the fix is moving it after upscale".
- **Evidence**:
  - `crates/renderer/src/vulkan/presentation.rs` (`UiOverlayDraw`, `record_overlay`,
    `dispatch(..., overlay: Option<UiOverlayDraw>)`) and
    `crates/renderer/src/vulkan/pipeline.rs` (`create_ui_pipeline`, now built against the
    presentation render pass with a single colour-blend attachment) implement the move; the
    `ui_overlay_composites_after_the_tone_map_draw` test in `presentation.rs` pins it and
    also pins that `context/geometry_pass.rs` no longer contains `pipeline_ui`.
  - `fsr3-troubleshooting.md:77` — "**The Scaleform/Ruffle UI overlay.** It is still
    composited *before* the upscale, so it goes through temporal reconstruction… the fix is
    moving it after upscale."
  - `fsr3-upscaler-integration-plan.md:5-7` — "Four items are carried as known scope rather
    than done: … the two phase-4 items below (transparency split, UI composited after
    upscale) remain open"; `:33` — "the Scaleform/Ruffle overlay + reticle are still
    composited before upscale rather than after (4.5)"; `:735-738` — "Until the UI moves,
    the Scaleform overlay is temporally reconstructed along with the scene and writes no
    mask".
  - The reticle half of item 4.5 is also done and always was post-presentation: the only
    crosshair in the tree is `crates/debug-ui/src/panels.rs` (`show_crosshair`), drawn by
    the egui pass, which `context/draw.rs:3721` records *after* the presentation pass.
- **Impact**: Doc-only. But this dimension's whole method is "verify the premise against
  current code", and these two files are the premise source. A future auditor or fixer
  reading them will re-file a closed item, or chase UI ghosting that the code no longer
  produces. Exactly the stale-premise class `feedback_audit_findings` exists for.
- **Needs RenderDoc**: no
- **Suggested Fix**: In `fsr3-troubleshooting.md`, delete the UI bullet from the
  "expected to ghost" list (or rewrite it as "the overlay is composited after upscale since
  #3426 and is never reconstructed"). In `fsr3-upscaler-integration-plan.md`, move 4.5 from
  carried scope to complete in the status header, §"Phase 4 landed", and the phase-5
  deferral note, leaving 4.1 (transparency split) and the FP32 permutation as the genuine
  carried items.

---
- **Cross-dimension corroboration**: Found independently four times — also as *D8-01*, *D11-06* and *D4-02*. `#3426` invalidated the same claim in two FSR documents plus the frame-graph prose; one fix closes all four.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D23-01

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
