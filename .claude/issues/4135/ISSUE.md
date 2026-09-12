### SAVE-D1-2026-09-11-01: `execute_pending_save_loads`'s #3789 fix restores the WHOLE resource set — including `ItemInstancePool` — before the cell teardown, so the teardown's item-instance release call now frees slots in the pool the player is trying to load, not the one being discarded

- **Severity**: CRITICAL
- **Dimension**: 1 — Snapshot Completeness & Determinism
- **Data-Loss Class**: corruption-on-load
- **Location**: `byroredux/src/save_io.rs:1546` (pre-reload `restore_resources`, added by `568343f3`/#3789) vs. `:1557-1559` (the reload call) vs. `byroredux/src/cell_loader/unload.rs:556-580` (`release_victim_item_instances`) vs. `byroredux/src/cell_loader/transition.rs:386-396` (`unload_current_interior` → `unload_cell`) vs. `crates/core/src/ecs/resources/mod.rs:1447-1484` (`ItemInstancePool::allocate`/`release`, slot-index based) vs. `crates/save/src/registry.rs:180-187` (`register_resource`'s `load` closure — `world.insert_resource(res)`, a wholesale replace, not a merge)
- **Status**: NEW. Introduced by `568343f3` (Fix #3789), which fixed a prior-cycle HIGH (`SAVE-D6-2026-08-30-01`, `ReferenceEnableState` read before its saved value arrived). The prior report's own *Disproved Candidates* section explicitly reasoned about this exact hazard in the opposite (safe) direction and warned against the fix that landed: *"the release happens before the wholesale replacement... The constraint is real in the other direction, and is why the fix for SAVE-D6-2026-08-30-01 cannot simply hoist `restore_resources` wholesale."* The fix that shipped is exactly the "hoist it wholesale" move that warning named.
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: `execute_pending_save_loads` now calls `byroredux_save::restore_resources(world, &registry, &snapshot)` — the full registry, not a filtered subset — at `:1546`, before `reload_interior_session`/`reload_exterior_session` run at `:1557`/`:1559`. `restore_resources` iterates every `register_resource` entry and calls its `load` closure, which is `world.insert_resource(res)` — a full replace. `ItemInstancePool` is one of those registered resources (`save_io.rs:448`).

`reload_interior_session` then calls `unload_current_interior(world, ctx)` (`:1230`), which reaches `unload_cell` → `release_victim_item_instances`. That function walks the **currently-loaded (pre-load, live)** cell's `Inventory` components, collects every `ItemStack.instance: Some(id)`, and calls `pool.release(id)` against whichever `ItemInstancePool` is currently installed:

```rust
pub(crate) fn release_victim_item_instances(world: &mut World, victims: &[EntityId]) {
    let mut to_release: Vec<ItemInstanceId> = Vec::new();
    { /* collect ids from the live Inventory rows */ }
    let Some(mut pool) = world.try_resource_mut::<ItemInstancePool>() else { return };
    for id in to_release {
        pool.release(id);
    }
}
```

Because of the #3789 reorder, that pool is now the **just-restored saved one**, not the live one the ids were actually allocated against. `ItemInstancePool::release` is a raw slot-index operation with no ownership/generation check across a resource swap:

```rust
pub fn release(&mut self, id: ItemInstanceId) -> Option<ItemInstance> {
    let slot = id.0.get();
    let cell = self.instances.get_mut(slot as usize)?;
    let taken = cell.take()?;
    if !self.free.contains(&slot) { self.free.push(slot); }
    Some(taken)
}
```

If the live-session id happens to index a populated slot in the saved pool — likely, since both pools are simple monotonically-growing arenas starting at slot 1 — this **deletes a legitimate saved `ItemInstance`** (a named/modded weapon's condition, a unique item's custom name) and returns that slot to the saved pool's free list, moments before `apply_deltas` overlays the very `Inventory` rows that reference it by that same id.

**Evidence**: Ordering confirmed by direct read of `save_io.rs:1471-1660` (pre-reload restore at `:1546`, `reload_interior_session` call at `:1557`, second restore at `:1591`, `apply_deltas` at `:1598`) — independently re-verified during publish, not taken on the audit report's word alone. `unload.rs:556-580` quoted verbatim above, called from `unload_cell`, shared by both the interior (`cell_loader/transition.rs:393`) and exterior (`cell_loader/exterior.rs:1277`) paths. `ItemInstancePool::release`/`allocate` confirmed slot-index-based with no cross-swap ownership check. `register_resource`'s `load` closure confirmed a bare `world.insert_resource` — total replace, not additive. No existing test exercises this path: `crates/save/tests/round_trip.rs`'s fixtures call `apply_deltas`/`build_form_id_remap`/`restore_resources` against hand-built `World`s directly, never through `unload_cell`/`release_victim_item_instances`; `byroredux/src/save_io/live_reload_tests.rs`'s own `saved_resources_are_restored_before_the_cell_reload` test's doc comment says outright that `execute_pending_save_loads` needs a live `VulkanContext` and is not reachable from a unit test, which is the same limitation that makes this pool-corruption path untested.

**Impact**: Every `load <slot>` / quickload / F9 in an active session (not the very first load right after boot) that has any inventory-bearing entity resident in the currently-loaded cell/tiles with an allocated `ItemInstance` — any named/modded/unique item in a container or carried by a nearby NPC — silently wipes that saved instance's data and frees its slot in the pool the player is loading *into*. The overlaid `Inventory` row for the entity that legitimately owns that instance then points at an emptied slot, or a slot a later `allocate()` in the same load reuses for an unrelated new item — silently aliasing two different items' data. This lands exactly in the class the subsystem exists to prevent: a save that round-trips CRC-clean on disk still loses real player-authored item state on the *load* side, with no log line pointing at the cause. Both interior and exterior reload branches are equally exposed, since `unload_cell` is the shared teardown for both.

**Related**: #3789 (introduced this ordering); `SAVE-D6-2026-08-30-01` (the HIGH #3789 correctly fixed — this is a new side effect of that same fix, not a reopening of it); the prior report's own *Disproved Candidates* entry that named this exact hazard.

**Suggested Fix**: Don't restore the *entire* registry before the reload — only the resources the spawn path actually consults pre-reload (today: `ReferenceEnableState`; per #3789's own comment, "and any future sibling"). Split `restore_resources` into a small pre-reload subset by resource name (mirroring `MUTABLE_DELTA_COLUMNS`'s explicit-list pattern), leaving `ItemInstancePool` and everything else for the existing post-reload call. Failing that, capture the live pool's state before the pre-reload restore and pass it to `release_victim_item_instances` explicitly, or force `unload_cell`'s teardown to run strictly before any `restore_resources` call. Add a regression test modeling a live entity's `Inventory.instance` allocation, calling the pre-reload restore with a *different* saved pool state, then running `release_victim_item_instances`, and asserting the saved pool's populated slots are untouched.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — model a live entity's `Inventory.instance` allocation, run the pre-reload restore against a different saved pool state, run `release_victim_item_instances`, assert the saved pool's populated slots are untouched
- [ ] **SIBLING**: Check every other resource in the pre-reload `restore_resources` call for the same "restored-before-teardown-consumes-the-live-one" hazard, not just `ItemInstancePool`
