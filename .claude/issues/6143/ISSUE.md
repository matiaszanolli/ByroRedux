# TD1-2026-10-19-07: Frame and Vulkan driver functions of 580-1373 lines have no regrowth budget (only `draw_frame` does); two are still growing (regression of #5096)

Labels: low,renderer,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (fn > 200, nesting > 5)
- **Location** (HEAD LOC / nesting): `renderer/src/vulkan/context/init.rs:527` `build_pipelines_and_finish` 1373 / 4 (+40 in 4 days); `context/build_and_upload_instances.rs:153` 1289 / 3 (+39); `byroredux/src/app_events.rs:681` `about_to_wait` 1032 / 7; `byroredux/src/render/static_meshes.rs:326` `collect_static_mesh_draws` 1031 / 4; `context/skinned_blas_refit.rs:78` 978 / **10**; `acceleration/blas_static.rs:266` `build_blas_batched` 975 / 5; `context/geometry_pass.rs:21` 761 / 8; `byroredux/src/app_frame.rs:110` `render_one_frame` 701 / 7; `physics/src/water.rs:726` `apply_buoyancy_with_scratch` 682 / 7; `cell_loader/references/synth_child.rs:284` 661 / 6; `cell_loader/references/mod.rs:296` `load_references_budgeted` 580 / **9**
- **Status**: Existing: #5096 (closed; the budget there is `DRAW_FRAME_LINE_BUDGET = 720`, `draw.rs:1403`, which `draw_frame` sits exactly on at 720). The other eleven have no pin; `build_pipelines_and_finish` and `build_and_upload_instances` regrew since 10-15.
- **Effort**: small per function (budget), medium per extraction
- **Description / extraction proposals** (outlines read; none touches barrier order):
  1. `about_to_wait`: the `--bench-frames` summary branch L1170-~1650 (~480 lines, 60 print/format sites, nest 7, includes the screenshot-claim state machine at L1590-1650) becomes `fn emit_bench_summary(&mut self)` + `fn advance_screenshot_request(&mut self)`; leaves the per-tick driver near 450.
  2. `build_pipelines_and_finish`: numbered construction steps (12c cluster cull, 12d skin, 12d.5 palette, 14 graphics, 15a water, groundcover blade/model tiers, ...) become one `build_<stage>(..) -> Result<..>` per stage returning the owning struct; construction only, no recording.
  3. `build_and_upload_instances`: instance-SSBO walk L188-~760, ReSTIR/caustic rig-key folding L760-~880, BLAS-key creation pass L1055-1230, TAA/skin recovery L1233-1372 become four private methods around the existing output type (L87).
  4. `collect_static_mesh_draws`: ~14 hoisted query bundles + per-entity resolve; split `StaticDrawQueries::new(world)` from `fn emit_draw(&mut out, entity, ..)`.
  5. `record_skinned_blas_refit` (nest 10) and `build_blas_batched` (phases 1-7 are numbered in its comments): one fn per phase/bracket (`#1194` skin-dispatch bracket L527-609, refit bracket L799-901). **Recording-adjacent**: extract verbatim, keep call order and barrier/bracket placement identical, verify under `BYRO_VALIDATION=1` (feedback_speculative_vulkan_fixes).
  6. `load_references_budgeted` (nest 9) and `import_embedded_animations` (`nif/src/anim/entry.rs:368`, 433 / nest 9): flatten with early-continue helpers per phase.
- **Evidence**: `fn_head.tsv`; section markers from the comment headers of each function.
- **Proposed fix**: first, copy the `draw.rs:1403-1457` budget test pattern (source-scan of the fn's line count) for these eleven at their current size so growth is a deliberate edit; then extract in the order above.

Related: #5096 (closed; its budget covers only `draw_frame`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
