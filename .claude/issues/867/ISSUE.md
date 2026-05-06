## Source Audit
`docs/audits/AUDIT_ECS_2026-05-06.md` — Dimension 5 (System & Scheduler)

## Severity / Dimension
LOW / 5 (System & Scheduler)

## Location
`crates/core/src/ecs/scheduler.rs:272-293`

## Description
`Scheduler::run` does not document what happens when an individual system panics.

With the default `parallel-scheduler` feature on, rayon's `par_iter_mut().for_each(...)` (`scheduler.rs:278-281`) propagates the first worker panic to the calling thread *after* all sibling tasks in the batch finish (rayon does not cancel sibling work on panic — see rayon-core panic policy).

With `parallel-scheduler` off (`scheduler.rs:283-287`), the plain `for` loop short-circuits on the first panic, leaving any remaining parallel-phase systems and *all* exclusive-phase systems in the same stage un-run for that frame.

There is no per-system `catch_unwind`, no panic-isolation, and no doc comment on `Scheduler::run` explaining either contract.

## Evidence
```rust
// scheduler.rs:273-293
pub fn run(&mut self, world: &World, dt: f32) {
    for (_stage, data) in &mut self.stages {
        #[cfg(feature = "parallel-scheduler")]
        { data.parallel.par_iter_mut()
               .for_each(|entry| entry.system.run(world, dt)); }
        #[cfg(not(feature = "parallel-scheduler"))]
        { for entry in &mut data.parallel { entry.system.run(world, dt); } }
        for entry in &mut data.exclusive { entry.system.run(world, dt); }
    }
}
```

Compare with `byroredux/src/streaming.rs:260,364` and `crates/core/src/ecs/world.rs:1780`, which use `std::panic::catch_unwind(AssertUnwindSafe(...))` for isolation in failure-tolerant call sites.

## Impact
Behavioural — none today (no engine system panics in steady state).

Diagnostic — when a panic *does* happen (e.g. an invariant violation in a third-party plugin system), the operator has to know the rayon contract to predict whether sibling systems ran or not. This is an operability gap, not a correctness bug.

## Suggested Fix
Add a doc comment to `Scheduler::run` describing:
1. Feature-gated panic propagation order (parallel-scheduler ON: rayon collects; OFF: short-circuits)
2. That exclusives in the same stage are skipped on a panicking parallel phase under `cfg(not(parallel-scheduler))`
3. That the engine contract is "panics escape — there is no recovery"

No code change needed unless we want a `panic_policy: enum { Propagate, Isolate }` field on `Scheduler`, which is a separate design call.

## Related
Issue #848 (footstep stale-read; same dimension). No prior audit has flagged the panic contract.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
