# ECS-2026-09-21-D7-01: `PickedUp` lands on the placement root, but both render skips test only the mesh entity, so a picked-up or tombstone-restored item keeps drawing

**Labels**: medium, ecs, gameplay, inventory, renderer, bug

Filed via /audit-publish from docs/audits/AUDIT_ECS_2026-09-21.md.

**Severity**: MEDIUM · **Dimension**: 7 (Component Lifecycles: marker placement vs consumer granularity)
**Location**: `byroredux/src/inventory.rs` (`pickup_loot`, ~:913-915, producer); `byroredux/src/cell_loader/reference_state.rs` (`restore`, ~:221, re-stamp on respawn); consumers `byroredux/src/render/static_meshes.rs` (~:320 query, ~:415 skip) and `byroredux/src/render/skinned.rs` (`build_skinned_palettes`, ~:86 query, ~:104 skip)
**Verified against**: HEAD `f97775ca8`

## Description

`pickup_loot` inserts the P3 `PickedUp` marker on `target`, and `target` is always a **placement root**. Both render passes check the marker on the mesh or skinned entity itself and never on an ancestor. The marker therefore never reaches the only code that reads it for hiding.

- `pickup_loot` only accepts a target that passes `is_pickup_target`, and that function requires `SceneAliasCandidate`. For a plain item placement, only `stamp_quest_reference(world, placement_root, …)` inserts that component (`cell_loader/references/synth_child.rs:832`, the `is_primary_synth` branch after `spawn_placed_instances`).
- `spawn_placement_root` (`cell_loader/spawn.rs:898`) never inserts a `MeshHandle` on the root. Each mesh is its own entity carrying `Parent(placement_root)` and its own `MeshHandle` (`cell_loader/spawn/mesh_instance.rs:1118` / `:1134`). The fog-volume path at `:629` has the same shape.
- `reference_state::restore(world, placement_root)` (the P3 tombstone branch just below the stamp in `synth_child.rs`) re-inserts `PickedUp` on that same root, so a copy respawned after an eviction or reload is affected too.
- Both doc comments claim the opposite. `pickup_loot`'s says it will "hide the placement's meshes". `restore`'s says "The marker hides the meshes".
- The sibling marker `NpcAppearanceHidden` avoids this on purpose. `npc_spawn/loot_appearance.rs::set_hidden` walks the subtree through `mesh_entities_under` (cycle-safe). Its comment says the marker is "consumed by the render passes per mesh entity, so hiding a root means marking every mesh in its subtree".
- The #4536 lockstep guards do not cover the production path. Both `picked_up_placements_stay_hidden_even_when_animation_says_visible` (`render/static_mesh_fx_skip_tests.rs`) and `picked_up_body_gets_its_first_skin_upload_only_when_dropped` (`render/bone_palette_overflow_tests.rs`) insert `PickedUp` directly on the mesh entity. No test runs `pickup_loot` on a root that has a child mesh.

## Evidence

```rust
// byroredux/src/inventory.rs — pickup_loot; `target` is the SceneAliasCandidate carrier = placement root
if let Some(mut markers) = world.query_mut::<PickedUp>() {
    markers.insert(target, PickedUp);
}

// byroredux/src/render/static_meshes.rs — `entity` is the MeshHandle entity, a child of that root
|| picked_up.as_ref().is_some_and(|q| q.get(entity).is_some())

// byroredux/src/render/skinned.rs — `entity` is the SkinnedMesh entity
|| picked_up.as_ref().is_some_and(|q| q.get(entity).is_some())
```

`grep -rn PickedUp byroredux/src` finds no other consumer. Neither pass looks at `Parent`.

## Impact

- After `script.activate <root>`, or a scripted `Activate()` on a loose item, the item goes into the inventory and interaction ignores it, because `is_pickup_target` checks the root. Its meshes stay in the raster pass and the TLAS, and its colliders stay as well.
- After an eviction or reload, the tombstone re-stamps the new root and the item reappears the same way.
- Reachability is limited today. Loose items are not E-key interaction candidates: `interaction.rs::populate_candidates` has no pickup arm, and the only `pickup_loot` caller is `container_loot_system`, which is driven by `ActivateEvent`. That is why this is MEDIUM rather than HIGH.

## Related

- #4536 (closed; REN-D9-2026-09-20-02, `AUDIT_RENDERER_2026-09-20.md`) added the mesh-level lockstep tests and rated the skip "sound". Its fix is still in place. This is a different defect (marker granularity), not a regression of it.
- #3319 (closed) is the same "marker that silently does nothing" class.
- ECS-2026-09-21-D5-02 (#4574): `container_loot_system`'s declared access also leaves out this `PickedUp` write.
- Owner overlap: `/audit-gameplay` (pickup) and `/audit-renderer` (skip sites).

## Suggested Fix

Stamp the render-hiding marker on every mesh under the root, as `set_hidden` does for `NpcAppearanceHidden`. Reuse `mesh_entities_under` rather than adding a second walk. Do this in both `pickup_loot` and `reference_state::restore`, and keep `PickedUp` on the root for interaction and capture. Add a test that runs `pickup_loot` on a root with a child mesh and asserts that `build_render_data` drops the mesh in both the static and skinned passes.

The two call sites need different mechanics. `pickup_loot` takes `&World`, so its per-mesh inserts must go through the pre-registered `query_mut` guard, the `boot/world.rs` pattern: collect the subtree first, then open the guard. `restore` takes `&mut World` and can insert directly.

Source: docs/audits/AUDIT_ECS_2026-09-21.md (ECS-2026-09-21-D7-01)

## Completeness Checks
- [ ] **SIBLING**: Every static and skinned mesh under the root is hidden. The `reference_state::restore` re-stamp path gets the same fix. Other root-level markers that are read per mesh entity have been checked.
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (collect the subtree before opening the marker write guard)
- [ ] **TESTS**: A regression test runs `pickup_loot` on a placement root with child meshes and asserts both render passes drop them. A second test covers the tombstone `restore` path.
