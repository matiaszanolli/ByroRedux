# #1911: REN-D1-01 — memory-budget.md misattributes shrink_tlas_to_fit to cell-unload

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `docs/engine/memory-budget.md` (Acceleration Structures → Scratch buffers) vs
`crates/renderer/src/vulkan/context/draw.rs:3976-3997` + `byroredux/src/cell_loader/unload.rs:141`

## Description
The doc stated `shrink_blas_scratch_to_fit` and `shrink_tlas_to_fit` both run
at cell-unload. Only the first is true. `shrink_tlas_to_fit` and
`shrink_tlas_scratch_to_fit` run at the end of every `draw_frame`, targeting
the just-incremented FIF slot whose fence was waited at frame start — a
different site with a stricter safety precondition. The LRU section also
omitted the per-frame post-TLAS `evict_unused_blas` call and the single-shot
`build_blas` guard.

## Suggested Fix
Reword: BLAS scratch shrink → cell-unload + resize; TLAS instance/scratch
shrink → end-of-frame in `draw_frame` (post fence-wait, slot-rotated). Add
the two missing `evict_unused_blas` call sites to the LRU section.

---

# #1912: REN-D1-02 — build_tlas resize comment sizes TLAS instances at 88 B; actual is 64 B

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:364-378`

## Description
The pre-size rationale comment claimed "8192 slots × 88 B per instance × 2
FIF = ~1.4 MB" and "~660 KB BAR" per slot.
`size_of::<vk::AccelerationStructureInstanceKHR>()` is 64 B, so the real
numbers are 512 KB per slot / ~1.0 MB across both — matching the sibling
docs (`constants.rs`, `memory.rs`), which are correct. Only the prose in
`tlas.rs` was wrong; the code allocates correctly (uses `size_of` directly).

## Suggested Fix
Replace 88 B → 64 B and rederive (~512 KB/slot, ~1.0 MB across 2 FIF).
