# REN-D4-06: shader-pipeline.md Per-Frame Submission Order omits copy_depth_to_history step and health-counter harvest

- **Severity**: LOW
- **Dimension**: 4
- **Labels**: low,renderer,documentation

## Description
The authoritative 22-step order omits `copy_depth_to_history` (a whole `TRANSFER`-stage step between 5 and 6 that transitions the depth image twice) and the step-21 health-counter harvest. `depth_history_image` is absent from the doc entirely, including the G-Buffer table — and **#2484 and #2485 are open findings about exactly that barrier and that image**.

## Location
`docs/engine/shader-pipeline.md` (§ Per-Frame Submission Order)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D4-06).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2788
