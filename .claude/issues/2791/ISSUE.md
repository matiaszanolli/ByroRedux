# REN-D5-04: allocate_scene_render_buffers bind-inverse staging comment understates size by 87x (144 KB vs ~12.6 MB)

- **Severity**: LOW
- **Dimension**: 5
- **Labels**: low,renderer,documentation

## Description
The bind-inverse staging comment computes "16 × 144 × 64 ≈ 144 KB"; the constant is 1366, making it **≈ 12.6 MB** — an 87× understatement next to the second-largest host-visible allocation the renderer makes. `constants.rs` and `memory-budget.md` are both correct; only this site is stale.

## Location
`crates/renderer/src/vulkan/scene_buffer/buffers.rs` (`allocate_scene_render_buffers`)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D5-04).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2791
