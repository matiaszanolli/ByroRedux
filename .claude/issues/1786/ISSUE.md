# CONC-D3-04: CommandRegistry read guard held across arbitrary command execution; help re-enters the same lock

_Filed as #1786 from `docs/audits/AUDIT_CONCURRENCY_2026-07-02.md`._

**Severity**: LOW · **Dimension**: ECS Lock Ordering · Source: `AUDIT_CONCURRENCY_2026-07-02` (CONC-D3-04)

## Location
Dispatchers `crates/debug-server/src/evaluator.rs:413-417`, `byroredux/src/main.rs:268-269`, `byroredux/src/main.rs:2688-2689`; re-entry `byroredux/src/commands/world_info.rs:17`.

## Description
All three command dispatch sites hold a `ResourceRead<CommandRegistry>` while calling `reg.execute(world, expr)` (structurally unavoidable — the registry owns the boxed `ConsoleCommand` objects). Every command body runs with a live read guard on the `CommandRegistry` RwLock. `HelpCommand::execute` re-acquires it read-only (world_info.rs:17). The always-on thread-local tracker permits read-read, and no runtime writer exists, so this is currently benign.

## Evidence
```rust
// evaluator.rs:413-415
if let Some(reg) = world.try_resource::<CommandRegistry>() {
    if reg.list().iter().any(|(name, _)| *name == first_word) {
        let output = reg.execute(world, expr);   // guard `reg` held across execution
```
All commands run on the main thread (`DebugDrainSystem` is `add_exclusive(Stage::Late, …)`; drain releases its queue guard before evaluating).

## Impact
Two latent failure modes, both gated on code that does not exist yet: (a) any future command taking `resource_mut::<CommandRegistry>()` (e.g. runtime alias registration) panics via the always-on tracker (release included); (b) a cross-thread writer queued on the lock between the dispatcher's read and `help`'s re-entrant read could deadlock `std::sync::RwLock` (re-entrant read under a queued writer is platform-dependent).

## Related
CONC-D3-01 (same tracker behavior).

## Suggested Fix
Document the contract on `ConsoleCommand::execute` ("runs under a read guard on `CommandRegistry` — commands must never acquire it mutably"); optionally have `HelpCommand` receive the listing via the dispatcher instead of re-locking.

## Completeness Checks
- [ ] **LOCK_ORDER**: The `CommandRegistry` read-guard-held-across-execute contract is documented so no future command re-acquires it mutably
- [ ] **SIBLING**: All three dispatch sites carry the same contract note
- [ ] **TESTS**: (optional) a test that a mutable `CommandRegistry` acquire under dispatch trips the tracker
