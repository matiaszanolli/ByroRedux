# REN-D1-05: shrink_tlas_scratch_to_fit case-2 live-slot realloc arm appears unreachable

- **Severity**: LOW
- **Dimension**: 1
- **Labels**: low,renderer,tech-debt,bug

## Description
The live-slot realloc arm appears unreachable — `current` and `peak` are written together in `ensure_tlas_state` and differ by ≤ `scratch_align − 1`, so `current > 2 × peak` cannot hold. All reclamation flows through case 1. Unit tests on the predicate give false confidence #1226 revived it. Confirm with a one-shot `log::debug!` before touching; **do not** change the shrink/destroy ordering (that is the #1782-class safety property).

## Location
`crates/renderer/src/vulkan/acceleration/memory.rs` (`shrink_tlas_scratch_to_fit`, case 2)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D1-05).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2774
