# REN-D9-2026-08-12-05: shader-pipeline.md wrongly claims skin_vertices.comp deforms normals; MAX_TOTAL_BONES factorisation off by 192

- **Severity**: LOW
- **Dimension**: 9
- **Labels**: low,renderer,documentation

## Description
(a) Compute table says `skin_vertices.comp` deforms "positions **/ normals**"; it has been position-only since #2170 (`SKIN_OUTPUT_STRIDE_FLOATS = 3`), and the shader body says so — so the doc contradicts the code's own explanation of a live behavioural gap. (b) `MAX_TOTAL_BONES` factorised as 144 × 1364 = 196 416 ≠ 196 608, omitting the reserved identity slot 0.

## Location
`docs/engine/shader-pipeline.md`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D9-2026-08-12-05).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2807
