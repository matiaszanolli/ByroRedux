# #5373: ECS-2026-10-08-D6-01: `forcegreet_system` and `eat_sleep_system` write the walk step into `GlobalTransform`, and transform propagation overwrites it, so force-greet and Eat/Sleep NPCs never walk

**Labels**: high,ecs,gameplay,ai,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5373

**Source**: `docs/audits/AUDIT_ECS_2026-10-08.md` — `ECS-2026-10-08-D6-01` (HEAD `00f580e09`)

- **Severity**: HIGH. This is ECS state correctness: a derived component is written, and the source-of-truth component is not. Eat and Sleep are common FO3/FNV schedule procedures, so many NPCs are affected. Owner overlap: `/audit-gameplay` (behaviour).
- **Dimension**: 6 — Propagation invariants
- **Location**:
  - `byroredux/src/systems/forcegreet.rs:76-109`
  - `byroredux/src/systems/eat_sleep.rs:116-152`
- **Status**: NEW (introduced by `14cff35ae` and `00f580e09`)
- **Description**: Both systems read the NPC's position from `GlobalTransform`, call `locomotion::step_toward`, and then:
  - write `new_pos` into **`GlobalTransform.translation`**;
  - write only the rotation into `Transform`.

  NPC placement roots are propagation roots, so `Transform` is the authoritative world pose. Two mechanisms undo the step:
  - The `Transform` rotation write uses `get_mut`, which marks the entity dirty unconditionally (`packed.rs:129`). `step_toward` returns `Some(rotation)` on every tick that still has distance to cover.
  - On the next propagation, a dirty root's global is rebuilt from its local (`ecs/systems.rs`, incremental seed: `None => GlobalTransform::new(local.translation, …)`). Every structural change does the same for all roots (Phase 1b). `Transform.translation` was never advanced, so the root snaps back to its old position.

  The effect differs by system:
  - **`forcegreet_system`** (Update) runs *before* PostUpdate propagation, so the step is erased in the same frame, before rendering.
  - **`eat_sleep_system`** (PostUpdate) runs after propagation. Its step moves only the root's global for one frame. The root carries no mesh, and the skeleton and body children are not recomposed. The next propagation resets the root.

  Either way the next tick starts again from the original position. The NPC turns toward its goal but never moves.
  - Eat/Sleep's `ARRIVE_RADIUS` check therefore never passes, so the actor never sits at the marker, unless it spawned within 64 units of its destination.
  - Force-greet never opens unless the player is already inside `radius`.

  Every other M42 mover writes `Transform.translation`:
  - travel's pass 2
  - `wander.rs:424-431`
  - `follow.rs:305-306`
  - the seat snap `apply_seat_assignments`
- **Evidence**:
  ```rust
  // eat_sleep.rs:141 (forcegreet.rs:98 is identical)
  if let Some(mut transforms) = world.query_mut::<GlobalTransform>() {
      if let Some(transform) = transforms.get_mut(npc) { transform.translation = new_pos; }
  }
  if let (Some(new_rotation), Some(mut transforms)) = (new_rotation, world.query_mut::<Transform>()) {
      if let Some(transform) = transforms.get_mut(npc) { transform.rotation = new_rotation; }   // marks Transform dirty
  }
  ```
  `eat_actor_far_from_destination_walks` calls the system once and asserts on `GlobalTransform`, without running propagation, so it cannot see this. `forcegreet.rs` has no tests.
- **Impact**:
  - Every FO3/FNV NPC whose winning package is Eat (procedure 3) or Sleep (procedure 4) stands at its spawn point; `ai_package.rs:389-403` and `:510-523` install these from vanilla schedules.
  - Every Dialogue-procedure force-greet that starts outside its radius never opens.
  - `GlobalTransform` and `Transform` disagree for a frame on every tick, so any consumer that reads `GlobalTransform` in that window sees a pose that is then discarded: the eat_sleep root after PostUpdate, or physics and audio between forcegreet and propagation.
- **Related**: CONC-D5-2026-10-08-01 (the same two write blocks, a lock-order aspect); #4995 (the Transform-writer placement rule).
- **Suggested Fix**: Write `new_pos` into `Transform.translation`, together with the rotation, in a single `query_mut::<Transform>()` scope, the way travel and wander do. Drop the `GlobalTransform` write. Add a test that runs the system and then `make_transform_propagation_system` for N ticks and asserts the actor reaches its destination (Eat/Sleep) or opens the conversation (force-greet).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every NPC mover writes `Transform`, never `GlobalTransform`, on a propagation root — travel, wander, follow, seat snap)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
