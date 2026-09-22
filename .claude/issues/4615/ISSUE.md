# PERF-D4-2026-09-21-01: The indirect-draw SSBO stayed eagerly sized at `MAX_INSTANCES` after #4199

**Labels**: bug, renderer, low, memory, performance

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: LOW (VRAM residency; no frame-time cost) · **Dimension**: 4 — SSBO Sizing & Upload
**Location**:
- `crates/renderer/src/vulkan/scene_buffer/constants.rs:220`: `pub const MAX_INDIRECT_DRAWS: usize = MAX_INSTANCES;` (262,144)
- `crates/renderer/src/vulkan/scene_buffer/buffers.rs:545-547`: `indirect_buf_size`, allocated for each frame in flight at init with no grow path
- `docs/engine/memory-budget.md:96`: the ledger row (10.5 MB), which does not say whether the size is deliberate

**Status**: NEW. #4199 moved the instance and previous-model SSBOs onto `INITIAL_INSTANCE_CAPACITY` (65,536) plus a grow path (`ensure_instance_capacity`). The indirect buffer was not in #4199's scope and stayed eager, so this is not a regression of it.
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

- The buffer is 262,144 × 20 B × 2 FIF = 10.5 MB, resident from init. It holds one `VkDrawIndexedIndirectCommand` per post-merge batch, so its fill is bounded by the raster batch count.
- The runtime baselines (`.claude/audit-baselines/runtime/*.tsv`) peak at 196 batches (`bench_draws_batches`, FO4 InstituteBioScience) and at most 283 raster commands (FNV FreesideAtomicWrangler). More than 99.9 % of the buffer is never written.
- Publish-time note: the denser stepped-camera bench-of-record (`docs/audits/BENCH_stepped-camera_4c9a5b36.tsv`) peaks at 1,281 batches (MedTek, `13545/1281b/29c`). That is still under 0.5 % of capacity.
- Upload is already O(live) and content-hash-gated (`upload_indirect_draws`, #1809). The waste is residency only.

## Impact

About 10.5 MB of VRAM is held for a capacity no measured workload approaches. Starting at `INITIAL_INSTANCE_CAPACITY` entries would hold 2.6 MB (65,536 × 20 B × 2).

## Related

- #4199 (closed): the same eager-at-`MAX_INSTANCES` residency class, for the two larger scene SSBOs.
- #2751 (closed): the `MAX_INDIRECT_DRAWS` clamp in the draw loop. A grow path must clamp to the *live* capacity instead.
- #309 / #992: the original sizing rationale in the constant's doc.

## Suggested Fix

Put the indirect buffers on `ensure_instance_capacity`'s grow path; since batches ≤ instances, they can grow alongside the instance buffers. Otherwise, record the 10.5 MB as a deliberate cost in `memory-budget.md`.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D4-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: The draw-loop clamp (#2751) and `upload_indirect_draws`' overflow cap use the live capacity after a grow
- [ ] **DROP**: If the indirect buffers become growable, the old buffers are destroyed only after the frames that used them retire, and teardown order is unchanged
- [ ] **TESTS**: The grow path is covered (starting capacity, grow on overflow, descriptor/binding refresh), or the `memory-budget.md` row states the eager size is deliberate

