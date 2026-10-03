# #5205 — REN-D4-2026-10-03-02: shader-pipeline.md still says only material kind 0 takes the early-test pipeline (stale since #5057), and names `context/draw.rs` as the uploader of `renderOrigin.w`

**Labels**: low,renderer,pipeline,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (doc)
- **Location**:
  - `docs/engine/shader-pipeline.md` §Per-Frame Submission Order, step 6: "Certified opaque batches (`DrawCommand::allows_early_fragment_tests`: no blend, no alpha test, material kind 0, …) bind `pipeline_early`".
  - The same file, §Render-origin-relative: "`renderOrigin.w` … uploaded in `context/draw.rs`".
- **Status**: NEW. CLOSED #4958/#5023 covered recorder names and the reactive mask, not this.
- **Description**:
  - Since `3c197ed8c` (#5057), `DrawCommand::allows_early_fragment_tests` admits `material_kind <= MATERIAL_KIND_MAX_LIGHTING_SHADER` (16). The doc's "material kind 0" now under-describes the certificate. A reader auditing early-Z soundness for kinds 1–16 would wrongly conclude they still go through late tests.
  - The `renderOrigin.w` FSR-reset upload is written in `assemble_camera_and_lights.rs`, which carries the comment "FSR one-frame-reset flag, read by `triangle.frag`'s FSR-reset…". This stale pointer predates the window; it dates from the #3282 split.
- **Impact**: Documentation only. The doc is the "code-verified reference" that the skill audits against.
- **Suggested Fix**:
  - Step 6: "material kind 0..=`MATERIAL_KIND_MAX_LIGHTING_SHADER` (16, the reviewed BSLightingShaderProperty types; review pinned by `early_fragment_kinds_have_no_discard_or_depth_write_path`)".
  - Change the `renderOrigin.w` pointer to `context/assemble_camera_and_lights.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)
