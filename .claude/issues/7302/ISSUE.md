# #7302: SCR-D3-03: Two new `HitEvent` producers resolve `weapon_ignores_resistance` while holding the HitEvent write guard

- **Severity**: LOW
- **Dimension**: Event Runtime (lock discipline)
- **Location**: `byroredux/src/combat.rs:310-347`, `byroredux/src/systems/combat_ai.rs:266-291`
- **Status**: NEW (introduced by fba412ebb)
- **Untrusted-Input**: No
- **Description**: In both producers the struct literal passed to `events.insert(..)` calls `crate::combat::weapon_ignores_resistance(world, ..)`, which takes `world.get::<EquippedWeapon>` and `try_resource::<LoadedCellIndex>`. Both run under the live `query_mut::<HitEvent>()` guard (HitEvent write, then EquippedWeapon read, then a resource read). The sibling producer in the same commit window resolves before the guard: `systems/vats.rs:959` ("resolved before the events guard is taken").
- **Evidence**: `combat_ai.rs:291`: `ignores_resistance: crate::combat::weapon_ignores_resistance(world, decision.entity),` inside `for decision in decisions.iter_mut() { … events.insert(..) }` under `if let Some(mut events) = world.query_mut::<HitEvent>()` (line 266). `combat.rs:347` is the same shape inside `events.insert(target, HitEvent { … })`.
- **Impact**: No inverse edge exists today. I grepped every `query_mut::<EquippedWeapon>` and `resource_mut::<LoadedCellIndex>` site: `crafting.rs:410`, `inventory/loot.rs:293,324`, `fragment/effects/inventory.rs:117`, `script_requests.rs:627`, `relocated.rs:1074`. None of them holds HitEvent at the time. It is a latent edge for `BYRO_LOCK_ORDER_CHECK`.
- **Related**: none
- **Suggested Fix**: Hoist the value into a local before taking `events`. `combat_ai` can collect it into `decisions`, as vats does for `sneaks`.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
