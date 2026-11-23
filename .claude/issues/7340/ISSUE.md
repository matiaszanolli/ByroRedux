# #7340: GP-04: The spawn-time package scan cannot see a reference-keyed Once Per Day block, so a stream respawn can re-run the package inside its 24 hours (#7317 half-applied)

- **Severity**: LOW
- **Dimension**: 5
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:1223-1236` (spawn scan and `note_selected`), `byroredux/src/npc_spawn/package_flags.rs:56-62` (`block_owner`), `:143-160`; `byroredux/src/cell_loader/references/mod.rs:873` (the stamp happens after the job); `byroredux/src/cell_loader/stream_snapshot.rs:129` / `:324` (`has_package_state`)
- **Status**: NEW (follow-up to closed #7317 c99ccdb23)
- **Domain**: ai, gameplay · **Game**: FNV, FO3, Oblivion
- **Trigger**: An actor with a Once Per Day package P (PKDT bit 10) and no always-open fallback package. P is selected at 08:10 on day 0. The cell unloads at 13:00 with nothing running. The player returns at 08:05 on day 1, when P's window is open again but its block (until 08:10) is live.
- **Description**: #7317 re-keys the block by the actor's placed reference (`FormIdComponent`). But `finish_actor` calls `apply_ai_package_behavior` inside the spawn job, and the job's placement root has no `FormIdComponent` yet. It is stamped later by `stamp_quest_reference` (mod.rs:873, then `adopt_entity_day_blocks`). So the spawn scan (`select_active_package(world, placement_root, .., None, ..)`) computes `block_owner` = `Entity(new id)` and never sees the `Reference` block. P wins, its behavior is installed, and `active_package_form_id = P`. On the first tick P is the running package (`current == Some(P)`), which `once_per_day_blocked` exempts (`current != Some(package.form_id)`). The block is bypassed for the whole window. `adopt_entity_day_blocks` only merges timestamps afterwards ("earlier start wins"); it does not undo the install. The #7317 test `a_block_noted_before_the_reference_stamp_moves_onto_it_keeping_the_earlier_start` even notes "a respawn whose spawn job re-selected it".
  The stream snapshot rescues the common case. When the actor was running some other package at eviction, `restore_actor_snapshot` calls `install_package_now(fallback)` and replaces P. The snapshot row exists only when `has_package_state()` (an active package, travel state or a seat). So the bypass survives exactly for actors with nothing running at eviction.
- **Evidence**:
  ```rust
  // ai_package.rs:1223  let active = select_active_package(world, placement_root, game_hour, None, false, ..);
  // package_flags.rs:56 fn block_owner: world.get::<FormIdComponent>(actor).map(Reference).unwrap_or(Entity(actor))
  // package_flags.rs:158-160  once_per_day_blocked: .. && current != Some(package.form_id) && ..
  ```
- **Impact**: A Once Per Day package re-runs after a stream round trip inside its 24-hour block, for actors with no fallback. This is the AIP-FLG-001 defect #7317 set out to close, remaining for one entry path.
- **Related**: #7317, #7252, AIP-FLG-001.
- **Suggested Fix**: Give the spawn scan the placed reference. Either stamp `FormIdComponent` before `apply_ai_package_behavior` runs, or pass the reference into `select_active_package` as the owner. Alternatively, when `adopt_entity_day_blocks` finds a pre-existing live `Reference` block for the spawn winner, re-select once after the stamp.
- **Dimension agent ID**: GP-D5-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other action systems / movers / park paths / add paths named in the finding)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the system's `Access` row matches
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_GAMEPLAY_2026-11-23.md`
