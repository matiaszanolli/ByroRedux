# CONC-D3-2026-09-21-02: The new P2 combat systems re-open two closed hold-stack patterns (#3444 shadowed guard; #3473 hold across a helper)

**Labels**: low, concurrency, gameplay, combat, bug

Filed via /audit-publish from docs/audits/AUDIT_CONCURRENCY_2026-09-21.md.

**Severity**: LOW (latent; no reverse edge exists today, and both systems are exclusive) · **Dimension**: 3 — ECS Lock Ordering & Deadlock (guard lifetime in system bodies)
**Location**:
- `byroredux/src/systems/combat_anim.rs:104-107` (`combat_feedback_system_inner`), with the guard live through the sound pass at `:329-417`
- `byroredux/src/systems/combat_ai.rs:61-156` (`npc_combat_ai_system`'s read pass) → `byroredux/src/combat.rs:369-465` (`attack_damage` → `melee_damage_charal_bonus`)

**Status**: NEW (recurrences of closed #3444 and #3473)
**Verified against**: HEAD `f97775ca8`.

## Description

1. **`combat_feedback_system_inner`** (ec3a18d2f, 2026-09-21):
   - `let Some(clips) = world.try_resource::<DraugrCombatClips>() else { return; }; let clips = *clips;` shadows the guard without dropping it. This is the exact #3444 defect (`let config = *config;`).
   - The `DraugrCombatClips` read guard therefore stays live for the whole function:
     - the read pass: `HitEvent`, `CombatState`, `PlayerEntity`, `GlobalTransform`, `PlayerMode`, `DraugrCombatAnim`, `Dead`, `AnimationPlayer`, `AnimationTarget`;
     - the write pass: the `AnimationPlayer` and `DraugrCombatAnim` writes;
     - the sound pass, commented "after every component write, locks dropped" (`:329`): `SoundArchiveProvider` (BSA extract + decode), then an `AudioWorld` write.
2. **`npc_combat_ai_system`** (f61ea0447, 2026-09-13):
   - The read pass holds the `AiCombatState` and `Transform` query guards (`combat_q`, `transform_q`) across its loop, and calls `crate::combat::attack_damage(world, entity)` inside it.
   - `attack_damage` → `melee_damage_charal_bonus` takes `MeleeDamageConfig` (scoped) → `CharacterRuleset` → `ActorValues` → `CharacterLevel` beneath those guards, plus `EquippedWeapon` / `CreatureAttack`. The same loop also takes `Dead`, `WalkSpeed`, and `EquippedWeapon` again (via `attack_reach_bu` / `attack_cooldown_seconds`).
   - That is the five-deep hold stack across a helper call that `attack_damage`'s own #3473 comment says the #2270 "snapshot before you iterate" house rule prohibits. #3473 fixed the callee's own `EquippedWeapon` binding; this caller re-creates the stack one level up.
   - The new `Transform → CharacterRuleset / ActorValues` edges are not in `docs/engine/ecs.md`, which records only `CharacterRuleset → ActorValues` (#3441). The combat_ai tests install no weapon or ruleset, so `attack_damage` returns before the CHARAL chain and the detector never records these edges.

## Evidence

No cycle exists today:
- `DraugrCombatClips`' only other readers (`populate_draugr_combat_clips` and its test) take `&mut World`.
- `CharacterRuleset → ActorValues → CharacterLevel` is consistent at `crates/core/src/character/regen.rs:213-226` and `crates/scripting/src/condition.rs:676-682`.
- Every AI-walker `WalkSpeed`/`Dead` read happens under a live `Transform` read.

## Impact

- Both sites add spurious or undocumented edges to the lock graph, and the combat_feedback comment "locks dropped" is false.
- A future `Transform`-after-CHARAL site, or promoting either system to a parallel lane, would close a cycle with no test signal. With the `lock-order-check` CI lane red, there would be no CI signal either (CONC-D3-2026-09-21-01 (#4603)).

## Related

- #3444 (closed; shadowed guard), #3473 (closed; hold across a helper), #2270 (the house rule), #4325 (closed; the `PhysicsWorld` half of combat_ai's lock discipline).
- ECS-2026-09-21-D5-02 (#4574): combat_ai's undeclared `WalkSpeed` in its `Access` row, plus four other exclusives' under-declarations.
- CONC-D3-2026-09-21-01 (#4603): the CI lane that would have to catch a cycle here.

## Suggested Fix

- Take the clips copy in a scoped expression so the guard dies at the copy, e.g. `let Some(clips) = world.try_resource::<DraugrCombatClips>().map(|c| *c) else { return; };`.
- In `npc_combat_ai_system`, compute per-attacker reach, damage and cooldown after the `AiCombatState`/`Transform` guards drop, in a second pass over the collected entities.
- Otherwise, document `Transform → CharacterRuleset` (and `AiCombatState → …`) in `docs/engine/ecs.md`.

Source: docs/audits/AUDIT_CONCURRENCY_2026-09-21.md (CONC-D3-2026-09-21-02)

## Completeness Checks
- [ ] **LOCK_ORDER**: after the change, the guard scopes preserve TypeId-sorted acquisition, and every remaining nested edge is in `docs/engine/ecs.md`
- [ ] **SIBLING**: the other combat / AI systems (`combat_input_system`, the AI walkers) checked for `let x = *x;` guard shadowing and for helper calls under live guards
- [ ] **TESTS**: a combat_ai test that installs `EquippedWeapon` + `CharacterRuleset` + `MeleeDamageConfig` runs under `BYRO_LOCK_ORDER_CHECK=1`, so the detector records the CHARAL edges
