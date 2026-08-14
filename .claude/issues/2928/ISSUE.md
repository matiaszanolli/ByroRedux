# PERF-D3-04: compute_blas_budget sums every DEVICE_LOCAL heap while its docs say the smallest one

- **Issue**: [#2928](https://github.com/matiaszanolli/ByroRedux/issues/2928)
- **Finding ID**: `PERF-D3-04`
- **Labels**: `low,performance,memory,bug`
- **Source report**: [`docs/audits/AUDIT_PERFORMANCE_2026-08-14.md`](../../../docs/audits/AUDIT_PERFORMANCE_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2928 --json state`.

---

- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs` (`compute_blas_budget`), `crates/renderer/src/vulkan/device.rs` (`total_device_local_bytes`, `smallest_device_local_heap_bytes`), `crates/renderer/src/vulkan/acceleration/mod.rs` (the `blas_budget_bytes` field doc), `docs/engine/memory-budget.md` (§ Reserve floors, `MIN_BLAS_BUDGET_BYTES` row)
- **Status**: NEW
- **Description**: `compute_blas_budget` calls `total_device_local_bytes`, which **sums** the sizes of every heap carrying `MemoryHeapFlags::DEVICE_LOCAL`, then divides by 3. Three separate pieces of prose describe it as a single heap: `compute_blas_budget`'s own doc says "`VRAM / 3`"; the `blas_budget_bytes` field doc says "Derived at construction time from DEVICE_LOCAL heap size (VRAM / 3)"; `memory-budget.md`'s `MIN_BLAS_BUDGET_BYTES` row says "device_local_heap / 3, capped below". The codebase already owns the correct query for a residency ceiling — `smallest_device_local_heap_bytes`, whose own doc states the rationale outright: *"this is the tighter of the two — running an allocator to that heap's limit fails first"* — and the allocator's 80%-of-heap pressure warning (`allocator.rs`) uses it. The BLAS budget, whose entire stated purpose is "so smaller-VRAM GPUs evict before hitting an out-of-memory condition" (#387), uses the looser sum instead. The two subsystems therefore disagree about how much VRAM exists.
- **Evidence**: `total_device_local_bytes` — `.filter(|heap| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL)).map(|heap| heap.size).sum()`. `smallest_device_local_heap_bytes` — identical filter, `.min()`. `compute_blas_budget` — `(device_local_bytes / 3).max(MIN_BLAS_BUDGET_BYTES)` over the `sum` variant. `grep -rn smallest_device_local_heap_bytes` returns only `allocator.rs` call sites; the acceleration module never references it.
- **Impact**: On any device exposing more than one DEVICE_LOCAL heap — the common AMD / hybrid layouts where a small `DEVICE_LOCAL | HOST_VISIBLE` BAR window is reported alongside the main VRAM heap, and the two are not disjoint physical memory — the budget over-estimates available VRAM and the eviction line sits above where an allocation actually starts failing. Single-heap NVIDIA desktop parts (including the RTX 4070 Ti dev card) are unaffected, so this is invisible on the target hardware and cannot be observed here. Practical magnitude on real multi-heap parts is small (a 256 MB over-count moves the budget by ~85 MB), which is why this is LOW and not a correctness finding — the value of fixing it is that the two VRAM-ceiling policies stop disagreeing, in a subsystem where a previously-wrong budget figure has already burned an audit (#387, "Roadmap claims 1 GB BLAS budget but code is 4 GB").
- **Which side is wrong**: the **code**, not the docs. All three prose sites describe the safer, intended semantics ("the DEVICE_LOCAL heap", singular); the implementation is the outlier. Changing the docs to say "sum of all DEVICE_LOCAL heaps" would document a weaker guarantee than #387 asked for.
- **Related**: #387 (FNV-D4-01 — established the dynamic budget and its OOM-avoidance purpose), #1572 (REN-D5-DOC-01 — the sibling case where `memory-budget.md` and the allocator warning were reconciled onto `smallest_device_local_heap_bytes`, which is the precedent this path did not follow).
- **Suggested Fix**: Switch `compute_blas_budget` to `smallest_device_local_heap_bytes` and keep the `MIN_BLAS_BUDGET_BYTES` floor (which already protects the degenerate zero/tiny-heap case), then leave all three doc sites as they are — they become accurate. A one-line unit test pinning "budget derives from the smallest heap, not the sum" would keep it from drifting back.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_PERFORMANCE_2026-08-14.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
