# #3655 — CONC-D5-2026-08-30-02: `ragdoll_writeback_system` acquires `LocalBound`/`WorldBound` under the `PhysicsWorld` guard — a second `PhysicsWorld -> storage` edge

**Severity**: MEDIUM · **Dimension**: D5 · RwLock Patterns — Resource<->Storage & Physics Step
**Location**: `byroredux/src/ragdoll.rs::ragdoll_writeback_system`

## Fix

Verified the premise: `local_bound_q`/`world_bound_q` were acquired
AFTER `world.try_resource::<PhysicsWorld>()`, inverting the crate-wide
"no storage under a `PhysicsWorld` guard" rule this same function's own
comment already documents for the `GlobalTransform`/`PhysicsWorld` pair
(#313). Confirmed via the issue's own reasoning that this is defence-in-
depth, not a live cycle today — `make_world_bound_propagation_system`
never touches `PhysicsWorld`, so no opposing edge exists yet — but it
becomes a real cycle the moment any bounds-side code reaches a physics
query.

Confirmed the sibling live-cycle issue this one names as a prerequisite
(**CONC-D5-2026-08-30-01**, #3580) is already closed, and the canonical-
order-table issue (**CONC-D5-2026-08-30-03**, #3656) is already closed
too — this fix completes the class both of those started.

Applied the issue's own suggested fix exactly: hoisted `local_bound_q`
and `world_bound_q` above the `try_resource::<PhysicsWorld>()` line.
Both queries are independent of `pw`, so this is a pure reordering — no
behavioral change. `PhysicsWorld` is now the LAST acquisition in the
function with nothing taken under it.

Also extended `docs/engine/ecs.md`'s "Physics shapes" worked-example
list (the section #3656 added) to name `ragdoll_writeback_system`
alongside `collect_newcomers`/`push_kinematic`/`combat_input_system`/
`interaction`'s line-of-sight ray, so the rule's documented example set
stays in sync with the code that now follows it.

## SIBLING (issue's own checklist item — "landed with or immediately
after CONC-D5-2026-08-30-01")

#3580 (the live cycle) and #3656 (the canonical-order-table gap) are
both already closed; this issue was the one remaining open member of
the class.

## LOCK_ORDER (issue's own checklist item)

`PhysicsWorld` is the final acquisition in `ragdoll_writeback_system`
with no storage taken under it; the canonical
`Transform -> Parent -> Children -> GlobalTransform -> LocalBound ->
WorldBound -> PhysicsWorld` order is intact.

## TESTS (issue's own checklist item — "`BYRO_LOCK_ORDER_CHECK=1 cargo
test -p byroredux --bins` green, and the ragdoll tests specifically")

Added `ragdoll_writeback_acquires_bound_queries_before_physics_world`, a
source-scan pin (the same class of guard this session uses for
structural/ordering invariants that a runtime assertion can't reach,
matching `docs/engine/ecs.md`'s own reasoning: a violation here is
circumstantial-safe until some future code path closes the opposing
edge, so the only test that can catch a regression *before* that happens
is textual).

The ragdoll-specific tests (`ragdoll::tests::`) pass cleanly under
`BYRO_LOCK_ORDER_CHECK=1`, both alone and as part of the full crate
suite.

**`BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins` itself is
NOT green** — 26 tests fail with unrelated cross-thread lock-order-cycle
panics (`Transform`/`GlobalTransform`, `ActorValues`/
`GlobalFormIdResolver`, `FactionReputation`/`GlobalFormIdResolver`,
`QuestObjectiveState`/`QuestStageState`). Verified via `git stash` that
**this is pre-existing and identical with or without this fix** — the
same 26 tests fail the same way on unmodified `main`, and every one of
them passes in isolation (`--test-threads=1` on a single failing test),
confirming this is cross-test global-graph interaction, not anything
`ragdoll_writeback_system` (or this fix) touches. Filed separately as
**#3819** since it's a distinct, much larger investigation (the CI
`lock-order-check` job is apparently red at HEAD right now, unrelated to
this issue's scope).

## Verification

- `cargo check -p byroredux --tests`: clean, zero warnings.
- `cargo test -q -p byroredux --bin byroredux ragdoll::`: 16 passing, 0
  failing (+1 new).
- `BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux --bin byroredux
  ragdoll::`: 16 passing, 0 failing.
- `cargo test -q --no-fail-fast` (full workspace, without the lock-order
  env var — the project's normal gate): **7177 passing, 0 failing**.
