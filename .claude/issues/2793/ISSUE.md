# REN-D5-05: collect_image_health writes through mapped_slice_mut with no invalidate/flush primitive documented

- **Severity**: LOW
- **Dimension**: 5
- **Labels**: low,renderer,memory,bug

## Description
The health counter is read and rewritten through `mapped_slice_mut` with neither invalidate nor the flush `mapped_slice_mut`'s own doc mandates; `GpuBuffer` has no invalidate primitive at all. Benign **only** because gpu-allocator 0.27 puts `HOST_COHERENT` in the *required* flag set for `CpuToGpu` — and nothing in the source says so.

Documentation half of REN-D4-04 / REN-D4-05.

## Location
`crates/renderer/src/vulkan/context/resources.rs` (`collect_image_health`), `crates/renderer/src/vulkan/buffer.rs`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D5-05).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2793
