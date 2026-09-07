# #3806 — investigation

## The blocked-on list was half-stale

The issue gates itself on #3802 and #3803, both of which read CLOSED.
Only one of them is actually done:

| Blocker | State | Reality |
|---|---|---|
| #3802 cross-tile NAVM path connectivity | CLOSED | **Done.** Landed in `494f18ad`, verified on `main`. |
| #3803 actor/package suspend/migrate/resume | CLOSED | **Not done.** Closed as a *duplicate of #3299*, which is still OPEN. |

So "both blockers closed" does not mean "fully unblocked". `unload_cell_inner`
still calls `world.despawn_batch(victims)` on every cell-owned entity with only
the narrow `cinematic_retained_entities` exception — there is no actor
migration to observe, and no package state that survives a boundary.

That splits the acceptance criterion cleanly in two:

- **Path-side** ("no dangling path") — has substrate, fully tested now.
- **Actor-side** ("no duplicate entity, no lost package state") — has no
  substrate. A test here would pin the despawn-wholesale behaviour that
  #3299 exists to replace. Deferred to #3299 by name, in the module doc.

## Why the harness re-drives unload instead of calling it

`unload_cell` / `unload_cell_inner` take `&mut VulkanContext`; there is no
device under `cargo test`. But `NavmeshTile`'s own doc comment states it
"carries no GPU handle" and rides the generic
`stamp_cell_root_range` → `CellRootIndex` → `unload_cell` chain — so the only
two steps of `unload_cell_inner` a NAVM row ever reaches are `despawn_batch`
and `bump_navmesh_residency`. The harness calls the real `drain_cell_victims`,
the real `despawn_batch` and the real `bump_navmesh_residency`; the phases it
skips (mesh/texture drops, item instances, Rapier bodies) are provably no-ops
for these rows.

`the_harness_mirrors_the_unload_steps_navm_residency_depends_on` source-pins
both steps so the mirror cannot silently go stale.

Placement follows from visibility: `drain_cell_victims` is `pub(super)` within
`cell_loader`, so the harness must live inside that module tree. Reaching
`systems::navmesh_path` from there needed one widening, `mod` →
`pub(crate) mod` (matching the existing `pub(crate) mod weather;`).

## Gotcha the soak test had to encode

`World::despawn_batch` documents that it "does not reclaim entity IDs", and
`next_entity_id` is a monotonic high-water mark. A soak leak assertion written
against `OwnershipSnapshot::entities_spawned` therefore fails on a *clean* run.
Row counts are the leak signal. Pinned by
`the_soak_leak_check_reads_row_counts_because_entity_ids_never_recycle` so the
trap is documented rather than rediscovered.

## Non-vacuity

Every assertion group was fault-injected and confirmed to fail:

| Injected fault | Tests that caught it |
|---|---|
| drop the `residency_generation` check from the cache predicate | 4 |
| nudge the far tile 0.5u so no border vertex matches exactly | 8 |
| stop despawning victims on unload (row leak) | 5 |
| drop `bump_navmesh_residency` from `unload_cell_inner` | 1 (the source pin) |

## Fixtures shared, not copied

`two_triangle_quad` / `adjacent_quad` / `zup_from_yup` moved out of
`navmesh_path`'s `mod tests` into `#[cfg(test)] pub(crate) mod test_tiles`.
The exact-shared-vertex property is what makes a cross-tile portal exist; a
second hand-written copy that drifted by one unit would leave this harness
exercising two unconnected tiles while still "passing" the invalidation
checks for the wrong reason. (Fault injection #2 above is exactly that
scenario.)
