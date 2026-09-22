# ECS-2026-09-21-D5-02: declared `Access` rows of five exclusive systems omit types their own bodies acquire

**Labels**: low, ecs, concurrency, gameplay, combat, bug

Filed via /audit-publish from docs/audits/AUDIT_ECS_2026-09-21.md.

**Severity**: LOW (exclusives are never paired; this affects `sys.accesses` diagnostics and the basis for promoting a system to a parallel lane) · **Dimension**: 5 (declared access)
**Location**: `byroredux/src/boot/schedule/update.rs`, the `add_exclusive_with_access` registrations of `interaction_system`, `container_loot_system`, `combat_input_system` and `npc_combat_ai_system`. `byroredux/src/boot/schedule/late.rs`, the registration of `reconcile_pending_dead_actors_system` and the plain `add_exclusive` of `ragdoll_writeback_system`.
**Verified against**: HEAD `f97775ca8`

## Description

The P2 scope comment above these registrations (in `update.rs`, just before `interaction_system`) says each declaration "covers the system body and the helpers it calls directly". A body scan at HEAD finds these acquisitions missing from the declarations:

| System | Missing type(s) | Where acquired |
|---|---|---|
| `combat_input_system` | `MeleeState` (read **and** write) | Its own body in `combat.rs`: `query_mut::<MeleeState>` in the cooldown arm and `query::<MeleeState>` in the rejected-edge log. Undeclared since #3709's split (2026-09-03). |
| `combat_input_system` | `CreatureAttack` (read) | `attack_damage`, called directly |
| `interaction_system` | `PhysicsSourceForm` (read) | `collider_belongs_to_target` (`interaction.rs`), reached through `select_interaction_target` → `target_has_line_of_sight`. The same helper's `FormIdComponent` and `ActorColliderOwner` reads **are** declared. |
| `container_loot_system` (registered 09-16 in `3cbf3674a`; the pickup and theft reach landed 09-19 in `479163836`) | `PickedUp` (write) | `pickup_loot` |
| `container_loot_system` | `Owned`, `FactionRanks` (read) | `transfer_is_theft` |
| `npc_combat_ai_system` (new, 09-13) | `WalkSpeed` (read) | Its own body in `systems/combat_ai.rs` (chase speed) |
| `reconcile_pending_dead_actors_system` | `AnimationTarget` (read) | `reconcile_dead_actor` (`combat.rs`), before the ragdoll activation that its scope comment excludes. The sibling `combat_damage_system` does declare `.reads::<AnimationTarget>()`. |

The same comment says the ragdoll activation's "own physics surface is declared by `ragdoll_writeback_system`". But `late.rs` registers that system with a plain `scheduler.add_exclusive(Stage::Late, crate::ragdoll::ragdoll_writeback_system)`, so that surface is declared nowhere.

Only `papyrus_provider_system` and `legacy_obscript_load_order_system` have an acquisition-completeness test (#3951). `p2_gameplay_exclusives_declare_non_empty_access` (`scheduler_access_tests.rs`) checks only that the P2 rows are non-empty and contain the specific types #3473 disputed.

## Evidence

- `combat.rs`, in `combat_input_system`: `world.query_mut::<MeleeState>().is_some_and(|mut melee| { … melee.insert(aggressor, MeleeState::default()); … })`.
- `systems/combat_ai.rs`, in `npc_combat_ai_system`: `world.query::<crate::components::WalkSpeed>()`.
- `combat.rs`, in `reconcile_dead_actor`: `world.get::<AnimationTarget>(actor)`.
- None of these types appears in the corresponding `Access::new()…` chain in `update.rs` / `late.rs`.

## Impact

`sys.accesses` understates the hold set of the newest gameplay systems. Any future promotion to a parallel lane would start from an incomplete declaration, which is the exact risk `assert_declares_everything_it_acquires`'s own panic text describes.

## Related

- Precedents, all closed: #3951 and #3275 (both LOW), and #3473 (the P2 exclusives' bare `add_exclusive`).
- ECS-2026-09-21-D7-01 (#4571) is about the same `PickedUp` marker that `container_loot_system` writes without declaring.
- ECS-2026-09-21-D5-01 (#4573): the completeness helper this fix would reuse has mode and substring holes. Fix it first, or the new exclusive tests inherit them.
- CONC-D3-2026-09-21-02 in `docs/audits/AUDIT_CONCURRENCY_2026-09-21.md` covers `npc_combat_ai_system`'s hold stack and cites this finding for the undeclared `WalkSpeed`.

## Suggested Fix

- Add the missing types to the five declarations.
- Either correct the `ragdoll_writeback_system` sentence, or give that system an `add_exclusive_with_access` row.
- Run `assert_declares_everything_it_acquires` over the declared P2 exclusives, with per-system source tables like the `PARALLEL_SYSTEMS` entries, so the next undeclared acquisition fails a test.

Source: docs/audits/AUDIT_ECS_2026-09-21.md (ECS-2026-09-21-D5-02)

## Completeness Checks
- [ ] **SIBLING**: The other declared exclusives in `update.rs` / `late.rs` (e.g. `combat_damage_system`) are re-scanned for the same gap
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: An acquisition-completeness test covers each of the five exclusives, so a new undeclared acquisition fails it.
