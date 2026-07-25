# 2166: PERF-D1-01: Scheduler per-system timing tracker is always armed, defeating the #1647 gate

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2166
**Labels**: bug, medium, performance

---

## Severity
MEDIUM

## Dimension
CPU Per-Frame Allocations & Hot Paths (Dim 1) — `/audit-performance` 2026-07-25

## Location
`byroredux/src/boot.rs:313`; `crates/core/src/ecs/scheduler.rs:62-84,453-503`

## Description
`Scheduler::run` is documented (per #1647) to allocate its per-system wall-time tracker only when the `SchedulerSystemTimings` resource is present — the stated intent being that the resource exists only when the debug UI is open. But `boot.rs` inserts `SchedulerSystemTimings::default()` unconditionally at world setup, so the "no resource" steady-state path the #1647 comment describes never occurs in the shipping binary. Every one of the 39 registered systems therefore pays, every frame: a `String::from(&'static str)` allocation, an `Instant::now()`, and a global `Mutex` lock/unlock — for a consumer (`byroredux/src/systems/metrics.rs`, the egui Metrics panel) that samples at <=2 Hz.

## Evidence
```rust
// scheduler.rs:468-471
let timings: Option<Mutex<Vec<(String, u64)>>> = world
    .try_resource::<SchedulerSystemTimings>().is_some()
    .then(|| Mutex::new(Vec::new()));
// scheduler.rs:73-80 — runs for EVERY system, EVERY frame
let name = self.system.name().to_string();
let t0 = Instant::now();
self.system.run(world, dt);
timings.lock()....push((name, ns));
// boot.rs:313 — unconditional, no debug-UI gate
world.insert_resource(byroredux_core::ecs::SchedulerSystemTimings::default());
```

## Impact
~2340 `String` allocations/s + ~2400 mutex acquisitions/s + ~360 `Vec` reallocs/s at 60 fps (39 systems). Absolute cost is low single-digit us/frame; the sharper edge is a single shared `Mutex` touched by every rayon worker at every system completion in every stage — a scaling hazard as system count grows, and exactly the churn #1647 was filed to remove.

## Related
#1647 (the gate this defeats — closed); same class as closed #2115/D9-01 (`format!` behind a rate gate).

## Suggested Fix
Gate the `insert_resource` call in `boot.rs` on the debug-UI / `BYRO_PROFILE` path (or insert lazily when the overlay first opens); additionally, store `&'static str` (already available from `SystemEntry::name()`) instead of allocating a `String`, and give `SchedulerSystemTimings` a persistent scratch `Vec` the scheduler clears and refills instead of a fresh `Mutex<Vec<_>>` every frame.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (e.g. assert no `SchedulerSystemTimings` resource exists when the debug UI never opens)
