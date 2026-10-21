# #6230: GAME-D4-2026-10-21-01: `PushActorAway` tears an actor's animation, package and fight down before ragdoll activation can fail; on failure it is left frozen with no package, and the player loses its body idle

**Labels**: medium,gameplay,combat,ai,bug,game:fo3,game:fnv,game:oblivion

**Source**: docs/audits/AUDIT_GAMEPLAY_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Combat & Death Pipeline (Dim 4, M47.3 script-driven actor states) · **Location**: `byroredux/src/systems/knockdown.rs:90-125` (`knock_down`), `:130-142` (`release_actor_for_ragdoll`); unchanged-winner short-circuit `byroredux/src/npc_spawn/ai_package.rs:1129-1136`

**Audit status**: NEW (from `8d32b3fe9`, 2026-10-18)

- **Trigger**: FO3 / FNV / Oblivion. ObScript `PushActorAway` (`obscript_more/actions.rs:94-101`), or `TrapUpdate` with a push force (`obscript_more/extras.rs:323-327`, which pushes every actor in contact). The target can be one of:
  - the player, whose visual body has no `RagdollTemplate` (`npc_spawn/resumable/mod.rs:52`);
  - an NPC whose seed pose `activate_ragdoll` rejects (#5161, `ragdoll.rs:327-...`);
  - any actor without a template.
- **Description**: `knock_down` runs `release_actor_for_ragdoll` first. That calls `clear_ambient_behavior`, removes `AiCombatState`, and removes `AnimationPlayer` / `AnimationStack` on the actor and its skeleton root. Only then does it call `activate_ragdoll(world, root)?`. On `Err`, the `?` returns before `KnockedDown` and `AiSuspended` are inserted. `get_up_system` therefore never sees the actor, and `stand_up` (which re-inserts the idle and calls `rearm_ai`) never runs. `clear_ambient_behavior` deliberately leaves `AmbientPackageRuntime.active_package_form_id` set. On the next ambient tick the winner is unchanged, and the `!changed => continue` branch never reinstalls the cleared behavior.
- **Evidence**: `knockdown.rs:101-104`:
  ```rust
  release_actor_for_ragdoll(world, actor, root);
  if world.get::<crate::ragdoll::RagdollActive>(root).is_none() {
      crate::ragdoll::activate_ragdoll(world, root)?;
  }
  ```
  The player capsule carries `AnimationTarget` → body skeleton plus the KF idle `AnimationPlayer` (`player_body.rs:353-399`). Both are removed, and nothing re-inserts them.
- **Impact**:
  - **NPC**: it stops animating and stands in its last sampled pose with no package behavior until the schedule's winner changes. A scripted fight it was in is lost.
  - **Player**: the third-person body loses its idle for the session; only the walk take animates it.
  - **Traps**: `TrapUpdate` re-raises the push every frame of contact while `continuous` is set, so the warning repeats.
  - The failure is logged only as `PushActorAway: actor … not pushed`. I did not count how often vanilla scripts target the player.
- **Related**: GAME-D4-02 (the success path's `stand_up`); #5161 (seed rejection).
- **Suggested Fix**: Check `RagdollTemplate` presence (or activate first) before releasing anything. On activation failure, restore nothing and return. Refuse the player explicitly, the same way `knock_down` refuses the dead. Pin it with a test: a template-less actor keeps its `AnimationPlayer` and its installed behavior.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other release-then-activate sequences: death teardown, `TrapUpdate` push, revive)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
