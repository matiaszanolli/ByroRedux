# REN-D10-04: shader-pipeline.md cites stale cell_loader/references.rs path

## Description
Cites *cell_loader/references.rs* (as written in the doc) for the `RT_ABSOLUTE_PRECISION_CEILING` guard; `byroredux/src/cell_loader/references/` is a **directory** — the constant and predicate are in `byroredux/src/cell_loader/references/mod.rs`, the firing `debug_assert!` in `byroredux/src/cell_loader/references/complete.rs`. Everything else in the section is accurate.

## Location
`docs/engine/shader-pipeline.md` (§ Absolute world space — f32 ceiling)

## Severity / Domain / Type
low / renderer / documentation

https://github.com/matiaszanolli/ByroRedux/issues/2755

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D10-04).
