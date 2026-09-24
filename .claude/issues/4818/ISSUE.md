# #4818 — GAME-D2-2026-09-24-02: A picked-up item keeps its collision body — an invisible solid that blocks line of sight to the next item

**Labels**: medium,gameplay,inventory,physics,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: MEDIUM
- **Dimension**: 2 — Interaction / pickup
- **Location**:
  - `byroredux/src/inventory.rs:1138-1152`: `pickup_loot` marks only `mesh_entities_under`.
  - `byroredux/src/cell_loader/reference_state.rs:215-231`: restore marks the same set.
  - `byroredux/src/cell_loader/spawn.rs:1321-1392`: collision entities are standalone, carry `PhysicsSourceForm` and have no `Parent`.
  - `byroredux/src/interaction.rs:1013-1078`: the line-of-sight check.
- **Status**: NEW. The #4571 fix covers render only.
- **Description**:
  - `PickedUp` hides the mesh descendants, but the placement's colliders are not descendants and stay in Rapier.
  - `cast_ray` excludes only sensors and the player (`crates/physics/src/world.rs:1172`). `collider_belongs_to_target` accepts a hit only on the target's own source form, so the taken item's leftover collider blocks any candidate behind it.
  - A tombstoned placement respawns its colliders (`spawn.rs:716`) before the row is restored (`synth_child.rs:870`), so the ghost survives cell reloads.
  - No system despawns or disables colliders for `PickedUp`.
- **Impact**: every taken item leaves an invisible solid (dynamic for clutter). Items behind it in the aim line can't be selected until the player changes angle, and the ghost body can be bumped or pushed.
- **Trigger**: any game; take the front bottle of a shelf row, then aim at the one behind it.
- **Related**: #4571, #4695, #4697.
- **Suggested Fix**: on pickup and on tombstone restore, remove the collision entities whose `PhysicsSourceForm` matches the root's `FormIdComponent`, or make line of sight and physics skip them. Add a two-items-in-a-row selection test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
