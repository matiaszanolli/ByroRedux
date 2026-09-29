# #4996: CONC-D5-2026-09-28-01: `container_loot_system`'s Access row does not declare the `PhysicsWorld` write that #4818 added to `pickup_loot`

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,concurrency,ecs,gameplay,inventory,bug
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW. The system is exclusive, so no parallel pair exists today. This is promotion-baseline drift of the #4574 / #4821 class.
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics)
- **Location**: `byroredux/src/boot/schedule/update.rs:169-190` (the declaration); `byroredux/src/inventory.rs:1108-1160` (`pickup_loot`, reached from `container_loot_system` at `inventory.rs:1026`); `byroredux/src/npc_spawn/loot_appearance.rs:129-180` (`collision_entities_of` and `remove_collision_bodies`)
- **Status**: NEW. It was introduced by 6c5555c70 (#4818, 2026-09-24), two days after #4574 (0f0287519) completed this declaration. `AUDIT_ECS_2026-09-28` Dim 5 did not catch it, because the #4573 mode-aware guard scans parallel systems only.
- **Description**: #4818 made `pickup_loot` remove the taken item's Rapier bodies:
  - `collision_entities_of` reads `FormIdComponent` and `PhysicsSourceForm`.
  - `remove_collision_bodies` reads `RapierHandles`, drops that guard, then takes `try_resource_mut::<PhysicsWorld>()`.

  None of these appears in the declaration. It lists PlayerNotifications, Dead, EquipmentSlots, EquippedWeapon, EquipmentEventBatch, PlayerEntity, InventoryCatalog, ActivateEvent, SceneAliasCandidate, Locked, PickedUp, Owned, FactionRanks, PlacedItemCount and Inventory.

  The same function also reaches undeclared `Children`/`MeshHandle` (through `mesh_entities_under`, `inventory.rs:1147`) and a `PersistentReferenceStates` write (through `mark_picked_up`, `reference_state.rs:337-360`). This makes it a second `PhysicsWorld` writer outside Stage::Physics that `sys.accesses` and the conflict analyzer cannot see.
- **Evidence**: `container_loot_system` → `pickup_loot` → `remove_collision_bodies`, which acquires in this order:
  1. `world.get::<FormIdComponent>(root)` (temporary guard)
  2. `query::<PhysicsSourceForm>` (collect, drop)
  3. `query::<RapierHandles>` (collect, explicit `drop`, `loot_appearance.rs:167-172`)
  4. `try_resource_mut::<PhysicsWorld>()` (`:173`)

  The guard order itself is correct: snapshot, then acquire, with no overlap.
- **Trigger Conditions**: Only if `container_loot_system` is promoted to the Update parallel batch, or a parallel Update system that reads `PhysicsWorld` is added. Update already has five parallel-adjacent `reads_resource::<PhysicsWorld>` declarations (`update.rs:125,203,237,257`). The analyzer would then report no conflict for a real `PhysicsWorld` write/read race.
- **Impact**: None at runtime today. The declaration is the only record of this write, so it is wrong exactly where a future promotion would rely on it.
- **Verification Path**: `grep -n "PhysicsWorld\|RapierHandles\|PhysicsSourceForm" byroredux/src/boot/schedule/update.rs` shows no hit inside the `container_loot_system` block (lines 169-190). `remove_collision_bodies` is at `loot_appearance.rs:160`.
- **Related**: #4574 (closed, same class), #4821 (open, same class on `npc_combat_ai`), #4818, #4983. Overlaps with the Dim 4 scheduler-declaration sweep; file once.
- **Suggested Fix**: Add these to the `container_loot_system` Access row:
  - `.writes_resource::<PhysicsWorld>()`
  - `.reads::<RapierHandles>()`, `.reads::<PhysicsSourceForm>()`, `.reads::<FormIdComponent>()`, `.reads::<Children>()`, `.reads::<MeshHandle>()`
  - `.writes_resource::<PersistentReferenceStates>()` (and `FormIdPool` if `identity` reads it)

  Then add the system to `p2_gameplay_exclusives_declare_non_empty_access`'s type-level assertions.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D5-2026-09-28-01) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related systems / CI steps
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition and the `docs/engine/ecs.md` canonical order are preserved
- [ ] **TESTS**: A regression test pins this specific fix
