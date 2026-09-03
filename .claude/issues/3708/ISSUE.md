# #3708 — ECS-P2-03: the alive→dead transition never retires the actor from the ambient-package scheduler

**Severity**: MEDIUM · **Dimension**: P2 Gameplay Slice / Component Lifecycles
**Location**: `byroredux/src/combat.rs` (`reconcile_dead_actor`) + `byroredux/src/npc_spawn/ai_package.rs` (`ambient_ai_package_system` passes 1-3)

## Fix

Implemented the issue's primary suggested fix (not the "cheaper
alternative"): added `remove_component::<AmbientPackageRuntime>` and
`remove_component::<EvaluatePackageRequest>` to `reconcile_dead_actor`'s
death-only teardown — a corpse has no package to select, so the whole
runtime component is removed rather than merely skipped-around, permanently
retiring the actor from all three of `ambient_ai_package_system`'s passes
(not just pass 3's clone).

**Why the removal had to go in `reconcile_dead_actor`, not
`clear_ambient_behavior`**: `disable_actor_ai` → `clear_ambient_behavior`
already removes every `*Behavior`/`*State` marker component
(`SandboxBehavior`, `WanderBehavior`/`WanderState`,
`TravelBehavior`/`TravelState`/`Traveled`,
`FollowBehavior`/`FollowState`, `EscortBehavior`/`EscortState`/`Escorted`,
`GuardBehavior`/`GuardState`, `PatrolBehavior`/`PatrolState`, `Seated`),
but that function is **shared** with the live schedule-handover path
inside `ambient_ai_package_system` itself (a saloon patron's daytime
Sandbox package giving way to an evening Travel package, per the #3333
comment in that function). `AmbientPackageRuntime` holds the actor's
`last_evaluated_game_minute`/`active_package_form_id`/
`package_candidates` — state that **must survive** a live handover, since
it's what drives the *next* evaluation. Removing it inside
`clear_ambient_behavior` would have wiped a live actor's AI runtime state
on every ordinary schedule change. The fix is deliberately placed only in
`reconcile_dead_actor`'s own death-specific teardown, alongside the
existing ragdoll/animation cleanup.

`EvaluatePackageRequest` was added alongside for the same reason (a
corpse needs neither), though it's lower-priority on its own — it's
already a one-shot marker `scripting::package::scene_package_system`
drains every tick regardless of the entity's `Dead` status.

## SIBLING (issue's own checklist item — "the rest of the death-teardown roster checked for other survivors driving per-frame work")

Every other per-actor behavior/state component `clear_ambient_behavior`
already covers is removed on death via the existing `disable_actor_ai`
call — `AmbientPackageRuntime` was genuinely the one component in this
runtime-state family the existing teardown missed. Checked one other
candidate with a superficially similar shape, `ScriptTimer`
(`crates/scripting/src/timer.rs`): it's `Copy` (no heap allocation to
clone), and it self-terminates — a stray timer on a corpse ticks down at
zero real cost and removes itself once expired, unlike
`AmbientPackageRuntime`'s frozen-forever gate that pays a real allocation
every mismatched-minute frame forever. No fix needed there.

## LOCK_ORDER (issue's own checklist item)

No `RwLock` scope changed — two more `remove_component::<T>()` calls
(each its own independent `query_mut::<T>()` acquire-and-drop, same shape
as every other call in this function) added sequentially, not nested with
anything else.

## TESTS (issue's own checklist item — "kills an actor and asserts zero package-runtime clones on subsequent frames")

Extended the existing `dead_state_reconciliation_removes_respawned_ai`
test with an `AmbientPackageRuntime` + `EvaluatePackageRequest` fixture
and asserts both are gone after `reconcile_dead_actor_runtime_state` runs
— the practical equivalent of "zero future clones": once the component is
absent, `ambient_ai_package_system`'s pass-1 query structurally can never
see the entity again, which is a stronger guarantee than counting clones
would be (nothing to instrument, no future frame can regress it back into
existence).

Verified the guard actually catches the regression (this session's
established quality bar): removed both new `remove_component` calls,
reran — the test failed with exactly the expected assertion message, then
restored the fix and confirmed a clean pass again.

## Verification

- `cargo check -p byroredux --tests`: clean.
- `cargo test -q -p byroredux --bin byroredux`: 1,871 tests passing, 0
  failing (test extended, not added — same count as before).
- `cargo test -q --no-fail-fast` (full workspace): **7092 passing, 0
  failing**.
