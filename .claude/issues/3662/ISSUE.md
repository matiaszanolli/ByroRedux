# #3662 — PERF-D3-2026-08-30-02: the 80 % DEVICE_LOCAL "approaching OOM" warning has exactly one caller — at engine init, before any cell loads — so it can never fire under the pressure it exists to detect

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D3-2026-08-30-02`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,renderer,memory,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3662

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/allocator.rs:289-334`
  (`warn_threshold_bytes`, `log_memory_usage`),
  `crates/renderer/src/vulkan/context/resources.rs:430-433` (wrapper),
  `byroredux/src/app_events.rs:204` (sole call site),
  `docs/engine/memory-budget.md:555-560`
- **Status**: NEW
- **Description**: `memory-budget.md` closes the VRAM ledger with "A warning fires
  when total allocated bytes exceed 80% of the smallest DEVICE_LOCAL heap
  (`(heap / 5) * 4`, with a 2 GB fallback when no DEVICE_LOCAL heap is reported)."
  The formula is exactly right and matches
  `warn_threshold_bytes` verbatim. The *firing* is not: a workspace-wide grep
  finds `log_memory_usage` reachable from precisely one place —
  `App::resumed`, immediately before `log::info!("Engine ready — entering game loop")`.
  `step_streaming` and `step_debug_loads` run from `about_to_wait`
  (`app_events.rs:706, 715`), i.e. strictly after that sample, and nothing
  re-takes it: not per frame, not per cell load, not per cell unload, and not
  from any console command (`ctx.memory` / `mem.frag` read
  `generate_report` directly and never consult the threshold;
  `commands/world_info.rs:761-780`). The debug-UI metrics sampler
  (`byroredux/src/systems/metrics.rs:110-130`) does sample VRAM every tick, but
  it compares nothing, logs nothing, and uses `GpuMemoryBudget::total_vram_bytes`
  (the **sum** of DEVICE_LOCAL heaps) rather than `smallest_heap_bytes`, which is
  the tighter cap the guard was written against.
- **Evidence**:
  ```
  $ grep -rn "log_memory_usage" --include="*.rs" crates byroredux
  crates/renderer/src/vulkan/allocator.rs:304:pub fn log_memory_usage(          # definition
  crates/renderer/src/vulkan/context/resources.rs:432:    …allocator::log_memory_usage(  # wrapper
  byroredux/src/app_events.rs:204:  self.renderer.as_ref().unwrap().log_memory_usage();  # only caller
  ```
  ```rust
  // byroredux/src/app_events.rs:203-205
  self.scheduler.run(&self.world, 0.0);
  self.renderer.as_ref().unwrap().log_memory_usage();
  log::info!("Engine ready — entering game loop");
  ```
- **Impact**: The engine has no live VRAM-pressure signal. Every mechanism this
  dimension audits — BLAS LRU eviction, TLAS/scratch shrink, staging-pool trim,
  the texture bindless ceiling — degrades *quietly* by design (evict, fall back to
  the checkerboard handle, keep the oversized buffer), so the 80 % warn was the
  one place a session was told it was approaching the heap. On the 12 GB dev card
  the boot sample is ~0.3 GB against a ~9.6 GB threshold; on the 6 GB RT-minimum
  target the same sample is taken before the content that would breach 4.8 GB
  exists. Defense-in-depth gap, not a crash — hence MEDIUM.
- **Related**: #505 (which introduced the heap-scaled threshold precisely because
  the old 2 GB constant "warned on every large cell load" — that observation
  implies a call frequency the code no longer has). #2030's
  `check_slot_available` 90 % one-shot latch is the pattern that *does* work,
  because it sits on the allocation path.
- **Suggested Fix**: Call `log_memory_usage` from the cell-load / cell-unload
  boundary (`cell_loader::unload_cells`' finalization, or the end of
  `step_streaming`) with a one-shot `Once` latch on the WARN arm so a sustained
  breach logs once rather than per transition — the same shape
  `check_slot_available` already uses. Optionally reword
  `memory-budget.md:555-560` to say *when* it samples.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
