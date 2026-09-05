# #3839: PERF-D3-2026-09-05-01: BLAS residency budget is a fixed fraction of the whole DEVICE_LOCAL heap, blind to the ~1.1–2.3 GB resolution-scaled floor the volumetric grid now dominates

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D3-2026-09-05-01) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3839 --json state`.

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D3-2026-09-05-01), published from `/audit-suite volumetrics-deep`. Premise re-verified against HEAD at publish time.

> Note: `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs:694-741`, `crates/renderer/src/vulkan/acceleration/mod.rs:270-320`
- **Status**: NEW
- **Description**: `compute_blas_budget` probes the DEVICE_LOCAL heap that
  will back a BLAS result buffer and returns
  `blas_budget_for_heap(heap) = (heap / 3).max(MIN_BLAS_BUDGET_BYTES)`. It is
  called **once**, from `AccelerationManager::new`; `blas_budget_bytes` is
  never re-derived after that. The `/3` divisor is the entire model of
  "leave room for everything else" — a model written when "everything else"
  meant textures, vertex/index pools and a framebuffer. It now competes with
  a fixed, resolution-scaled floor `docs/engine/memory-budget.md` puts at
  **~1.10 GB at 1080p native and ~2.32 GB at native 4K**, of which the
  froxel grid alone is **~183 MB / ~730 MB** — that page's own words, "still
  the largest resolution-scaled allocation in the engine". On a 12 GB card
  the BLAS budget is ~4 GB; 4 GB (BLAS) + 2.32 GB (fixed floor) + peak
  textures + the vertex-pool cap over-subscribes the heap with no subsystem
  positioned to notice, because each measures against its own private
  ceiling. Worse for this run's motivation: the froxel grid **grows at
  runtime** (a window resize to a larger extent quadruples it), while
  `blas_budget_bytes` stays frozen at its init value — the engine can move
  ~550 MB into the froxel grid mid-session and the BLAS eviction threshold
  will not move a byte.
- **Evidence**:
```rust
// predicates.rs:697-700
pub(super) fn blas_budget_for_heap(heap_bytes: vk::DeviceSize) -> vk::DeviceSize {
    (heap_bytes / 3).max(MIN_BLAS_BUDGET_BYTES)
}
```
  `mod.rs:273` — `let derived_budget_bytes = compute_blas_budget(instance, device, physical_device)?;`
  is the sole call site; `blas_budget_bytes` is thereafter only read
  (`blas_static.rs:1029`, `1074`). Contrast `context/resize.rs:819-866`,
  which reallocates the froxel grid at the new render extent with no
  corresponding budget re-derivation anywhere in `recreate_swapchain`.
- **Impact**: On a 6 GB RT-minimum card at 1080p the arithmetic is 2 GB BLAS
  budget + 1.10 GB fixed floor + textures — the BLAS manager will happily
  fill its 2 GB and let the allocator OOM rather than evict, because from its
  own view it is under budget. Surfaces as an allocation error inside
  `build_blas_batched` (degrading to a missing BLAS, which #1793 already
  documents as having no recovery path) or a driver-side host-memory
  fallback plus a frame-time cliff. Not observable on the 12 GB dev GPU,
  which is why it has stayed invisible.
- **Related**: `docs/engine/memory-budget.md` "Volumetrics (M55)" +
  "VRAM Rough Budget"; #3117 (grid growth that never reached the ledger);
  #387 (the original `VRAM/3` rationale); #3043 (the heap-probe correction —
  fixed *which* heap is measured, not *what else* claims it).
- **Suggested Fix**: Subtract a computed fixed-floor reservation from
  `heap_bytes` before the `/3` — the resolution-scaled passes already know
  their own sizes (`FROXEL_BYTES_PER_SLOT`, `SVGF_BYTES_PER_PIXEL`,
  `CAUSTIC_BYTES_PER_PIXEL`, `RESERVOIR_STRIDE`) — and re-derive
  `blas_budget_bytes` at the end of `recreate_swapchain` so a resolution
  change moves the eviction threshold with it.
- **Confidence**: High on the code shape and arithmetic; the failure mode is
  inferred from the RT-minimum-hardware budget math, not reproduced on a
  6 GB card (not available in this environment) — flagged per the
  Speculative-Vulkan-caveat posture as a real but unreproduced-here risk.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
