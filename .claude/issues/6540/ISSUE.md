# GAME-D4-2026-10-31-02: A mine blast replaces a colliding `HitEvent`, so two mines tripped together land one blast

Labels: medium,gameplay,combat,bug,game:fo4

Filed from `docs/audits/AUDIT_GAMEPLAY_2026-10-31.md` (finding `GAME-D4-2026-10-31-02`).

- **Severity**: MEDIUM
- **Dimension**: Combat & Death (Dim 4, single damage path)
- **Location**: `byroredux/src/systems/mines.rs:280-297` (`detonate`)
- **Status**: NEW (`c36ab4125`). ECS-2026-10-31-02 / CONC-2026-10-31-02 flag the missing pin entry and call the overwrite "a design note and not a defect"; the multi-mine case below is why it is one.
- **Trigger**: Fallout 4. Two or more mines within 500 BU of each other, tripped by the same approaching actor, or a mine blast on a target that takes a melee or projectile hit the same frame.
- **Description**: `HitEvent` is one `SparseSet` row per target and `insert` replaces (the #4324 contract stated in `combat_ai.rs:259-264`). Every other producer holds a collision instead: `npc_combat_ai_system` keeps the attacker ready and strikes next frame (`combat_ai.rs:268-275`), and `pending_trap_hit_flush_system` waits a frame (#5860, pinned by `a_same_frame_melee_hit_survives_a_projectile_hit`). `detonate` calls `events.insert(target, ...)` for every actor in the outer radius without looking. A minefield trips in groups: `mine_system` scans every 0.25 s and arms every mine within `proximity` of the approaching actor in the same pass, each with the same record fuse (vanilla `fragMineExplosion`, 1.5 s), so adjacent mines reach `fuse_left <= 0` in the same frame. Their blasts overlap (outer radius 500) and the second `insert` replaces the first on every shared target, the player included.
- **Evidence**:
  ```rust
  if let Some(mut events) = world.query_mut::<byroredux_scripting::HitEvent>() {
      for (target, damage) in hits {
          events.insert(target, byroredux_scripting::HitEvent { aggressor: mine, ... damage, ... });
  ```
  There is no `events.get(target).is_some()` check, unlike `combat_ai.rs:268`.
- **Impact**: The player takes one mine's damage where the data says N. A melee hit that lands on the blast frame is silently dropped. No crash or save damage. Not measured: how many vanilla mines sit within one outer radius of a neighbour.
- **Related**: #4324, #5860 (closed, same contract); ECS-2026-10-31-02 (the pin).
- **Suggested Fix**: Make `detonate` stage its hits and keep the colliding ones for the next frame (a `PendingMineHits` resource drained by `mine_system`'s next pass, or defer the whole blast a frame), the way the other producers hold. Add a two-mines-one-frame case to the `mines.rs` tests. Fold the pin change from ECS-02 into the same commit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
