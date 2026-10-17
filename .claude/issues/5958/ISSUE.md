# SAFE-VOL-2026-10-17-02: `fog_cluster_dirty_range` is committed after the fallible uploads it describes

Labels: low,safety,renderer,vulkan,bug
Source: docs/audits/AUDIT_SAFETY_2026-10-17.md

- **Severity**: LOW (latent). The trigger is a driver-reported failure in `vkFlushMappedMemoryRanges` (non-coherent memory only) or a missing/unmapped allocation, which no normal run produces. No out-of-bounds access results (all stale offsets/counts were produced by an earlier valid build, so the shader's reads stay inside the index buffer); the effect is wrong local-fog references.
- **Dimension**: GPU-lifetime / state commit (Dims 3 and 5 adjacent). Same class as #5772 and the concurrency sibling's latch (state advanced before the GPU work it vouches for is confirmed).
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:886-902`.
- **Description**: The frame-slot's GPU-side `entries` array only holds non-zero counts inside `fog_cluster_dirty_range[frame]`. Each frame rewrites `union(current range, previous dirty range)`. The new range is stored at `:901`, after `write_mapped_at(...)?` (`:891`) and the index `write_mapped(...)?`. If the entries write copies bytes into the mapped slice and then the flush (or the following index write) returns `Err`, `dispatch` returns early, the frame is treated as skipped, and the range is not updated. The slot's mapped memory now holds the failed frame's counts for `(lo_f, hi_f)`, a range that appears in neither the stale dirty range nor, necessarily, the next frame's range. Those clusters keep non-zero `count` until some later frame touches them, and the shader then dereferences index slots that were never written for them.
- **Disproof attempted**: On HOST_COHERENT memory `flush_range` returns `Ok` before any fallible call, and the only other `Err` sources (`Buffer has no allocation` / `Buffer not mapped`) are permanent and fail before the copy. So the stale-range window needs a non-coherent memory type and a flush failure. It is real but not reachable without fault injection.
- **Suggested fix**: Store `fog_cluster_dirty_range[frame] = union(old, new)` (or `(0, FOG_VOLUME_CLUSTER_COUNT)`) before the first fallible write and narrow it only after both writes succeed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other init/teardown sites)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_SAFETY_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
