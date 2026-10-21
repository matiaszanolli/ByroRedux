# #6289: GAME-D4-2026-10-21-05: `HeadLook` survives death: `head_look_system` keeps turning a corpse's head

**Labels**: low,gameplay,combat,animation,bug,game:fo3,game:fnv,game:oblivion

**Source**: docs/audits/AUDIT_GAMEPLAY_2026-10-21.md · **Severity**: LOW · **Dimension**: Combat & Death Pipeline (Dim 4, reconciler growth) · **Location**: `byroredux/src/systems/head_look.rs:52-127` (no `Dead` / ragdoll skip); `byroredux/src/combat.rs:1019-1128`

**Audit status**: NEW (from `d1b298921`)

- **Description**: ObScript `Look` stamps `HeadLook`. Only `StopLook`, or the target losing its `GlobalTransform`, removes it. `reconcile_dead_actor` does not remove it, and the system never checks `Dead` or `RagdollActive`. On a corpse it keeps writing the head bone's local `Transform`. Each frame it applies the yaw/pitch delta to whatever rotation the head currently has. The module doc assumes an animation re-poses the head every frame, and a corpse has no `AnimationPlayer`. On a corpse that still has a ragdoll, the ragdoll writes the head bone too, and which write lands last decides what is visible.
- **Impact**: Cosmetic. A dead `Look` caller's head can drift or rotate, most visibly when the ragdoll was refused and nothing else writes the bone.
- **Suggested Fix**: Remove `HeadLook` in `reconcile_dead_actor`, and skip `Dead` actors in `head_look_system` as a second guard.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other M47.3 per-actor components (`ActiveMagicEffects`, `HeadLook`, `FleeState`) checked against `reconcile_dead_actor`)
- [ ] **TESTS**: A regression test pins this specific fix
