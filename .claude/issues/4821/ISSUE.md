# #4821 — GAME-D7-2026-09-24-02: The `npc_combat_ai_system` access declaration claims to cover every pair `clear_ambient_behavior` removes, but declares none

**Labels**: low,gameplay,concurrency,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: LOW
- **Dimension**: 7 — Stage order / access
- **Location**: `byroredux/src/boot/schedule/update.rs:256-264`; `byroredux/src/npc_spawn/ai_package.rs:436-499`
- **Status**: NEW (introduced by #4703, 6c517bc7a)
- **Description**:
  - The comment says the declaration covers "every ambient Behavior/State pair that function removes". Only `AmbientPackageRuntime`, `Seated`, `SeatReservations` and `AnimationPlayer` are declared.
  - The 17 types removed through the generic `remove_component::<T>` go undeclared: seven Behavior/State pairs, plus `Traveled`, `Escorted`, `WalkStuckTimer` and `NavPath`.
  - The `acquired_in` guard can't see through the generic helper.
- **Impact**: diagnostics only. The system runs exclusively, but the reported write set is wrong.
- **Related**: #4574.
- **Suggested Fix**: declare the 17 writes, or share one declaration fragment with `reconcile_dead_actor`'s callers. Teach the guard to follow `remove_component::<T>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
