# REN-D4-02: Per-swapchain-image render_finished contract is prose-only, no test coverage

- **Severity**: LOW
- **Dimension**: 4
- **Labels**: low,renderer,sync,tech-debt,bug

## Description
The per-swapchain-image `render_finished` contract (`548c1b69`, VUID-…-00067) is prose-only — 6 grep hits, none in a `#[cfg(test)]` block.

## Location
`crates/renderer/src/vulkan/sync.rs`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D4-02).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2783
