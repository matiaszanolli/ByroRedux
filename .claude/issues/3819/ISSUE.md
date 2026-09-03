# #3819 — lock-order-check CI job is red at HEAD: 26 tests fail with cross-thread ABBA cycles

**Severity**: HIGH · **Source**: filed while verifying #3655 under `BYRO_LOCK_ORDER_CHECK=1`

## Investigation and fix — all four cycle families closed

Filed with four distinct cycle families captured from a full failing
run. Investigated and fixed each in turn; `BYRO_LOCK_ORDER_CHECK=1 cargo
test --workspace` (the exact command CI's `lock-order-check` job runs)
now passes **97/97 test binaries, 0 failures** — confirmed with a final
full run after all four fixes landed.

### Family 1 — `Transform ↔ GlobalTransform` (15 tests, the largest family)

Dispatched an investigation to trace every acquisition site. Seven-plus
sites — including `make_transform_propagation_system`
(`crates/core/src/ecs/systems.rs`), the canonical anchor
`docs/engine/ecs.md` cites for the project's whole documented lock
order — consistently acquire `Transform` before `GlobalTransform`. Found
exactly **one** outlier: `byroredux/src/extensions.rs::capture_spatial_snapshot`
acquired `GlobalTransform` before `Transform`, both held live through the
whole loop. Swapped the two `let` bindings to match the dominant
convention. This single two-line fix cleared all 15 tests across
`ragdoll::tests::*`, `systems::bounds::tests::*`,
`systems::character::tests::camera_follow_does_not_close_character_lock_cycle`,
`systems::escort::tests::*`, `systems::follow::tests::*`,
`systems::guard::tests::*`, `systems::travel::tests::*`.

### Family 2 — `ActorValues ↔ GlobalFormIdResolver`

`byroredux/src/extensions.rs::capture_entity_projections` acquires
`ActorValues` then `GlobalFormIdResolver` (a `if let (Some(a), Some(b))
= (expr_a, expr_b)` tuple, evaluated left-to-right). The reverse-order
outlier was `apply_pending_actor_value_writes`, which acquired
`GlobalFormIdResolver` first, then `ActorValues` — reordered to match,
and added an explicit `drop(resolver);` alongside the existing
`drop(values);` since `resolver` is unused past the read loop and would
otherwise also overlap the later `query_mut::<ActorValues>()` write pass.

### Family 3 — `FactionReputation ↔ GlobalFormIdResolver`

Same shape as Family 2: `capture_entity_projections` acquires
`FactionReputation` then `GlobalFormIdResolver`;
`apply_pending_reputation_writes` had the reverse order. Reordered to
match (this site already had the early `drop(live); drop(resolver);`
hygiene in place — just in the wrong acquisition order).

### Family 4 — `QuestObjectiveState ↔ QuestStageState`

Different mechanism from the other three: not a production code-path
disagreement, but a **same-thread test hygiene bug** entirely within
`save_io::live_reload_tests::quest_stage_and_objective_state_survive_snapshot_round_trip`.
The thread-local lock-order graph tracks acquisitions by **type**, not
by which `World` instance a `ResourceRead` guard came from — the test
held `restored_stages` (`QuestStageState`, from `restored_world`) across
an un-dropped lexical scope while later acquiring
`QuestObjectiveState` (also `restored_world`), then held THAT across
acquiring `QuestStageState` again from a **completely different**
`overlay_world` instance. Three un-dropped-until-function-end guards
closed a same-thread cycle even though two of the three "worlds" were
distinct objects. Added explicit `drop()` calls at each guard's last
use, matching the early-drop discipline already established elsewhere
in this codebase.

### The 97th binary — a self-inflicted CI gate on the gate

After all four production/test fixes, one more failure surfaced:
`crates/core/tests/lock_tracker_allocation_bounds.rs`'s own
`nested_read_lock_costs_no_more_than_an_isolated_one_when_disabled`
(#3680) `assert!`ed that `BYRO_LOCK_ORDER_CHECK` was unset — correct
that its own measurement is meaningless with the detector active, but
an `assert!` makes CI's `lock-order-check` job (which runs the whole
workspace suite with that var set) permanently red regardless of any
real regression, which is exactly the failure mode this whole issue is
about. Changed to an early `return` (with an explanatory `eprintln!`)
instead of a failing assertion — "not applicable in this mode" is a
skip, not a failure.

## Impact

The CI `lock-order-check` gate was non-functional as a regression
signal — permanently red, unable to catch a *new* cycle landing on top
of the existing ones. Now genuinely green, so it can do its job again.

## Completeness Checks
- [x] **LOCK_ORDER**: all four families' pairs now acquire in a consistent order (or, for #4, guards are dropped before the next acquisition)
- [x] **TESTS**: `BYRO_LOCK_ORDER_CHECK=1 cargo test --workspace` green — 97/97 binaries passing, 0 failures, verified in a fresh full run
- [x] Confirmed CI's exact command (`.github/workflows/ci.yml`'s `lock-order-check` job) reproduces the fix locally

## Verification

- `cargo check -p byroredux -p byroredux-core --tests`: clean, zero
  warnings.
- `cargo test -q -p byroredux --bin byroredux extensions:: save_io::live_reload_tests::`:
  all passing.
- `BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux --bin byroredux`:
  1894 passing, 0 failing (was 26 failing).
- `BYRO_LOCK_ORDER_CHECK=1 cargo test -q --workspace` (CI's exact
  command): **97 test binaries, all `ok`, 0 `FAILED`**.
- `cargo test -q --no-fail-fast` (full workspace, normal mode — the
  project's primary gate): **7178 passing, 0 failing** (unchanged count
  — no new tests, only reordering + hygiene fixes).
