# PERF-D3-02: BLAS compaction's alloc_compact leaks compacted acceleration structures on both early exits

- **Issue**: [#2926](https://github.com/matiaszanolli/ByroRedux/issues/2926)
- **Finding ID**: `PERF-D3-02`
- **Labels**: `medium,performance,memory,vulkan,bug`
- **Source report**: [`docs/audits/AUDIT_PERFORMANCE_2026-08-14.md`](../../../docs/audits/AUDIT_PERFORMANCE_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2926 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs` — the `alloc_compact` closure inside `build_blas_batched` (Phases 5+6)
- **Status**: NEW (residual gap left by CLOSED #316; the closure that #316 added is present and still runs — this is not a regression of it)
- **Description**: `alloc_compact` builds a local `Vec<CompactedBlas>` (`compact_accels`), pushing one `(mesh_handle, vk::AccelerationStructureKHR, GpuBuffer, …)` tuple per mesh. It has two failure exits, and neither destroys the tuples already pushed:
  1. `let compact_buffer = GpuBuffer::create_device_local_uninit(…)?;` — the `?` unwinds the closure with `compact_accels` holding `i` entries.
  2. The `create_acceleration_structure` `Err(e)` arm destroys only *this* iteration's `compact_buffer` and then `anyhow::bail!`s, again with `i` entries already pushed.
  In both cases `compact_accels` is simply dropped. `GpuBuffer` has a `Drop` safety net (#656) that reclaims the backing buffer, but `vk::AccelerationStructureKHR` is a raw handle with **no `Drop` impl at all** — the same reasoning #2481 / AS-D1-NEW-02 spells out for `BlasEntry` in `build_blas`. Every one of those `i` acceleration structures leaks for the process lifetime. The outer error handler (which does destroy `prepared` and the `query_pool`) cannot help: `compact_accels` never escapes the closure on the error path.
- **Evidence**: the in-loop comment on exit (2) states *"Buffer was created in this iteration but not yet pushed into `compact_accels`, so the outer cleanup loop won't see it — destroy it locally before bubbling so the OOM path is leak-free."* There is no outer cleanup loop over `compact_accels`; the closure's return type is `Result<(Vec<CompactedBlas>, u64, u64)>` and the `Err` arm at its call site only iterates `prepared`. The comment describes a cleanup that does not exist, which is why the gap survived #316. No test in `crates/renderer/src/vulkan/acceleration/tests.rs` touches the compaction rollback (grep for `compact` there returns only flag-drift and instance-map tests).
- **Impact**: A leak on the exact error path that memory pressure produces — an allocator failure during compaction leaves the pool *more* exhausted than before, so a retry on the next cell load fails earlier and leaks again. Positive feedback under sustained pressure. Blast radius is bounded by batch size (one leaked AS per already-compacted mesh in the failing batch), and `total_blas_bytes` / `static_blas_bytes` never see these bytes, so the leak is invisible to `blas_budget_bytes`, to the eviction predicate, and to `ctx.scratch` / `tex.stats` telemetry. Secondary consequence on the same path: the `GpuBuffer::Drop` safety net carries `debug_assert!(false, "GpuBuffer leaked into Drop: call destroy() first")`, so a debug build panics once per stranded buffer while unwinding an OOM it was meant to recover from.
  **Reachability**: `create_device_local_uninit` only fails on allocator OOM. Unreachable on the 12 GB dev card (BLAS budget ~4 GB vs. a ~300 MB typical cell); this is a 6 GB-RT-minimum-target defect, which is precisely the population `compute_blas_budget`'s floor exists to serve.
- **Related**: #316 (D2-02 — the closure-based rollback this is the residual of), #2481 / AS-D1-NEW-02 (the "a raw `vk::AccelerationStructureKHR` has no `Drop`" precedent in the same file), #1097 / REN-D8-003 (the equivalent, and complete, Phase-1 rollback over `prepared`).
- **Suggested Fix**: Hoist `compact_accels` out of the closure (or return it in the `Err` payload) so the existing outer rollback can walk it, destroying each `compact_accel` via `accel_loader.destroy_acceleration_structure` and each buffer via `GpuBuffer::destroy` — mirroring the `copy_result` failure arm below it, which already does exactly this correctly. Add a source-level test in the file's `tests.rs` pinning that both early exits are preceded by a `compact_accels` cleanup, as `#1812`/`#2494` do for their own ordering invariants.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_PERFORMANCE_2026-08-14.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
