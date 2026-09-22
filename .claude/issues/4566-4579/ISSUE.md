# 4566: NIFAL-D9-2026-09-21-01: assert_pbr_override_ceiling covers only the two Skyrim lanes — the other six games cannot see upward drift

State: OPEN  Labels: ['bug', 'low', 'nifal', 'test-gap']

**Severity**: LOW · **Dimension**: Completeness (translation-completeness signal) · **Tier Violated**: harness-gap · **Game Affected**: Oblivion, FO3, FNV, FO4, FO76, Starfield
**Location**: `crates/nif/tests/translation_completeness.rs:578` and `:617` (the only two `assert_pbr_override_ceiling` call sites)
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
#4393's lesson, recorded in `assert_pbr_override_ceiling`'s own doc (:236-240), is that a floor alone cannot see upward drift — a parser placeholder counted as a classifier signal. The ceiling was added only where #4393 happened to fire (SkyrimLE/SkyrimSE, both 97.0). Every other game asserts floors only, so the exact regression class the guard exists for would keep all six remaining games green. The low-fill BGSM-era rows are the most exposed: FO76's floor is 8.0% and Starfield's 1.0% (measured 5.1% on 2026-09-21) — a placeholder that drifted Starfield's metO to 50% would pass. FO4 measured 99.4% with no ceiling (legitimate per the union-of-signals classifier analysis, but nothing pins that).

### Evidence
Ceiling call sites at `:578`/`:617` only (third grep hit is the fn definition).

### Impact
A repeat of #4393 on any non-Skyrim game is invisible to the harness until re-measured by hand.

### Related
#4393, #4250, #2707

### Suggested Fix
Add a ceiling per game ~10pp above each current measured value (Oblivion/FO3/FNV ≈ 99-100 or a documented skip, FO4 ≈ 100, FO76 ≈ 20, Starfield ≈ 10), mirroring the floor margins' philosophy.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix


---

# 4571: ECS-2026-09-21-D7-01: `PickedUp` lands on the placement root, but both render skips test only the mesh entity, so a picked-up or tombstone-restored item keeps drawing

State: OPEN  Labels: ['bug', 'ecs', 'renderer', 'medium', 'gameplay', 'inventory']

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


---

# 4572: ECS-2026-09-21-D3-01: the new `apply_placed_water_type` subtree walk skips both cycle guards, and three older walks have the same shape

State: OPEN  Labels: ['bug', 'ecs', 'low']

**Severity**: LOW (defence in depth; no live code path builds a cycle) · **Dimension**: 3 (hierarchy walks: `HierarchyTraversalGuard` rule)
**Location**: `byroredux/src/cell_loader/water.rs` (`apply_placed_water_type`, ~:731-749; new in `ec9ed8524`, 2026-09-14). Siblings: `byroredux/src/ragdoll.rs` (`ragdoll_writeback_system`: the #1981 `mesh_walk_queue` walk ~:629-653 and the #1979 descendant re-derive ~:673-718, both run every frame while a ragdoll is active) and `byroredux/src/anim_convert.rs` (`build_subtree_name_map`, ~:21-54)
**Verified against**: HEAD `f97775ca8`

## Description

The audit-ecs rule reads: "A new hierarchy walk without it, or without a visited set, is an unbounded-loop hazard". #3700's fix bounded the three core walks with `HierarchyTraversalGuard`: `make_transform_propagation_system` in `crates/core/src/ecs/systems.rs`, and the parent climb and post-order walk in `byroredux/src/systems/bounds.rs`. Four walks outside that set still have neither a guard nor a visited set:

1. `apply_placed_water_type`, the Skyrim placed river/stream activator path, called from `cell_loader/references/synth_child.rs` after `spawn_placed_instances`. Its walk is `let mut stack = vec![placement_root]; while let Some(entity) = stack.pop() { … stack.extend(kids.0.iter().copied()); }`.
2. `ragdoll_writeback_system`'s #1981 `LocalBound` BFS (`mesh_walk_queue`) over the actor's subtree.
3. `ragdoll_writeback_system`'s #1979 descendant re-derive BFS (`queue`), seeded from every body bone.
4. `build_subtree_name_map`'s DFS from an animation root.

By contrast, `npc_spawn/loot_appearance.rs::mesh_entities_under`, added two days after the water walk, does keep a visited set.

Save load still does no acyclicity check. `crates/save/src/validate.rs::validate_hierarchy` checks only that `Parent` and `Children` agree, plus dangling ids, so a bidirectionally consistent cycle passes. A corrupt or hand-edited save is still a cycle source for the per-frame ragdoll walks.

## Evidence

```rust
// byroredux/src/cell_loader/water.rs — apply_placed_water_type
let mut stack = vec![placement_root];
while let Some(entity) = stack.pop() {
    if let Some(plane) = planes.get(entity) { /* … */ found.push((entity, *plane, flow)); }
    if let Some(kids) = children.as_ref().and_then(|q| q.get(entity)) {
        stack.extend(kids.0.iter().copied());
    }
}

// byroredux/src/ragdoll.rs — ragdoll_writeback_system, #1979 pass (the #1981 pass has the same shape)
while let Some(entity) = queue.pop_front() {
    // …
    if let Some(children) = cq.get(entity) {
        queue.extend(children.0.iter().copied());
    }
}
```

`grep -rn HierarchyTraversalGuard` finds users only in `crates/core/src/ecs/systems.rs` and `byroredux/src/systems/bounds.rs`.

## Impact

On a cyclic `Children` graph, each of these walks loops forever and its stack or queue grows until OOM, with no diagnostic. The two ragdoll walks run every frame while a ragdoll is active, over hierarchies that may come from a save. A duplicated `Children` edge makes the walk visit the subtree again. In the water case it also pushes duplicate `(entity, WaterPlane)` targets, and each duplicate gets its own texture resolve and insert. The water walk runs over a freshly spawned subtree, so its hazard is theoretical.

## Related

- #3700 (closed) bounded the three core walks, and those fixes are still in place. These four walks were outside its scope, so this is not a regression of it.
- Correctly bounded precedents: the `cell_loader/unload.rs` retained-set walk, the capped console parent climbs (`commands/scene.rs`, `commands/assets.rs`), and `loot_appearance::mesh_entities_under`.

## Suggested Fix

Bound all four walks with `HierarchyTraversalGuard::new(world.next_entity_id() as usize, child_refs)` or a visited set. Follow the `bounds.rs` pattern: when the guard runs out, `log::error!` and bail. Optionally, add an acyclicity check to `validate_hierarchy`, so that a corrupt save is rejected at load time instead of hanging a later frame.

Source: docs/audits/AUDIT_ECS_2026-09-21.md (ECS-2026-09-21-D3-01)

## Completeness Checks
- [ ] **SIBLING**: All four walks are bounded. A grep for other `Children`-following `extend(` loops outside the guarded set finds none left unbounded.
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test builds a `Parent`/`Children` cycle and asserts that each walk terminates with a diagnostic.


---

# 4573: ECS-2026-09-21-D5-01: `every_parallel_system_declares_everything_it_acquires` ignores read vs write and matches names by substring

State: OPEN  Labels: ['bug', 'ecs', 'low', 'concurrency', 'test-gap']

**Severity**: LOW (test gap; the live schedule is clean) · **Dimension**: 5 (declared-access guard)
**Location**: `byroredux/src/boot/schedule/mod.rs`: `acquired_in` (~:616-640, where `types.push(short)` at ~:638 records the short type name only) and `assert_declares_everything_it_acquires` (~:702, where `declared.contains(*ty)` runs over the raw block text returned by `declaration`)
**Verified against**: HEAD `f97775ca8`

## Description

`every_parallel_system_declares_everything_it_acquires` is the mechanical backing for the boot RELEASE assertion `known_conflict_count() == 0` in `install_runtime_registries`. It compares each parallel system's acquisitions against its `add_to_with_access(...)` registration block, and it has three blind spots:

- **Mode.** `acquired_in` records type names with no read/write mode, and the comparison only checks that the name appears somewhere in the registration block. So a parallel system that takes `query_mut::<T>` or `resource_mut::<T>` while declaring only `.reads::<T>()` passes. `analyze_pair` then treats a real write as a read, and a reader in the same stage pairs with it as non-conflicting.
- **Substring.** `declared.contains(ty)` is a plain `str::contains`. An acquisition of `Transform` is satisfied by a block that declares only `GlobalTransform`, and the same holds for any type name that appears inside another declared name.
- **Comments.** `declaration()` returns the whole balanced-paren block, comment lines included. Take the skill's own precedent, `ac1d44f5c`: if `early.rs` dropped only `.writes::<GlobalTransform>()` from the `fly_camera_system` registration, the adjacent comment "`fly_camera_system` publishes its pose to `GlobalTransform`" would keep the test green.

The known forms gap is already documented in the audit-ecs skill: `world.get::<T>` / `get_mut` / `has`, `query_2_mut`, `resource_2_mut`, and acquisitions with no turbofish. These three blind spots are not.

## Evidence

```rust
// acquired_in — no mode recorded
let short = path.rsplit("::").next().unwrap_or(path).to_owned();
if !types.contains(&short) {
    types.push(short);
}

// assert_declares_everything_it_acquires — substring test over raw block text, comments included
let declared = declaration(system);
let missing: Vec<&String> = types.iter().filter(|ty| !declared.contains(*ty)).collect();
```

The audit also ran its own mode-aware version of the check. That version matches exact names, compares read/write, covers the `get`/`has`/`query_2_mut`/`resource_2_mut` forms, and follows same-file callees to depth 3. It finds **0 undeclared and 0 write-declared-as-read** acquisitions across all 9 parallel systems at HEAD.

## Impact

Nothing breaks today. But the next write-for-read slip in a parallel body will ship green, and the boot soundness proof is then silently void.

## Related

- #4064 (closed) was about which systems the table covers, not how the comparison works. That fix is still in place.
- #4404 (closed) was the same comment/substring blind spot in the NIFAL particle completeness guards.
- The same helper backs `papyrus_provider_system_declares_everything_it_acquires` and `legacy_obscript_load_order_system_declares_everything_it_acquires` (#3951), so this fix tightens those tests as well.
- ECS-2026-09-21-D5-02 (#4574) would extend this helper to the P2 exclusives, so fix these holes first.
- ECS-2026-09-21-D2-01 (#4575): the audit-ecs skill's "What the guard cannot see" list omits these three blind spots.
- `docs/audits/AUDIT_CONCURRENCY_2026-09-21.md` cites this finding without re-reporting it.

## Suggested Fix

1. Make `acquired_in` return `(type, mode)`.
2. Strip comments from the registration block, then parse its `.reads` / `.writes` / `.reads_resource` / `.writes_resource::<…>` calls into exact short-name read and write sets.
3. Require every write-mode acquisition to appear in the write set, and every read-mode acquisition to appear in either set.
4. Add negative self-tests, each of which must fail:
   - a body that writes `T` against a block that only reads `T`;
   - a body that acquires `Transform` against a block that only declares `GlobalTransform`;
   - a block whose only mention of the type is in a comment.

Source: docs/audits/AUDIT_ECS_2026-09-21.md (ECS-2026-09-21-D5-01)

## Completeness Checks
- [ ] **SIBLING**: The two exclusive completeness tests that share `assert_declares_everything_it_acquires` (#3951) still pass under the stricter comparison.
- [ ] **TESTS**: Negative self-tests pin all three blind spots (mode, substring, comment text) so the guard cannot silently regress to name-presence matching.


---

# 4574: ECS-2026-09-21-D5-02: declared `Access` rows of five exclusive systems omit types their own bodies acquire

State: OPEN  Labels: ['bug', 'ecs', 'low', 'gameplay', 'combat', 'concurrency']

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


---

# 4575: ECS-2026-09-21-D2-01: `audit-ecs/SKILL.md` drift found while running it

State: OPEN  Labels: ['documentation', 'low', 'tech-debt', 'doc-rot']

**Severity**: LOW (audit tooling) · **Dimension**: 2 / 5 (skill text)
**Location**: `.claude/commands/audit-ecs/SKILL.md`: the Dim 2 "Change tracking" bullet (~:68) and the Dim 5 "What the guard cannot see" paragraph (~:161)
**Verified against**: HEAD `f97775ca8`

## Description

Two statements in the audit-ecs skill no longer match the code:

1. **Dim 2, "Change tracking"**, says `TRACK_CHANGES` is on for `Transform`, `GlobalTransform`, `Parent` and `Children`. It is also on for three more components. All three use sparse storage, so they get only a `structural_generation` bump and no dirty set:
   - `LocalBound` (`crates/core/src/ecs/components/local_bound.rs`), since `ad012f9d6` (2026-05-31);
   - `Material` (`crates/core/src/ecs/components/material.rs`) and `ParticleEmitter` (`crates/core/src/ecs/components/particle.rs`), both since `1d56758ba` (#3836, 2026-09-05).
2. **Dim 5, "What the guard cannot see"**, lists the forms gap: `world.get` / `get_mut` / `has`, `query_2_mut`, `resource_2_mut`, acquisitions without a turbofish, cross-file hops, closures and macros. It leaves out the three blind spots found in ECS-2026-09-21-D5-01 (#4573):
   - read vs write is never compared;
   - type names are matched by substring;
   - comment text inside the registration block satisfies the check.

## Evidence

`grep -rn 'const TRACK_CHANGES: bool = true' crates/core/src/ecs/components/` finds seven components: `transform`, `global_transform`, `hierarchy` ×2 (`Parent`, `Children`), `local_bound`, `particle` and `material`. The skill names four of them.

## Impact

- An auditor following Dim 2 skips the change-tracking contract of three components that feed incremental paths: #3836's `SceneEffectSoftCache` and the incremental world-bound propagation.
- An auditor following Dim 5 trusts the completeness guard for exactly the slips it cannot catch.

## Related

- ECS-2026-09-21-D5-01 (#4573).
- Earlier audit-ecs skill drift, all closed: #4065, #4174, #3035.
- `_audit-validate.sh` reports no stale path and no advisory against this skill. This drift is semantic, which the path gate cannot catch.

## Suggested Fix

1. In Dim 2, list all seven `TRACK_CHANGES` components, noting which are packed (dirty set) and which are sparse (generation bump only). Better still, replace the list with the grep so it cannot rot.
2. In Dim 5, add the three D5-01 blind spots. If D5-01 is fixed first, record them as closed instead.
3. Run `.claude/commands/_audit-validate.sh` after the edit.

Source: docs/audits/AUDIT_ECS_2026-09-21.md (ECS-2026-09-21-D2-01)

## Completeness Checks
- [ ] **SIBLING**: Other skills and `docs/engine/` pages that list `TRACK_CHANGES` components or describe the declaration-completeness guard are updated in the same pass. At filing, a grep found no other list of tracked components.


---

# 4576: REN-D1-2026-09-21-01: removing the blend→EFFECT divert makes FO3/FNV/Oblivion blended FX cards (mist, ground fog, light beams, glow shells) binary shadow blockers

State: OPEN  Labels: ['bug', 'renderer', 'high', 'vulkan', 'game:fnv', 'game:fo3', 'game:oblivion']

**Severity**: HIGH (rendering correctness on the default path in three games; per-cell magnitude not yet captured live) · **Dimension**: AS Correctness (instance mask)
**Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs` `shadow_mask_for_instance` (~:976-1028; the blend arm is gone and `_alpha_blend` is unused), called from `acceleration/tlas.rs` (~:649). Consumers: `shaders/include/shadow_transport.glsl` `traceShadowTransmittanceDetailed`, `shaders/include/ray_hit.glsl` `rayHitHasCoverage` (~:438), `shaders/include/shadow_common.glsl` `traceShadowBinary` (volumetrics_inject / caustic_splat), `shaders/volumetrics_inject.comp` `combustionPathBlocked` (`VISIBILITY_MASK_SOLID`, ~:1719-1737)
**Status**: NEW (introduced by `f97775ca8`, 2026-09-21)
**Verified against**: HEAD `f97775ca8`

## Description

Until `f97775ca8`, every non-actor alpha-blended instance was routed to `VISIBILITY_LAYER_EFFECT` (32), which is outside `VISIBILITY_MASK_ALL_OPAQUE` (15) and `VISIBILITY_MASK_SOLID` (31). The commit removed that divert as part of the single-sided-wall light-leak fix (Bleak Falls Barrow ice and door panels), so a blended non-actor now keeps its render layer's opaque bucket. The new comment gives the premise:

> True non-occluders are separated by MATERIAL KIND above (effect shader, fire refraction, refractive glass) — the authored "not solid" signal; blend state is not.

That holds for Skyrim+, whose effect family imports as `MATERIAL_KIND_EFFECT_SHADER` (101). It does not hold for:
- **FO3/FNV.** The effect family is `BSShaderNoLightingProperty`, which imports as `MATERIAL_KIND_NO_LIGHTING` (102) (`crates/nif/src/import/material/legacy_properties.rs`, pinned by `nolighting_sets_material_kind_to_102`). Kind 102 is not in the non-occluder arm.
- **Oblivion.** FX cards import as kind 0.

TLAS membership (`byroredux/src/render/static_meshes.rs` `tlas_exclusion`) excludes only distant LOD blocks, `IsDecalMesh` and fire refraction. These cards are therefore in the TLAS, and now sit in the `ARCHITECTURE` / `STATIC_PROP` / `FOLIAGE` buckets.

## Evidence

- **Code (HEAD).** `shadow_mask_for_instance` tests glass → `EFFECT_SHADER`/`FIRE_REFRACTION` → actor → `match render_layer`. It has no kind-102 arm and no blend arm, and `_alpha_blend` is unused.
- **Coverage.** `rayHitHasCoverage` treats a blended non-glass hit as covered at alpha ≥ 1/255. Without `INSTANCE_FLAG_DIFFUSE_ALPHA` and with `alphaThreshold == 0`, alpha is forced to 1.0, so soft fog and beam textures are effectively solid quads. `traceShadowBinary` (`gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT`) has no coverage test at all.
- **Real-data census.** The audit ran a read-only scratch tool over the vanilla `Meshes` BSAs. It counted drawn meshes that are alpha-blended, not glass-keyword, not kind 100/101/103, and not a never-drawn `IsFxMesh` texture:
  - **FNV: 4,699** (kind 102: 3,777). Examples: `effects\nv\hchambergroundfog.nif`, `effects\ambient\fxmistlow01.nif`, `architecture\strip\lucky38lights.nif`, `ultraluxdome_glowsbackside.nif`.
  - **FO3: 2,119** (kind 102: 1,613). Examples: `clutter\fakefog01.nif`, `effects\ppurityfx\ppurityfxtankfog01.nif`.
  - **Oblivion: 1,011** (kind 0). Examples: `dungeons\misc\fx\fxlightbeam01.nif`, `oblivion\environment\fxoblivionlightbeam01.nif`, `oblivion\gate\flashglow01.nif`.
  - Nearly all are additive `SRC_ALPHA/ONE` or soft-alpha cards.
- **Lights affected.** FO3/FNV ESM lights are all `FULL` (the zero-authoring correction), as are NIF lights and the sun/XCLL. Oblivion's unflagged lights use the conservative mask, which gained `STATIC_PROP` in `d54382415`.

## Impact

- Ground-fog and mist planes shadow the floor beneath them from every overhead light.
- Light-beam and glow shells block the lights they depict and cast hard quad shadows.
- Volumetric in-scatter is cut off at each card (`traceShadowBinary`).
- Smoke and fire transport stops at mist and beam cards. `combustionPathBlocked` traces `VISIBILITY_MASK_SOLID`, so the comment above it ("effect cards do neither") is now false for these cards.
- `rt.masks` cannot show any of this, because blended cards are now counted inside the opaque buckets.
- The per-cell magnitude has not been captured live (see the report's "Needs-RenderDoc / live validation").

## Related

- #3305 (open): actor ground-contact shadows, the opposite direction of the same mask policy.
- `84bbc44ed`: the blended-actor carve-out, which `f97775ca8` keeps.
- REN-D1-2026-09-21-02 (#4580): `TRIANGLE_FACING_CULL_DISABLE` is inert. The `f97775ca8` investigation relied on the opposite premise.
- REN-D8-2026-09-21-02 (#4589): the water-caustic visibility ray's mask. Now that legacy FX cards sit in opaque buckets, no mask choice can exclude them.
- Cited, not re-reported, by `docs/audits/AUDIT_PERFORMANCE_2026-09-21.md`.

## Suggested Fix

Keep the Skyrim ice/door-panel intent, and add the legacy authored "not solid" signals to the non-occluder arm: `MATERIAL_KIND_NO_LIGHTING` (102) and additive blends (`dst_blend == ONE`). The durable version is an explicit canonical non-occluder bit carried across the NIFAL boundary, so the renderer does not re-derive it from per-game kinds. Also:
- add an FNV fixture and an Oblivion fixture to `shadow_mask_bucket_selection_is_pinned`;
- add a blended-in-opaque-bucket counter to `rt.masks`.

Validate with an A/B pre/post `f97775ca8` (`rt.masks` + screenshots) on one cell per game: FNV Lucky 38, FO3 Project Purity, and an Oblivion Ayleid beam dungeon. Include Bleak Falls Barrow, the case that motivated the commit.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D1-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: `mask_divert_cause` and the `rt.masks` census partition stay in lockstep with the new arm (`divert_cause_matches_the_mask_it_explains`)
- [ ] **SIBLING**: every mask consumer re-checked — `traceShadowTransmittanceDetailed`, `traceShadowBinary` (volumetrics_inject / caustic_splat), `combustionPathBlocked`, and `water.frag`'s caustic visibility ray
- [ ] **CANONICAL-BOUNDARY**: if a non-occluder bit is added, it is derived once at the NIFAL parser→`Material` boundary (`translate_material`), never re-derived per game in the renderer
- [ ] **TESTS**: FNV (kind-102 additive card) and Oblivion (kind-0 blended card) fixtures in `shadow_mask_bucket_selection_is_pinned`; the Bleak Falls Barrow ice-panel case stays in its opaque bucket


---

# 4577: REN-D12-2026-09-21-01: `composite.frag.spv` is stale on two generated constants — both new debug views render full-frame magenta and the `DBG_VIZ_AO` oracle is contaminated (regression of #3322)

State: OPEN  Labels: ['bug', 'renderer', 'medium', 'shaders', 'test-gap']

**Severity**: MEDIUM (debug surface; same grade as #3322) · **Dimension**: Debug/Telemetry
**Location**: `crates/renderer/shaders/composite.frag.spv` vs `crates/renderer/shaders/composite.frag`: the `debugMode > RENDER_DEBUG_MODE_MAX` guard (~:418-423) and `DBG_VIZ_REQUIRES_RAW_OUTPUT(dbgFlags)` (~:430), both fed by the generated `crates/renderer/shaders/include/shader_constants.glsl`
**Status**: Regression of #3322 (closed 2026-08-27)
**Verified against**: HEAD `f97775ca8`

## Description

The committed `composite.frag.spv` was last rebuilt in `0e12f1f10` (2026-09-20). Two later commits changed generated constants that composite compiles in, and both recompiled only `ssao` and `triangle`:
- `09d9bc6f8` added `DBG_VIZ_AO` (0x200) to `DBG_VIZ_RAW_OUTPUT_ANY_MASK` (2215116800 → 2215117312).
- `f97775ca8` added `RENDER_DEBUG_FACING_RATIO` (14) and `RENDER_DEBUG_RESTIR_LIGHT` (15), bumping `RENDER_DEBUG_MODE_MAX` from 13 to 15.

#3322 fixed the same failure: a stale composite binary renders a debug view magenta. That fix was a recompile with no composite SPIR-V pin, and the triangle sibling's pin (#3120) does not cover composite.

## Evidence

- `spirv-dis` of the committed binary at HEAD:
  ```
  %uint_2215116800 = OpConstant %uint 2215116800
  %2858 = OpUGreaterThan %bool %2857 %uint_13
  %2874 = OpBitwiseAnd %uint %2872 %uint_2215116800
  ```
  The source constants are `RENDER_DEBUG_MODE_MAX 15u` and `DBG_VIZ_RAW_OUTPUT_ANY_MASK 2215117312u`.
- The audit ran `scripts/check-shader-artifacts.sh` with the matching glslang (11:16.2.0). `composite.frag.spv` is the only drifting artifact of 35, and a fresh compile differs in exactly these three instructions.
- CI: the **Shader source/artifact parity** job (`shader-artifacts` in `.github/workflows/ci.yml`) has reported `DRIFT crates/renderer/shaders/composite.frag.spv` on every main run from `42725fdcc` through `f97775ca8`. `42725fdcc` is the first run that contains `09d9bc6f8`.

## Impact

- **Both new views render magenta.** `render.debug facing` and `render.debug restir` (modes 14/15) trip composite's out-of-range guard. The host's `render_debug_requires_raw_output` routes every non-Final mode raw past bloom, TAA, FSR and presentation, so the magenta frame reaches the screen. The two views added for the single-sided-wall light-leak hunt are unusable.
- **The `DBG_VIZ_AO` oracle is contaminated.** In the legacy-flags path, the stale composite treats `DBG_VIZ_AO` as non-raw. It composites caustics (into direct), fog, volumetric transmittance and sky over the raw AO image, so the oracle is clean only in fog-free, caustic-free scenes (the Cornell box it was tuned on).
- **CI signal is lost.** The parity job has been red on main since the drift, which masks any further SPIR-V drift.

## Related

- #3322 (closed): the same stale-composite failure. This is its recurrence.
- #3120 (closed): the `triangle.frag.spv` sibling. That one is pinned by `triangle_frag_spv_debug_mode_guard_matches_render_debug_mode_max` (`crates/renderer/src/vulkan/reflect.rs`).
- #1917 (closed): an earlier stale-`composite.frag.spv` instance.
- REN-D2-2026-09-21-01 (#4582) and REN-D2-2026-09-21-02 (#4583): defects in the two new views themselves, which become visible once this is fixed.

## Suggested Fix

1. Recompile `composite.frag.spv` (plain `glslangValidator -V`, glslang 11:16.2.0) and confirm `scripts/check-shader-artifacts.sh` passes.
2. Extend the `max_u_greater_than_rhs_constant` pin in `triangle_frag_spv_debug_mode_guard_matches_render_debug_mode_max` to `composite.frag.spv`.
3. Add an `OpConstant == DBG_VIZ_RAW_OUTPUT_ANY_MASK` presence pin for every shader that uses `DBG_VIZ_REQUIRES_RAW_OUTPUT` (`composite.frag`, `presentation.frag`, `triangle.frag`).

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D12-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every shader that compiles `RENDER_DEBUG_MODE_MAX` or `DBG_VIZ_REQUIRES_RAW_OUTPUT` (`composite.frag`, `presentation.frag`, `triangle.frag`) is byte-fresh; `scripts/check-shader-artifacts.sh` is green
- [ ] **TESTS**: a SPIR-V constant pin for `composite.frag.spv` (debug-mode guard + raw-output mask), so a generated-constant bump without a recompile fails `cargo test`, not only CI's parity job


---

# 4578: REN-D11-2026-09-21-01: AgX clamps linear input to [0,1] before `log2` — highlights ≥ ~1.0 flatten to 0.59 display-linear, white is unreachable, black takes a `log2(0)` → NaN path

State: OPEN  Labels: ['bug', 'renderer', 'medium', 'shaders', 'test-gap']

**Severity**: MEDIUM (opt-in display transform visibly wrong; the default ACES path is unaffected) · **Dimension**: FSR/Presentation
**Location**: `crates/renderer/shaders/presentation.frag` `agx()` (~:99-112); Rust mirror `crates/renderer/src/tonemap.rs` `agx()` (~:113) and its tests `saturated_channels_are_monotonic` / `outputs_are_bounded_and_black_maps_to_black`
**Status**: NEW (introduced by `c5663fe39`)
**Verified against**: HEAD `f97775ca8`

## Description

`presentation.frag` `agx()` runs:

```glsl
val = AGX_MAT * val;
val = clamp(val, 0.0, 1.0);   // linear clamp
val = log2(val);
val = (val - min_ev) / (max_ev - min_ev);
```

Minimal AgX (Wrensch 2023), the implementation the file cites, clamps in log space instead: `val = clamp(log2(val), min_ev, max_ev)`. three.js's port also clamps after the log encode. Because the linear value is clamped at 1.0, `log2` never exceeds 0. The `max_ev = 4.026069` headroom (about 4 stops above 1.0) is never used.

The low end is broken too. An exactly-zero channel (black, or a negative input clamped to 0) gives `log2(0) = -inf`. The contrast polynomial then evaluates inf − inf = NaN, and the outset matrix carries it. The only rescue is `max(val, 0.0)`, which lowers to GLSL.std.450 `FMax`, whose result is undefined for a NaN operand. The image-health counter runs before tone mapping and cannot see this.

## Evidence

- Grey ramp evaluated with the shader's own constants (display-linear output):

  | Input | Repo | Reference (log-space clamp) |
  |---|---|---|
  | 0.5 | 0.425 | 0.425 |
  | 1.0 | 0.590 | 0.590 |
  | 1.5 | 0.590 | 0.683 |
  | 2.0 | 0.590 | 0.743 |
  | 4.0 | 0.590 | 0.861 |
  | 16 | 0.590 | 0.995 |

- The Rust mirror `tonemap.rs` `agx()` copies the linear clamp; its doc says "The input clamp to `[0, 1]` matches the GLSL". The comment on `saturated_channels_are_monotonic` calls the clip "reference behaviour".
- Rust's `f32::max(NaN, 0.0)` returns 0.0, so `outputs_are_bounded_and_black_maps_to_black` passes on the Rust side and cannot see the GLSL NaN path.
- `shaders_pin_the_mirror_constants` pins the constants, not the order of operations. The tests are circular: they encode the deviation, not the reference.

## Impact

With `--tonemap agx` or the `tonemap agx` console command, every surface, light and sky texel at or above ~1.0 linear clips to a flat ~0.59 display-linear (~79 % sRGB grey). That defeats the highlight roll-off Stage 1 chose AgX for. Black-pixel output depends on the driver (NaN through `FMax`/`FClamp`). The default ACES path is unaffected.

## Related

- SAFE-D7-2026-09-21-01 (`docs/audits/AUDIT_SAFETY_2026-09-21.md`): the exposure meter's averaging bug. It is a separate defect with its own issue.
- REN-D3-2026-09-21-01 (#4584): the operator id is a hand-written `1u` in the same `tonemap()` dispatch.
- Cited, not re-reported, by `docs/audits/AUDIT_PERFORMANCE_2026-09-21.md`.

## Suggested Fix

- Use the reference log-space clamp, with a small floor before the log: `val = clamp(log2(max(val, 1e-10)), min_ev, max_ev)`.
- Mirror it in `tonemap.rs`.
- Replace the circular tests with two new ones: the grey ramp keeps rising past 1.0 and reaches ≥ 0.99 at 16; an exact-zero input yields a finite value, checked before any `max` so the Rust side cannot mask a NaN.

Do this before any Stage-1 AgX oracle is minted against the current curve.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D11-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: `tonemap.rs` `agx()` mirrors the shader's new order of operations (log-space clamp), not just its constants
- [ ] **SIBLING**: `presentation.frag.spv` recompiled with glslang 11:16.2.0; `scripts/check-shader-artifacts.sh` green
- [ ] **TESTS**: grey ramp monotonic past 1.0 reaching ≥ 0.99 at 16, plus a finite-output assertion for exact-zero input


---

# 4579: REN-D6-2026-09-21-01: `c0b740ce7` silently disabled `translate_material_copies_every_canonical_field` — the NIFAL boundary's core copy-fidelity regression test

State: OPEN  Labels: ['bug', 'renderer', 'medium', 'nifal', 'test-gap']

**Severity**: MEDIUM (a test gap, but on the single HIGH-floor NIFAL boundary with all-game blast radius; the production code is currently correct) · **Dimension**: NIFAL Material
**Location**: `byroredux/src/material_translate.rs`, `mod canonical_completeness_harness`: the orphaned doc block + `#[test]` (~:2722-2739), the new `an_msn_named_normal_slot_declares_model_space_normals` (~:2740-2775), and the now un-annotated `fn translate_material_copies_every_canonical_field()` (~:2777); meta-pin `every_source_derived_material_field_is_pinned_by_a_test` (~:3019)
**Status**: NEW (owner: `/audit-nifal` Dim 1; found by the renderer audit. The 2026-09-21 NIFAL report predates `c0b740ce7`.)
**Verified against**: HEAD `f97775ca8`

## Description

`c0b740ce7` ("Fix #4548: an _msn-named normal slot declares model-space normals at the translate boundary") added a new test (doc comment + `#[test]` + fn). It landed between the existing `#[test]` attribute of `translate_material_copies_every_canonical_field` and that fn's signature. At HEAD:

```rust
    /// The core regression: every canonical-tier field the boundary is
    /// documented to copy must carry its source value through unchanged. …
    #[test]                                   // orphaned: now applies to the fn below
    /// #4548 (…) — a material whose BGSM authors `model_space_normals = false` …
    #[test]
    fn an_msn_named_normal_slot_declares_model_space_normals() { … }

    fn translate_material_copies_every_canonical_field() {   // no #[test]
```

So `an_msn_named_normal_slot_declares_model_space_normals` carries two `#[test]` attributes. The core copy-fidelity test is now a plain fn that never runs.

## Evidence

- The source layout above.
- The audit listed both HEAD-built `byroredux` test harnesses (2353 tests). `an_msn_named_normal_slot_declares_model_space_normals` is listed twice (it ran twice: "2 passed"), and `translate_material_copies_every_canonical_field` is not listed at all.
- `every_source_derived_material_field_is_pinned_by_a_test` scans the test module's source text (`include_str!("material_translate.rs")`) for asserting statements that read each field. It stays green while the assertions it counts never execute.

## Impact

The #2214/#3462 contract is void for every field that only this test covers. That contract says that "deliberately reverting any single `source.X` → `material.X` line … fails the corresponding assertion". The affected fields include the `water_shader_flags`/`is_water_shader` NIFAL↔WATAL seam and the emissive, specular, diffuse, ambient, UV, alpha, env-map and vertex-colour copies. A boundary drop in any of those now ships green.

## Related

- #4548: the fix commit `c0b740ce7` belongs to.
- #4411 (open): the same meta-pin also counts comment prose as a pin. That is a second blind spot of the same instrument, so harden both together.
- #2214, #3462: the contract this test implements.

## Suggested Fix

- Move the orphaned doc block and `#[test]` back onto `fn translate_material_copies_every_canonical_field()`, and drop the duplicate attribute from `an_msn_named_normal_slot_declares_model_space_normals`.
- Harden the meta-pin so a de-attributed test cannot satisfy it: either assert that the fn is immediately preceded by `#[test]` in the source text, or have the meta-pin call the fn directly.
- Sweep the rest of the harness for the same orphaned-attribute shape.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D6-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every `#[test]` in `canonical_completeness_harness` (and in the other `include_str!` source-shape meta-pin modules) sits directly on its fn; no other doubled or orphaned attributes
- [ ] **CANONICAL-BOUNDARY**: the fix touches only the test module of `byroredux/src/material_translate.rs`; `translate_material` itself is unchanged
- [ ] **TESTS**: the meta-pin fails when `translate_material_copies_every_canonical_field` loses its `#[test]` (verified by temporarily removing it)


---
