# #3709 — ECS-P2-06: per-actor melee state (cooldown_remaining, blocking) lives in the global CombatState Resource

**Severity**: LOW · **Dimension**: P2 Gameplay Slice / Resource shape
**Location**: `byroredux/src/combat.rs` (`CombatState`)

## Fix

Implemented the issue's suggested fix: split `cooldown_remaining`/
`blocking` out of `CombatState` (a `Resource`, so it could only ever
represent one combatant) into a new `MeleeState` component
(`SparseSetStorage`, matching the sibling per-actor components in
`crate::components` like `HavokAnimationTarget`), attached to the
attacking entity. `CombatState` now holds only genuinely session-global
telemetry (`attacks_started`/`hits_landed`/`kills`/`last`).

`combat_input_system` resolves `MeleeState` on `aggressor` via
`query_mut` (inserting a default if the entity doesn't have it yet)
instead of the resource fields. Without an aggressor entity there's
nowhere to attach the state — but no attack could resolve past the
`let Some(aggressor) = aggressor else { record_miss(...) }` bail further
down anyway, so gating the whole cooldown mechanism on
`aggressor.is_some()` is a correctness improvement, not a behavior loss:
pre-fix, a phantom global cooldown could tick even with no player entity
to actually attack with.

`commands/view.rs`'s `combat.status` diagnostic (the only other reader)
now looks `MeleeState` up via `PlayerEntity` separately from
`CombatState`'s telemetry fields.

`MeleeState` needed registering in two more places, both structural
housekeeping this codebase already enforces:
- `boot.rs` — `world.register::<MeleeState>()`, or the real engine's
  `combat_input_system` would silently no-op forever (the established
  "storage isn't registered → query returns `None`" pattern).
- `save_io/registry_completeness_tests.rs`'s allowlist — every
  `Component`/`Resource` must be registered for save XOR explicitly
  classified; added `MeleeState` with the identical justification
  `CombatState` itself already carries ("session-local attack timing...
  canonical Health/Dead/EquippedWeapon state is saved separately").

## SIBLING (issue's own checklist item — "other gameplay `Resource`s checked for per-actor fields, e.g. `InteractionTrace`")

Checked `InteractionTrace` (the issue's own named example): clean —
`activation_count` is genuinely global telemetry, and `last` records the
single most-recent interaction event (interaction is inherently
single-target-per-frame by design, not per-actor concurrent state that
could collide). No fix needed.

## LOCK_ORDER (issue's own checklist item)

`MeleeState`'s write query and `CombatState`'s write guard never overlap
— the `MeleeState` query closure completes and drops before the
subsequent `if attack_ready { try_resource_mut::<CombatState>() }` block
opens. `EquippedWeapon`'s read (via `attack_cooldown_seconds`, resolved
into `armed_cooldown`) still runs and fully drops before either write
guard opens, unchanged from #3697's fix.

## TESTS (issue's own checklist item — "drives two simultaneous attackers and asserts independent cooldowns")

Added `two_combatants_have_independent_cooldowns`: since there is no NPC
melee producer yet (`combat_input_system` only ever arms its
`PlayerEntity` aggressor — the issue's own "Impact" section says as
much), this drives the real system for the player and confirms a second,
independently-seeded combatant's `MeleeState` is completely untouched —
the structural guarantee a shared `Resource` field could never make. This
is a test of the storage model itself, not of a specific reproducible
symptom the pre-fix code could exhibit (the whole component didn't exist
pre-fix), so unlike this session's other fixes there's no meaningful
"revert and confirm the exact failure" step here — the property under
test (two entities, two independent components) is definitionally
impossible to represent with one shared resource field, which is the bug
this fix removes structurally rather than behaviorally.

Also updated the three existing `attack_edge_fixture`-based tests
(`fly_cam_attack_press_does_not_burn_the_edge_or_arm_the_cooldown`,
`character_mode_attack_press_consumes_the_edge_and_arms_the_cooldown`,
`cooldown_keeps_ticking_down_in_fly_cam`) to read cooldown state via the
new `melee_state_of` helper instead of `CombatState`, and to use a real
spawned aggressor entity (the pre-split fixture used `PlayerEntity
(None)`, which is no longer sufficient to exercise arming at all once
cooldown state lives on the entity).

## Verification

- `cargo check -p byroredux --tests`: clean.
- `cargo test -q -p byroredux --bin byroredux`: 1,872 tests passing, 0
  failing (+1 new).
- `cargo test -q --no-fail-fast` (full workspace): **7093 passing, 0
  failing**.
