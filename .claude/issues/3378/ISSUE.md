# CONC-D7-2026-08-27-03: `build_stream_parse_pool`'s "reserving half the cores" rationale is false — rayon's global pool is never resized

- **Issue**: [#3378](https://github.com/matiaszanolli/ByroRedux/issues/3378)
- **Finding ID**: `CONC-D7-2026-08-27-03`
- **Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `low,concurrency,doc-rot,documentation`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3378 --json state`.

---

- **Severity**: LOW
- **Dimension**: Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds — worker thread inventory / stated invariant
- **Location**: `byroredux/src/streaming.rs:1008-1029`
- **Status**: NEW
- **Trigger Conditions**: None — this is a documentation/design-claim defect, not a runtime fault. Its practical consequence shows up whenever a fresh-parse burst (`>= PRE_PARSE_RAYON_MIN` uncached NIFs in one cell) overlaps a `Stage::Update` parallel batch.
- **Verification Path**: Static. `grep -rn "build_global\|ThreadPoolBuilder" crates byroredux` returns exactly one production hit — `streaming.rs:1022` — so nothing ever calls `rayon::ThreadPoolBuilder::build_global()`. rayon-core 1.13's default global registry is therefore built with `num_threads == 0`, i.e. `available_parallelism()`.
- **Description**: The `#3089` fix correctly gave the cell-stream worker a private rayon pool so its Phase-2 fan-out cannot occupy the global pool's workers. The accompanying rationale over-claims what that buys:

  > *"reserving half here means a large fresh-parse burst can never claim more workers than the frame's parallel stages have left to run on."*

  Nothing is reserved *from* the global pool. Building a second `ThreadPool` creates an independent registry; the global pool keeps all `N` threads. During a burst the process therefore has `N` global-pool workers **plus** `N/2` `byro-stream-parse-*` workers **plus** the cell-stream worker, main, listener and audio threads runnable at once — 1.5×N rayon threads on N hardware threads, arbitrated by the OS scheduler rather than by any partition. On the dev 7950X (`available_parallelism` = 32) that is 32 + 16 = 48 rayon workers.
- **Evidence**: `byroredux/src/streaming.rs:1017-1029` (the builder — `num_threads((total / 2).max(1))`, no `build_global`), and the absence of any other `ThreadPoolBuilder` / `build_global` call site in the workspace.
- **Impact**: No correctness impact. The isolation benefit `#3089` actually delivers is real and worth keeping (a burst can no longer starve `par_iter_mut` of global-pool workers). The risk is the stale premise: a future reader sizing the pool, or auditing a frame-time regression during streaming bursts, will reason from a core partition that does not exist. This is the same class as `#3091` (a streaming doc comment describing the wrong function) and is why the project treats stated invariants as auditable.
- **Related**: `#3089` (CLOSED — the pool itself), `#3211` (CLOSED — the guards that pin the pool constructor and rayon's `install`, but not this claim), `#3091` (CLOSED — the sibling doc-accuracy fix in the same function's neighbourhood).
- **Suggested Fix**: Reword the comment to state what is true — the pool *isolates* stream parsing from the frame's global-pool batch and is deliberately sized to `N/2` to limit oversubscription — and drop the "can never claim more workers than the frame has left" sentence. If a real cap is wanted, `rayon::ThreadPoolBuilder::new().num_threads(N/2).build_global()` at boot would actually partition, at the cost of halving the ECS scheduler's parallelism.

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_CONCURRENCY_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `CONC-D7-2026-08-27-03`._
