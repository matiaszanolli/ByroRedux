# REN-D1-02: build_tlas second LRU-stamp pass over draw_commands is not equivalent to the first

- **Severity**: LOW
- **Dimension**: 1
- **Labels**: low,renderer,bug

## Description
A second full pass over `draw_commands` re-bumps LRU stamps `build_tlas_instances` already set; the two passes are not equivalent — the second also protects BLAS the first dropped on the `missing_ssbo_instance` arm. Predates the #2259 split.

## Location
`crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas`)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D1-02).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2769
