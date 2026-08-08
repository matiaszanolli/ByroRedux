# #2389 — ECS-D5-01: Two Late-stage parallel systems read resources they never declare — same shape as the closed #1787

- **Severity**: MEDIUM
- **Domain**: ecs
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2389


- **Severity**: MEDIUM
- **Dimension**: 5b — Scheduler Access Declarations (M27)
- **Location**: `byroredux/src/boot.rs:1071-1078` (`log_stats_system`), `byroredux/src/boot.rs:1084-1094` (`metrics_sample_system`); bodies at `byroredux/src/systems/debug.rs:125-200` and `byroredux/src/systems/metrics.rs:66-204`
- **Status**: NEW (regression of the *class* fixed by #1787 / CONC-D4-01, which is closed; this is a different, previously-unexamined pair of systems)

**Description**

Both systems sit in the `Stage::Late` parallel batch (4 systems, 6 analyzed pairs — the largest parallel batch in the engine) and read resources absent from their `Access` declaration, so the analyzer reports `AccessConflict::None` for pairings it has not actually proved disjoint. This is exactly the failure mode #1787 fixed for `physics_sync_system`; that fix did not examine these two telemetry systems.

`log_stats_system` declares only `TotalTime`/`DeltaTime`/`DebugStats` (reads) but its body also reads `SkinCoverageStats` (`systems/debug.rs:146`) and `CpuFrameTimings` (`systems/debug.rs:153`).

`metrics_sample_system` declares `TotalTime`, `SkinCoverageStats`, `AllocatorResource`, `GpuMemoryBudget` (reads) + `MetricsState`, `MetricsSnapshot` (writes) but its body also reads `CpuFrameTimings` (`systems/metrics.rs:134`) and `SchedulerSystemTimings` (`systems/metrics.rs:170`).

Both reads in `log_stats_system` are behind a `want_breakdown` runtime gate; `BYRO_PROFILE` gates part of the metrics path — the same "runtime gate the analyzer cannot see" shape #1787's own boot.rs comment describes.

**Evidence** (verified directly against `boot.rs:1071-1094` and the two system bodies, re-confirmed at publish time against the same commit `79bfc76e`):

```rust
// boot.rs:1071 — declaration
scheduler.add_to_with_access(
    Stage::Late,
    log_stats_system,
    Access::new()
        .reads_resource::<TotalTime>()
        .reads_resource::<DeltaTime>()
        .reads_resource::<DebugStats>(),   // ← nothing else
);
// systems/debug.rs:146,153 — body
.then(|| world.try_resource::<SkinCoverageStats>())
.then(|| world.try_resource::<CpuFrameTimings>())
```
```rust
// boot.rs:1084-1094 — declaration has no CpuFrameTimings / SchedulerSystemTimings
// systems/metrics.rs:134
if let Some(cpu) = world.try_resource::<CpuFrameTimings>() { … }
// systems/metrics.rs:170
world.try_resource::<SchedulerSystemTimings>()
```

Non-race verification: the only writers of the three missing types are `byroredux/src/main.rs` (main thread, outside `scheduler.run`) and `Scheduler::run` itself (`scheduler.rs:519-528`, after the whole stage loop). No system in the `Late` parallel batch writes any of them, so `known_conflict_count()` is correctly 0 today.

**Impact**

No live data race and no wrong `sys.accesses` row today. The #1602 boot guard and the `scheduler_access_invariants_hold_on_the_real_schedule` test are blind on these three types: the moment any future `Stage::Late` parallel system declares a write on `CpuFrameTimings`, `SkinCoverageStats`, or `SchedulerSystemTimings`, the analyzer will silently pass a pair that genuinely serialises on an `RwLock` — the exact regression the M27 machinery exists to catch. `SchedulerSystemTimings` is the most likely candidate, since it is already engine-written state that a future in-ECS profiler would naturally move into a system.

**Related**: #1787 / CONC-D4-01 (closed — same class, `physics_sync_system`), #1785 / CONC-D3-02 (closed — same class, `animation_system` color sinks), #1602 / #1601 (the guard these gaps evade), #2138 (the test that would have caught a resulting conflict).

**Suggested Fix**: Append `.reads_resource::<SkinCoverageStats>().reads_resource::<CpuFrameTimings>()` to the `log_stats_system` declaration and `.reads_resource::<CpuFrameTimings>().reads_resource::<SchedulerSystemTimings>()` to `metrics_sample_system`'s, then add a `scheduler_access_tests.rs` source-assertion pin in the style of `physics_sync_declaration_reads_contact_config_and_faller_dump_types` so a future edit that drops them fails the build.

## Completeness Checks
- [ ] **SIBLING**: Check other `Stage::Late` (and other-stage) parallel systems for the same undeclared-resource-read shape
- [ ] **LOCK_ORDER**: Confirm the added declarations don't introduce a genuine `known_conflict_count() > 0` regression once accurate
- [ ] **TESTS**: A source-assertion pin (style of `physics_sync_declaration_reads_contact_config_and_faller_dump_types`) prevents future silent drops

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
