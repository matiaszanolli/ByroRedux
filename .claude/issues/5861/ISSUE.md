# #5861: GAME-D4-2026-10-15-03: Fallout 4 corpses are re-animated by their behavior graph

- **Labels**: medium,bug,gameplay,combat,animation,game:fo4
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Severity**: MEDIUM
- **Dimension**: Combat & Death (Dim 4), the `reconcile_dead_actor` teardown
- **Location**: `byroredux/src/combat.rs:963-1056` (`reconcile_dead_actor`); `byroredux/src/asset_provider/actor_graph.rs:307-318` (`resolve_wanted`); the graph step (`object_graph_step_system`) and `byroredux/src/systems/graph_locomotion.rs:124-166`
- **Status**: NEW (from `3bb6806b5` / `df6ad0410` / `e8026e737`)
- **Trigger**: FO4, any cell where a humanoid NPC dies while another FO4 actor's graph loads a clip on demand. Also any later graph step or locomotion tick.
- **Description**: The teardown removes `AnimationPlayer` and `AnimationStack` on the actor and its skeleton root, but leaves `ObjectAnimationGraph` and `GraphLocomotion` in place. Three paths then put a clip back on the corpse through `apply_playback_for`:
  1. `resolve_wanted` calls `graph.start()` on **every** graph sharing the loaded graph. FO4 humanoids all share one, and no `Dead` filter exists. This was verified at HEAD: the filter is `std::ptr::eq(graph.graph(), &*job.loaded)` only.
  2. The graph step has no `Dead` filter.
  3. `graph_locomotion` reads `Dead` but treats the corpse as parked and sends `moveStop`.

  FO4 ragdolls do not activate yet (`docs/engine/physal.md`), so nothing overrides the restored pose.
- **Impact**: An FO4 corpse stands, idles or walks in place.
- **Suggested Fix**: Remove `ObjectAnimationGraph` and `GraphLocomotion` (and `ConsoleWalk`) in `reconcile_dead_actor`, or make every graph consumer skip `Dead`. Restrict `resolve_wanted`'s restarts to the graphs actually waiting on the provided clip; restarting all of them also makes live actors hitch, which belongs to `/audit-scripting`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
