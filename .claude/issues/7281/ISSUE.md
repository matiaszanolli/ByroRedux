# #7281: PHYS-D5-03: `stance_use_system` banks Athletics / Sneak skill use while the player is dead, input-locked, or in the fly camera

- **Severity**: LOW
- **Dimension**: Character & NPC Controller (handoff: skill-use leveling, `/audit-character`)
- **Location**: `byroredux/src/skill_use_emit.rs:191-235`
- **Status**: NEW
- **Description**: The stance is derived from the raw `ActionState` keys, `Sneaking`, detection and `actor_is_swimming`. None of the gates the character controller applies are checked:
  - `Dead`;
  - `PlayerMode` (fly camera);
  - `player_accepts_movement_input` (`PlayerControlState`, `ActorControlState.restrained`).
  Holding a move key therefore advances the run and swim timers and queues `ATHLETICS` uses while the body cannot move.
- **Evidence**: `let moving = [MoveForward, MoveBackward, StrafeLeft, StrafeRight].into_iter().any(|action| actions.is_held(action));` feeds `stance_of(moving, …)`. There is no `Dead` / mode / input-acceptance check before `stance_step`.
- **Trigger Conditions**: Oblivion, with the player dead, restrained or script-locked, or in fly-cam mode, while holding a movement key.
- **Impact**: Unearned Athletics (and swim Athletics) skill progress.
- **Related**: #7051 (skill-use emitters, 72a73d765), #7056
- **Suggested Fix**: Gate the stance on the controller's own predicate. Moving should mean the controller actually accepted movement this frame, for example its post-move horizontal displacement, or `player_accepts_movement_input` plus Character mode plus not `Dead`.

**Found by**: `docs/audits/AUDIT_PHYSICS_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: the other skill-use emitters (hit, block, jump) checked for the same gates
- [ ] **TESTS**: a dead / fly-cam / restrained player holding a move key banks nothing
