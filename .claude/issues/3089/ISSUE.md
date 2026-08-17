# CONC-2026-08-16-01: streaming pre-parse worker and ECS scheduler contend for rayon's global pool

**Issue**: #3089
**Severity**: MEDIUM
**Labels**: `medium,sync,performance,bug`
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_CONCURRENCY_2026-08-16.md` (Dimension — Worker Threads).

**Location**: `byroredux/src/streaming.rs`:1233-1239 (worker side) · `crates/core/src/ecs/scheduler.rs`:499-504 (scheduler side)

**Trigger conditions**: A cell whose fresh-parse count reaches `PRE_PARSE_RAYON_MIN = 8` — session start, first entry into a new worldspace region, or any door transition into un-cached content. Steady-state streaming (0–6 fresh NIFs per cell, per the in-code Riverwood measurement) takes the serial fast path and does not trigger this.

## Description

`cell_pre_parse_worker` runs on its own dedicated `std::thread` (`streaming.rs`:738) — the whole point of the M40 design is that cell parsing must not sit on the frame's critical path.

But its CPU-bound Phase 2 fans out with `extracted.into_par_iter().map(parse_one_nif).collect()`, which goes to rayon's **global** pool. `Scheduler::run` dispatches each stage's parallel batch with `data.parallel.par_iter_mut().for_each(…)` into the **same** global pool.

## Evidence

```
$ grep -rn "ThreadPoolBuilder\|num_threads" --include="*.rs" crates/ byroredux/ | wc -l
0
```

Re-verified 2026-08-17: **no custom rayon pool is constructed anywhere in the workspace**, so both the streaming worker and the ECS scheduler share the single global pool and its default thread count.

## Impact

The dedicated worker thread does not buy the isolation the M40 design intends. When a cell crosses the 8-NIF threshold, the worker's parallel parse competes for the same rayon workers the frame's parallel stages need — so the frame stalls behind background parsing at exactly the moments (session start, region entry, door transition) when frame pacing matters most.

On the dev CPU (Ryzen 7950X, 16c/32t) there is enough headroom to mask it much of the time, which is why it has not shown up as an obvious hitch.

## Suggested Fix

Give the pre-parse worker its **own** `rayon::ThreadPool` (via `ThreadPoolBuilder`) sized to leave headroom for the scheduler, and run Phase 2 inside `pool.install(…)`. That preserves the parallel parse while restoring the isolation the dedicated thread was meant to provide.

Measure before and after on a door transition into un-cached content — that is the trigger case.

## Related

- M40 cell lifecycle (`byroredux/src/streaming.rs`)
- #3005, #3006 (the telemetry regressions that would be affected by frame-pacing changes)

## Completeness Checks
- [ ] **LOCK_ORDER**: A dedicated pool does not change ECS resource-acquisition ordering
- [ ] **SIZED**: The worker pool leaves headroom rather than doubling total thread count
- [ ] **MEASURED**: Benchmarked on the trigger case (≥8 fresh NIFs), not steady state
- [ ] **SIBLING**: Any other `par_iter` on a non-frame thread checked for the same contention
- [ ] **TESTS**: A regression test asserts the worker uses its own pool

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3089 --json state` when live state is needed.*
