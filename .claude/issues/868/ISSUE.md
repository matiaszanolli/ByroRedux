## Source Audit
`docs/audits/AUDIT_ECS_2026-05-06.md` — Dimension 5 (System & Scheduler)

## Severity / Dimension
LOW / 5 (System & Scheduler)

## Location
`crates/core/src/ecs/scheduler.rs:273` + `byroredux/src/main.rs:774,1111`

## Description
`Scheduler::run` takes `&mut self`. Systems take `&World` (interior-mutable via RwLock), so a system body cannot reach the `Scheduler` instance through any safe path the engine exposes — `Scheduler` is owned by the `App` struct (`byroredux/src/main.rs:240,388`) and is **not** stored as a `Resource`. So reentry is structurally impossible.

However, the contract is implicit. If a future maintainer adds `Scheduler` as a resource (or wraps it in `Arc<Mutex<_>>` to enable hot-reload), `system → scheduler.run` would compile — and panic at `BorrowMutError` with a confusing call site, since the outer `&mut self` is held across the inner call.

## Evidence
- `scheduler.rs:273` — `pub fn run(&mut self, world: &World, dt: f32)`
- No `running: bool` flag, no `Cell<bool>` re-entry guard, no documentation that says "this method must not be called from within a System"
- Engine call sites: `byroredux/src/main.rs:774,1111` (window-resize and per-frame draw)
- No system calls `scheduler.run` (verified by grep)

## Impact
None today — design is sound. Latent risk if the invariant is silently broken by a future refactor (e.g. M27 dynamic scheduling, hot-reload).

## Suggested Fix
Two options:

**(a)** Add a one-line doc comment on `Scheduler::run`:
> Must not be called recursively. Systems cannot reach the `Scheduler` because it is owned by `App` and not exposed via `World`. If you need ad-hoc dispatch, use a `CommandQueue` resource and drain in a Late-stage exclusive system (the `DebugDrainSystem` pattern).

**(b)** Add a `Cell<bool>` re-entry guard with a hard-fail at the top of `run`.

(a) is sufficient and matches the rest of the codebase's hygiene level.

## Related
Aligns with the "no Scheduler-as-Resource" architectural invariant implied by the system-instructions contract.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
