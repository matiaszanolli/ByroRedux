# REN-D2-03: shader-pipeline.md Set-1 table describes binding 11 as bare u32, wrong by 28 B since GpuRayBudget

- **Severity**: LOW
- **Dimension**: 2
- **Labels**: low,renderer,documentation

## Description
Binding 11 still described as a bare `u32` (it has been an 8-word `GpuRayBudget` since `5798e467`, so a range/flush/barrier sized from the row is wrong by 28 B); bindings 8/9/13 listed triangle-only though `water.frag` statically reads all three.

Note: table marks this "NEW — merged with stale-run `REN-D2-01`, file once" — filed as a single issue per that note.

## Location
`docs/engine/shader-pipeline.md` (Set-1 table)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D2-03).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2781
