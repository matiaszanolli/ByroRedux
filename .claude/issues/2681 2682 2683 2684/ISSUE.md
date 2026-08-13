# Issues 2681, 2682, 2683, 2684

All four filed from perf/safety audits (`docs/audits/AUDIT_PERFORMANCE_2026-08-12.md` and
`docs/audits/AUDIT_SAFETY_2026-08-12.md`). Domain: **renderer** (`byroredux-renderer` /
`byroredux` binary).

## #2681 — PERF-D2-04: sort_draw_commands re-extracts the 11-tuple key from a ~480-byte DrawCommand on every comparison
- **Severity**: LOW
- **Location**: `byroredux/src/render/mod.rs` — `sort_draw_commands` / `draw_sort_key`
- **Bug**: `sort_unstable_by_key`/`par_sort_unstable_by_key` re-evaluate `draw_sort_key` on each comparison (~2·N·log₂N extractions), each touching a ~480-byte `DrawCommand`. Sibling `collect_lights` was already converted to decorate-sort-undecorate for the same reason (#2034).
- **Caveat (explicit in issue)**: "No quantitative guard exists for this site; do not land it on reasoning alone." Suggested fix: prototype decorate-sort-undecorate behind the existing `manual_bench_draw_sort_serial_vs_parallel` bench harness (sweeps N=400…10K), extend with a third arm, ship ONLY if the win survives at N=1800–3400 (actual runtime baseline range), and re-derive `DRAW_SORT_PARALLEL_THRESHOLD` together.
- **Scope decision needed**: this requires a bench harness run + measurement, not a blind land — check what's actually feasible in this environment.

## #2682 — PERF-D2-05: sort_draw_commands' in-place partition self-swaps ~480-byte DrawCommands
- **Severity**: LOW (downgraded from MEDIUM by the auditor's own disproof)
- **Location**: `byroredux/src/render/mod.rs` — `sort_draw_commands` (raster/RT-only partition loop)
- **Bug**: `draw_commands.swap(raster_len, index)` called unconditionally; `<[T]>::swap` does a full 3-way copy even when `raster_len == index` (self-swap). Worst case O(N) wasted ~480B copies when nothing is culled (e.g. `BYRO_NO_CULL=1`).
- **Fix** (one-liner, given in the issue): `if raster_len != index { draw_commands.swap(raster_len, index); }`.

## #2683 — SAFE-D4-01: GpuBuffer flush SAFETY comments assert false facts
- **Severity**: MEDIUM
- **Location**: `crates/renderer/src/vulkan/buffer.rs` — `GpuBuffer::flush_if_needed` (SAFETY ~768-771, block ~772), `GpuBuffer::write_mapped`'s flush (SAFETY ~826-832, block ~833), `GpuBuffer::flush_range` (SAFETY ~870-872, block ~873); helper `aligned_flush_range` (~506-516)
- **Bug**: Three `vkFlushMappedMemoryRanges` call sites' SAFETY comments claim the flushed range is "contained within this allocation's slice" and that "gpu-allocator already pads sub-allocations to nonCoherentAtomSize" — both false. `aligned_flush_range` rounds the offset DOWN and size UP, producing a strict superset of the allocation (widens outward), not a subset. gpu-allocator 0.28 has zero `nonCoherentAtomSize`/`non_coherent` awareness (verified in vendored source) — it only aligns to `VkMemoryRequirements.alignment`. `write_mapped`'s comment additionally claims a cap that `aligned_flush_range` never applies.
- **Impact**: Not unsound today (every call site uses `AllocationScheme::GpuAllocatorManaged`, so widened range stays inside the parent multi-MB block) — but the comment is actively misleading for a future refactor (e.g. switching to `DedicatedBuffer` or exceeding gpu-allocator's 64MB host-visible block) that could put `offset+size` past the parent allocation with no guard.
- **Suggested Fix**: Rewrite the three SAFETY comments to state the true invariant ("range is widened outward past the sub-allocation, bounded only by the parent gpu-allocator block, which is a multiple of NON_COHERENT_ATOM_SIZE"); add a `debug_assert!` that `aligned_offset + aligned_size <= <parent block size>` (or clamp) so a future dedicated-allocation regression fails loudly.

## #2684 — SAFE-D4-03: six unsafe fn carry no # Safety doc section
- **Severity**: MEDIUM
- **Location**:
  - `crates/renderer/src/vulkan/frame_upscaler.rs` — `record_native_blit` (~592), `record_fsr_barriers_before` (~705), `record_fsr_depth_restore` (~764), `record_fsr_barriers_after` (~822)
  - `crates/renderer/src/vulkan/gbuffer.rs` — `GBufferAttachment::destroy` (~180)
  - `crates/renderer/src/vulkan/context/screenshot.rs` — `screenshot_record_copy` (~101)
- **Bug**: These 6 of 77 workspace `unsafe fn` lack a `# Safety` doc section stating the caller contract, unlike the other 71. All private/`pub(super)` (crate-internal blast radius), but the 4 `frame_upscaler` ones are the FSR3 boundary barriers whose contract (cmd in recording state, images in specific layout) is discussed at length in prose but never written as a caller obligation. `GBufferAttachment::destroy`'s inner blocks reference "caller of `destroy` (an `unsafe fn`) guarantees…" forwarding to a contract that doesn't exist at the signature.
- **Fix**: Add `# Safety` section to each of the six stating the caller obligation. Consider `#![warn(clippy::missing_safety_doc)]` on `crates/renderer` (private fns aren't caught by clippy's default lint) — evaluate feasibility/noise before adding.

## Domain
renderer → `byroredux-renderer` (#2683, #2684) / `byroredux` binary (#2681, #2682)
