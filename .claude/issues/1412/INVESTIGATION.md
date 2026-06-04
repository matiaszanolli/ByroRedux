# Investigation — #1412 TS-08 scheduler panic policy / partial state

**Domain:** sync / ecs (documentation)

## Finding
`Scheduler::run` already documented panic *stage-execution* semantics (added
2026-05-08, #867 — predates this audit), but two things the auditor flagged were
NOT covered: (1) the policy is intentional fail-fast, and (2) `std::sync::RwLock`
component storages POISON when a system panics mid-write.

## Investigation result (poisoning is already handled)
Storages are `std::sync::RwLock` (`World::storages`). Every acquisition site
resolves `PoisonError` via `storage_lock_poisoned::<T>()` /
`storage_lock_poisoned_erased()`, which re-panics with a diagnostic naming the
poisoned storage + the system that panicked first. So a post-panic access fails
loud — it never silently reads torn state. The audit's "leaves poisoned state"
is accurate but the consequence is a clear diagnostic panic, not silent
corruption.

## Fix (documentation only — per the issue's primary recommendation)
Extended the `# Panic propagation` doc on `Scheduler::run` with:
- **Policy: fail-fast is intentional** — continuing a frame with half-mutated ECS
  state risks silent corruption worse than a clean crash.
- **Lock poisoning** — std RwLock poisons; already handled by
  `storage_lock_poisoned`; and this is a concrete argument against a naive
  per-frame `catch_unwind` (a recovered frame would hit `storage_lock_poisoned`
  on every storage the failed system was writing). A real recovery scheme must
  wrap each rayon task in catch_unwind, serialize the first error, AND
  `clear_poison` the affected storages — deferred as a medium-term quality item.

The catch_unwind itself is intentionally NOT shipped (LOW; the issue scopes it
medium-term, and naive recovery is unsound here).

## Completeness
- [x] LOCK_ORDER: no RwLock scope change (doc only)
- [x] TESTS: N/A — doc only; poison handling already exists and is exercised by
  the world tests.

cargo test 2801 passed.
