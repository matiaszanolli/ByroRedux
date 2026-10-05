# #5307: ECS-2026-10-05-D5-02: `npc_dialogue_selection`'s Access row declares the fragment queues as reads, but the #5152 spoken-fragment path writes them

**Labels**: low,ecs,concurrency,dialogue,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5307

**Source**: `docs/audits/AUDIT_ECS_2026-10-05.md` — `ECS-2026-10-05-D5-02` (HEAD `a2c24b16e`)

- **Severity**: LOW
- **Dimension**: 5 — Scheduler Wiring
- **Location**:
  - `byroredux/src/boot/schedule/late.rs:437-466` (the row; the queue reads are at `:459-460`)
  - `crates/scripting/src/fragment/effects.rs:348` and `:1886` (the writes)
- **Status**: NEW. The same class as #5035 (closed) and #5069 (open, a different row).
- **Description**:
  - `c59600138` added `.reads_resource::<FragmentExecutionQueue>()` and `.reads_resource::<PendingFragmentActivations>()`.
  - The path it describes is `speak_info_*_fragment` → `apply_spoken_info_fragment` → `apply_fragment_guard_free`. It takes:
    - `try_resource_mut::<FragmentExecutionQueue>()` in `apply_effects` (`effects.rs:1886`)
    - `try_resource_mut::<PendingFragmentActivations>()` in `DeferredFragmentEffects::apply_at_depth` (`effects.rs:348`)
  - It also calls `mark_scene_actor_bindings_dirty`, which writes `SceneActorBindings`. The row declares that type read-only.
- **Impact**: The analyzer never pairs exclusives, so no conflict is hidden today. The row is the promotion baseline, and the reference for the recorded `BYRO_LOCK_ORDER_CHECK` edges. A read-vs-write mismatch is exactly what #4573 taught the parallel guard to reject. This row is outside that guard's reach (not in `PARALLEL_SYSTEMS`, not one of the three covered exclusive fns).
- **Suggested Fix**: Change these three declarations to `writes_resource`: `FragmentExecutionQueue`, `PendingFragmentActivations` and `SceneActorBindings`.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
