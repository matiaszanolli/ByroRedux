# #5371: CONC-D5-2026-10-08-01: `forcegreet_system` and `eat_sleep_system` hold the `PhysicsWorld` guard across storage acquisitions, inverting the order production records

**Labels**: high,concurrency,physics,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5371

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-08.md` — `CONC-D5-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: HIGH. ECS deadlock potential has a floor of HIGH (`_audit-severity.md`). There is **no runtime deadlock today**: both
  systems are exclusive and the participants in the reverse order run elsewhere. The closest precedent, #4325 (`npc_combat_ai_system`,
  same shape), was graded MEDIUM on exactly that reasoning, whereas #2134 (HIGH) and the recent #5305 (HIGH) were graded at the floor.
  I followed the table; the orchestrator may reasonably downgrade to MEDIUM.
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics)
- **Location**:
  - `byroredux/src/systems/forcegreet.rs:66-125` (`forcegreet_system`; guard bound at `:66`, never dropped; nested acquisitions at `:72`, `:76`, `:83`, `:90`, `:98`, `:104`, `:112`, `:119`).
  - `byroredux/src/systems/eat_sleep.rs:128-153` (`eat_sleep_system`; guard bound at `:128`, nested acquisitions at `:133`, `:141`, `:147`).
  - Registered at `byroredux/src/boot/schedule/update.rs:442` (Update, exclusive) and `byroredux/src/boot/schedule/post_update.rs:129` (PostUpdate, exclusive).
- **Status**: NEW. The same class as closed #2134 / #3262 / #3655 / #4325; none of those covers these two sites (both added after the baseline: `14cff35ae`, `00f580e09`).
- **Verification Path**: `cargo test` under `BYRO_LOCK_ORDER_CHECK=1` — reproduced in the scratch copy (below).
- **Description**: `docs/engine/ecs.md` § Lock-ordering policy states the rule: *"`PhysicsWorld` last, with nothing taken under it … it
  is a sink with no outgoing edges"*; `ragdoll_writeback_system`'s comment (`ragdoll.rs:607-614`) calls it "the crate-wide 'no storage
  under a `PhysicsWorld` guard' rule". Every other locomotion system follows it with the same three-pass shape — collect under
  storage guards, drop them, take `PhysicsWorld` for the `step_toward` / `step_along_waypoints` loop alone, then apply writes
  (`travel.rs:262-340`, `patrol.rs:191-195`, `wander.rs:378-382`, `follow.rs:262-267`, `guard.rs:235-239`, `escort.rs:368-375`,
  `combat_ai.rs:249-256`). The two new systems skip that shape:
  - **`forcegreet_system`**: binds `physics_guard` at `:66` before the loop and keeps it to the end of the function. Under it the loop reads `Dead` / `AiCombatState` /
    `ActorControlState` (`npc_refuses_dialogue`), `GlobalTransform` (`get`, `query_mut`), `WalkSpeed`, `Transform` (`get`, `query_mut`), and then calls
    **`forcegreet_open`** — the whole dialogue-open stack (`LoadedCellIndex`, the CTDA evaluator's read set, `apply_selection`'s `DialogueRegistry` /
    `NpcDialogueTopic` / `DialogueSurfaceState` writes, the spoken-INFO fragment effects, `raise_hello_story_event`) — and finally `query_mut::<ForceGreetDirective>()`.
  - **`eat_sleep_system`**: inside the walk branch, `physics_guard` (`:128`) is held across `world.get::<Transform>` (an argument of `step_toward`), `query_mut::<GlobalTransform>` and
    `query_mut::<Transform>` until the `continue` at `:153`. The module doc says the walk is "the same straight-line `step_toward` locomotion the force-greet bridge uses", so it
    inherited the shape.
- **Evidence**: Production already records the reverse edges. `ragdoll_writeback_system` (Late exclusive) holds `Transform`, `Parent`, `Children`,
  `GlobalTransform` (write), `LocalBound`, `WorldBound` and *then* takes `PhysicsWorld` (`ragdoll.rs:593-615`). (`push_kinematic` no longer contributes: it snapshots its storage guards before taking the resource, `sync.rs:1209-1228`, #2404.)
  Measured in the scratch copy (edits: `world.insert_resource(PhysicsWorld::new())` in `eat_actor_far_from_destination_walks`, and a new 14-line `forcegreet_system` test with a `PhysicsWorld`):
  ```
  systems::eat_sleep::tests::eat_actor_far_from_destination_walks … FAILED
    attempted acquisition of `…::transform::Transform` while holding `byroredux_physics::world::PhysicsWorld` on this thread — that closes a cycle …:
    `Transform` → `PhysicsWorld` → `Transform`
  systems::forcegreet::scratch_experiment::scratch_forcegreet_with_physics_world … FAILED
    attempted acquisition of `…::global_transform::GlobalTransform` while holding `byroredux_physics::world::PhysicsWorld` …:
    `GlobalTransform` → `PhysicsWorld` → `GlobalTransform`
  ```
  The first edge in each chain was already in the process-wide graph from tests that ship in the binary (the only production site that holds `Transform` / `GlobalTransform` across the `PhysicsWorld` acquisition is `ragdoll_writeback_system`; I did not attribute the edge to one specific test), exactly as it would be in a real run once the Late ragdoll pass has executed.
  Why the lane does not see it: `forcegreet_system` has no unit test, and the three `eat_sleep_system` tests run **without** a `PhysicsWorld` resource (`try_resource` returns `None`, no edge) — the same blind spot #2134 documents.
  The follow / guard / travel systems each have a "with a real `PhysicsWorld` installed" regression test (`follow.rs:561`, `guard.rs:487`, `travel.rs:565`); these two have none.
- **Trigger Conditions**: A debug build with `BYRO_LOCK_ORDER_CHECK=1`, a loaded cell (any `PhysicsWorld`), and one actor carrying a `ForceGreetDirective` far from the player, or an `EatBehavior` / `SleepBehavior` actor outside `ARRIVE_RADIUS` (64 u) of its destination. Any cell where an actor's Eat/Sleep or Dialogue package wins ambient selection qualifies. The panic fires once `ragdoll_writeback_system` has also run (it runs every Late frame).
- **Impact**: The detector aborts the debug session at the first walking step, and — more importantly — it blinds the tool that gates promoting a system to a parallel lane. Promoting `forcegreet_system`
  or `eat_sleep_system` (or putting any parallel `GlobalTransform` / `Transform` writer in their stage window) turns it into a real `PhysicsWorld(R) → X(W)` against `X(W) → PhysicsWorld(R)` ABBA. No release-build effect today.
- **Related**: #2134, #3262, #3655, #4325 (same class), `docs/engine/ecs.md` § Lock-ordering policy, CONC-D3-2026-10-08-01 (the `forcegreet_open` nest also records `PhysicsWorld → LoadedCellIndex …`).
- **Suggested Fix**: Use the three-pass shape the sibling systems share. Pass 1a gathers `(npc, current, rotation, speed, target)` into an owned `Vec` under storage guards (and, for force-greet, partitions "refuses", "walks", "opens" and
  resolves `forcegreet_open` **after** the physics scope); Pass 1b takes `PhysicsWorld` alone for the `step_toward` calls; Pass 2 applies the `GlobalTransform` / `Transform` writes. Add a "with a real `PhysicsWorld`" test per system under the detector, like `travel.rs:565`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every `try_resource::<PhysicsWorld>()` site in `byroredux/src/systems/` sits in a block that acquires no storage)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
