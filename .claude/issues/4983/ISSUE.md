# #4983 — ECS-2026-09-28-D1-02: `pickup_loot` / `restore` hold the `PickedUp` write guard across `mesh_entities_under`, closing `PickedUp → Children → GlobalTransform → PickedUp` against the render skips

Filed 2026-09-28 via `/audit-publish docs/audits/AUDIT_ECS_2026-09-28.md`. Snapshot as filed; GitHub is authoritative for live state.

**Source**: `docs/audits/AUDIT_ECS_2026-09-28.md` (HEAD `21319618c`)

- **Severity**: HIGH (floor: ECS deadlock potential). Latent under today's schedule. Every
  participant is a `&World` path, so a parallel promotion of `container_loot_system`, or of any
  system that walks `Children` under `GlobalTransform`, makes the cycle live.
- **Dimension**: 1 — Lock Ordering
- **Location**:
  - `byroredux/src/inventory.rs:1138-1150` (`pickup_loot`, called from the Update exclusive
    `container_loot_system`)
  - `byroredux/src/cell_loader/reference_state.rs:253-259` (`restore`, the same shape)
  - The walk itself is `byroredux/src/npc_spawn/loot_appearance.rs:105-121` (`Children` +
    `MeshHandle`).
- **Status**: NEW. Introduced by `dcdfafa15` "Fix #4571" (2026-09-22). It is the cycle that turned
  the lane red that day: run 35744024504 on `ee6d3fb39` reports only this cycle.
- **Description**: The #4571 fix, a finding from this audit's previous run, stamps `PickedUp` on the
  subtree's mesh entities. It does this inside `if let Some(mut markers) =
  world.query_mut::<PickedUp>()`, calling `mesh_entities_under(world, target)` from inside that
  block. That records `PickedUp → Children` and `PickedUp → MeshHandle`.

  The two render skips run the other way. `build_skinned_palettes` (`render/skinned.rs:81-86`) takes
  `GlobalTransform` → `SkinnedMesh` → `NpcAppearanceHidden` → `PickedUp`, and `static_meshes.rs`
  mirrors it. Transform propagation records `Children → GlobalTransform`
  (`crates/core/src/ecs/systems.rs:109`). Together these give `PickedUp → Children →
  GlobalTransform → PickedUp`.
- **Evidence**:
  ```text
  render::bone_palette_overflow_tests::picked_up_body_gets_its_first_skin_upload_only_when_dropped:
    attempted acquisition of `PickedUp` while holding `GlobalTransform` … `PickedUp` → `Children` →
    `GlobalTransform` → `SkinnedMesh` → `PickedUp`     @ render/skinned.rs:86
  AUDIT_EDGE PickedUp -> Children @@ npc_spawn/loot_appearance.rs:113 | inventory.rs:1640
  ```
  The recorded edge comes from `pickup_stamps_the_subtree_meshes_not_just_the_root`
  (`inventory.rs:1640`). That test reproduces the production block verbatim, so production records
  the same edge on the first real pickup.
- **Impact**: 2 of the 27 panics
  (`picked_up_body_gets_its_first_skin_upload_only_when_dropped` and
  `picked_up_placements_stay_hidden_even_when_animation_says_visible`). It is also a latent real
  deadlock between `&World` systems, as described under Severity.
- **Related**: #4571 (the change that introduced it), D1-01, #4546 (the same shape: a marker
  consumer ordered after the hierarchy cluster).
- **Suggested Fix**: Compute `let meshes = mesh_entities_under(world, target);` *before* taking
  `query_mut::<PickedUp>()`, then insert the root and `meshes` under the one write guard. Apply the
  same fix in `reference_state::restore`, and update the test fixture at `inventory.rs:1638-1643`
  to match. `PickedUp` then stays a pure sink after the hierarchy/skin cluster, which is where
  `docs/engine/ecs.md` § Canonical acquisition order places cinematic read-onlys.

## Completeness Checks
- [ ] **LOCK_ORDER**: `mesh_entities_under` runs before `query_mut::<PickedUp>()`; `PickedUp` is acquired as a sink after the hierarchy/skin cluster
- [ ] **SIBLING**: Both producers fixed — `pickup_loot` (`inventory.rs`) and `reference_state::restore` — plus the test fixture in `pickup_stamps_the_subtree_meshes_not_just_the_root`; `NpcAppearanceHidden` stamping in `loot_appearance.rs` checked for the same shape
- [ ] **DOC**: `docs/engine/ecs.md` § Canonical acquisition order names where `PickedUp` / `NpcAppearanceHidden` sit
- [ ] **TESTS**: `picked_up_body_gets_its_first_skin_upload_only_when_dropped` and `picked_up_placements_stay_hidden_even_when_animation_says_visible` pass under `BYRO_LOCK_ORDER_CHECK=1`
