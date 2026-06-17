# D1-NEW-01: Scheduler run() allocates ~25 Strings + a Vec every frame for system-timing names

**Issue**: #1647
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-06-16.md`
**Severity**: LOW · **Dimension**: CPU Hot Paths
**Labels**: low, performance, ecs, enhancement

## Location
- `crates/core/src/ecs/scheduler.rs:438` (collection), `:448,461,473` (`to_string()` pushes), `:483-485` (acknowledging doc comment)
- Called per frame from `byroredux/src/main.rs` (~2212)

## Description
The scheduler `run()` allocates a fresh `Mutex<Vec<(String, u64)>>` and pushes `entry.system.name().to_string()` per system (~25 heap `String`s/frame, ~1500/s @ 60 fps) unconditionally — even when the `SchedulerSystemTimings` resource is absent, in which case the Vec is built and discarded. `System::name()` returns a `type_name`-derived `&'static str`, so the `String` allocations are gratuitous.

## Impact
Small, non-compounding heap churn. Not a leak. No `dhat` guard exists for this site.

## Suggested Fix
Store `&'static str` instead of `String`, or gate the entire collection on `SchedulerSystemTimings` presence and reuse a persistent Vec via `clear()+extend`.

## Related
- Session-46 CPU hot-path work (#1371–#1379) did not touch the scheduler timing path.
