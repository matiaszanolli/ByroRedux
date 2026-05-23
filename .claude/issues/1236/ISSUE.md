# ECS-D5-NEW-01: add_exclusive_with_access missing — closure/function exclusives can't declare access

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1236
**Filed from**: `docs/audits/AUDIT_ECS_2026-05-23_DIM5.md`
**Severity**: MEDIUM
**Labels**: `medium`, `ecs`, `bug`

## Description

The parallel-stage registration API has four flavours:
```
add_to                       add_to_with_access
try_add_to                   try_add_to_with_access
```
Each parallel slot has a closure-friendly entry point that takes an `Access` declaration via `declared_override`. The exclusive-stage surface has only two flavours:
```
add_exclusive                <missing>
try_add_exclusive            <missing>
```

Closures and bare `fn(&World, f32)` items go through the blanket `impl<F: FnMut(&World, f32)> System for F` at `crates/core/src/ecs/system.rs:49`, which inherits the default `fn access(&self) -> Option<Access> { None }` from the trait. So every exclusive system registered with these types — and that's *every* exclusive system in `byroredux/src/main.rs` today — sees `declared_access() == None`, classified as undeclared by `access_report`.

Engine inventory:
```
$ grep -c "add_exclusive\b" byroredux/src/main.rs
16
$ grep -c "add_to_with_access" byroredux/src/main.rs
10
```

The 16 exclusives cover the entire papyrus_demo dispatcher set, `spin_system`, the PostUpdate ordering chain (`footstep_system`, `particle_system`, `billboard_system`, `world_bound_propagation_system`, `submersion_system`), the M27 Phase 3 re-staged `audio_system`, and `event_cleanup_system`. None can declare access today without rewriting them as struct systems.

## Suggested Fix

Mirror `add_to_with_access` / `try_add_to_with_access` against the exclusive vector at `scheduler.rs:227-270`. Two new methods, both with `declared_override: Some(access)`:

```rust
pub fn add_exclusive_with_access<S: System + 'static>(
    &mut self, stage: Stage, system: S, access: Access,
) -> &mut Self { … }

pub fn try_add_exclusive_with_access<S: System + 'static>(
    &mut self, stage: Stage, system: S, access: Access,
) -> Result<&mut Self, String> { … }
```

Pair with a regression test that asserts an `add_exclusive_with_access` registration's row in the report carries `declared = Some(_)`. Then migrate the 16 main.rs exclusives in a follow-up commit.

## Completeness Checks

- [ ] **UNSAFE**: N/A (pure-Rust trait method addition)
- [ ] **SIBLING**: verify `try_add_exclusive_with_access` is added alongside the non-`try` flavour so all four cells of the {parallel,exclusive} × {lax,try} matrix exist
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: regression test in `scheduler.rs::tests` asserting an exclusive registered via `add_exclusive_with_access` shows `declared = Some(_)` in `access_report().stages[…].systems[…]` and that the report's `undeclared_count` decrements accordingly

## Related

- M27 Phase 3 commit `05fe2bac` (the "0 undeclared" claim).
- #1237 (ECS-D5-NEW-02 — paired metric-semantic gap that becomes decideable once this lands).
