# CONC-D3-NEW-01: cross-thread ABBA detector closes only length-2 cycles

**Issue**: #2675
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`


> **Emphasised.** The detector's own documentation claims general cycle detection. It does not
> deliver it — and a **complete 3-cycle already exists in the live edge graph today**, with
> `BYRO_LOCK_ORDER_CHECK=1` CI staying **GREEN** on it. It is unreachable only because the three
> producers happen to sit in three different scheduler stages. **Restaging any one of them
> springs the trap while the detector stays silent.**

- **Severity**: MEDIUM
- **Dimension**: 3 — ECS Lock Ordering & Deadlock
- **Location**: [lock_tracker.rs](crates/core/src/ecs/lock_tracker.rs) —
  `global_order::record_and_check` (both the read fast path and the write slow path), plus the
  module doc block above `mod global_order` and the file-header doc item 2.
- **Status**: NEW
- **Description**: `record_and_check` panics only when the *direct* reverse edge already exists:
  for each currently-held `held_id`, it tests `GRAPH[new_id].contains(held_id)` — i.e. "was
  `held_id` ever acquired while `new_id` was held?". There is **no reachability search** over
  `GRAPH`, so a cycle of length ≥ 3 (`A → B`, `B → C`, `C → A`, each edge observed on a
  different thread and each individually legal) is recorded happily and never reported. The
  documentation claims more than the code delivers: the module doc says "If … the graph has a
  cycle … the second observation panics" and "the graph generalizes the guarantee to any N-lock
  hold pattern across the scheduler". The generalization is real for *how many locks are held at
  once* (every held lock contributes an edge) but not for *cycle length*, which is capped at 2.

  This is not hypothetical: the current schedule already writes a complete 3-cycle into `GRAPH`
  on any character-mode frame.
- **Evidence**: The detector, at both check sites:
  ```rust
  if let Some(new_edges) = graph.get(&new_id) {
      for (held_id, held_name) in held_others {
          if new_edges.contains(held_id) {   // ← depth-1 only, no DFS
              panic!("ECS cross-thread deadlock risk (ABBA) …");
  ```
  The three edges that close the triangle, each confirmed by reading the guard lifetimes (not
  inferred from declarations):

  | Edge | Producer | Held-across evidence |
  |---|---|---|
  | `Transform → GlobalTransform` | `make_transform_propagation_system` ([systems.rs](crates/core/src/ecs/systems.rs)) | `query_mut::<Transform>()` is bound first and still live when `query_mut::<GlobalTransform>()` is taken 5 lines later (same fn body, no intervening drop) |
  | `GlobalTransform → CharacterController` | `camera_follow_system` ([character.rs](byroredux/src/systems/character.rs)) | inside the `let (body_pos, eye_height, prev_cam_y) = { … }` block: `gq = world.query::<GlobalTransform>()` then `cq = world.query::<CharacterController>()`; `gq` is read again (`gq.get(cam_entity)`) after `cq` is bound, so it is provably still held |
  | `CharacterController → Transform` | `character_controller_system` (same file) | inside the `let (controller, current_pos, …) = { … }` block: `cq = world.query::<CharacterController>()` then the nested `let pos = { let tq = world.query::<Transform>() … }` |

  The three producers live in `Stage::Early` (via `player_controller_system`),
  `Stage::PostUpdate`, and `Stage::Late` respectively — all three are `add_to_with_access`
  parallel-batch members ([boot.rs](byroredux/src/boot.rs)), so all three edges are recorded
  from rayon worker threads under `BYRO_LOCK_ORDER_CHECK=1`, and the CI `vulkan-validation` job
  stays green.
- **Impact**: The only automated cross-thread deadlock guard in the project reports "clean" for
  an entire class of real deadlocks. Today the triangle is *unreachable* because the three edges
  are produced in three different stages, and `Scheduler::run` runs stages strictly sequentially
  — so this is a detector-coverage defect, not a live hang. The blast radius is what happens
  next: any stage merge, any promotion of one of these three systems into a sibling's stage, or
  any new parallel system that reproduces one of these edges in a stage where another already
  exists, produces a hard hang (three rayon workers blocked forever, no panic, no log) that the
  `lock-order-check` and `vulkan-validation` jobs will both certify as passing.
  `camera_follow_system` and `transform_propagation` are both on the renderer-feeding path, so
  the hang would present as a frozen render loop.
- **Trigger Conditions**: Detection gap: **always** — no timing window needed; run CI today with
  `BYRO_LOCK_ORDER_CHECK=1` on a character-mode cell and all three edges land in `GRAPH` with
  zero diagnostics. Actual deadlock requires the three edge-producers to be co-scheduled: e.g.
  move `camera_follow_system` from `Stage::Late` to `Stage::PostUpdate` (it already carries an
  ordering comment tying it to `physics_sync_system`, so a future Physics/PostUpdate merge is
  plausible) and the `GlobalTransform → CharacterController` and `Transform → GlobalTransform`
  holds overlap; add `character_controller_system`'s `CharacterController → Transform` hold on a
  third worker and the cycle closes with no participant able to proceed.
- **Verification Path**: **Pure-CPU, `cargo test`-observable** — no Vulkan or RenderDoc needed.
  Add a unit test in `lock_tracker.rs`'s existing `global_graph_detector_end_to_end` style: with
  `set_enabled_for_tests(true)`, drive `A→B`, then `B→C`, then `C→A` on three sequential (or
  spawned) scopes and assert the third acquisition panics. It currently does not. Contrast with
  the existing scenario 1 in that test, which only exercises the 2-cycle the code does handle —
  which is precisely why the gap survived.
- **Related**: #2385 (GRAPH poison handling), #2386 (recursive same-type reads invisible to the
  graph), #2387 (no cross-worker test coverage), #2547 (detector documented as debug-only, omits
  default-off), #2388 (six inverted pairs among exclusives). None of these covers cycle length —
  they are all about *whether* an edge is recorded or *whether* the detector runs, not about the
  cycle-closure predicate.
- **Suggested Fix**: Replace the `GRAPH[new_id].contains(held_id)` test with a reachability
  probe — before inserting `held_id → new_id`, DFS/BFS from `new_id` over `GRAPH` and panic if
  `held_id` is reachable. The graph is tiny (one node per locked type) and the probe only runs on
  the novel-edge slow path, so the steady-state cost is unchanged. Until then, at minimum correct
  the module doc to say the detector closes *direct* two-lock cycles only, so a green
  `lock-order-check` is not read as proof of acyclicity.

---


---
*Filed from [`docs/audits/AUDIT_CONCURRENCY_2026-08-12.md`](docs/audits/AUDIT_CONCURRENCY_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `CONC-D3-NEW-01`.*

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related systems
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
