# #6208: ECS-2026-10-21-01: idle-playback exclusives take `AnimationClipRegistry` under the `AnimationPlayer` write guard

**Labels**: high,ecs,concurrency,bug

**Source**: docs/audits/AUDIT_ECS_2026-10-21.md · **Severity**: HIGH · **Dimension**: 1 Lock Ordering & Deadlock Machinery · **Location**: `byroredux/src/systems/cinematic.rs:64-98`

- **Location**: `byroredux/src/systems/cinematic.rs:64-98` (`idle_clip_playback_system`: the `one_shot` closure is called inside the `query_mut::<AnimationPlayer>()` block at :70), `:181-212` (`idle_resume_system`: the `finished` closure is called inside the block at :189)
- **Status**: NEW. The code was written in `d1b298921` (2026-10-18). The edge first reached the detector through `355f752d8` (Fix #6179, 2026-10-20), whose new test `scripted_one_shot_idle_marks_the_actor_until_it_finishes` is the first to drive both systems with a populated registry. That commit is the first red CI run (`51f97f708`'s run 39425702282 contains only this cycle).
- **Description**: `animation_system_inner` holds `AnimationClipRegistry` as its outermost lock for the whole function. Its doc (`systems/animation.rs:501-512`, #2400) states that nothing may acquire the registry while holding `AnimationPlayer`/`AnimationStack`. Both idle systems do exactly that. Each one's closure calls `world.try_resource::<AnimationClipRegistry>()` per request while `players` (a `QueryWrite<AnimationPlayer>`) is alive. Detector message: `attempted acquisition of AnimationClipRegistry while holding AnimationPlayer … closes a cycle AnimationClipRegistry → AnimationPlayer → AnimationClipRegistry`.
- **Evidence**:
  ```rust
  let one_shot = |handle: u32| { world.try_resource::<AnimationClipRegistry>() … };   // :64
  if let Some(mut players) = world.query_mut::<AnimationPlayer>() {                    // :70
      … if player.clip_handle != *handle && one_shot(*handle) { … }                   // registry under players
  ```
  `idle_resume_system` has the same shape with `finished(player.clip_handle, player.local_time)` inside the `players` loop.
- **Impact**: Today both systems are Update *exclusives*, and `animation_system` is Update's only parallel system, so the two never run on different threads in the same frame. There is no live deadlock. The damage is to the gate. The ABBA lane is the only mechanical check of the process-wide lock order, and it has been red for 22 consecutive `main` runs. Two more cycles have already landed while it was red (ECS-2026-10-21-02). A third cycle that *is* reachable across threads would not be noticed. The cycle becomes live if either system is ever promoted to parallel.
- **Related**: #2400 (the invariant), ECS-2026-10-21-02 (the other cycle keeping the lane red), `/audit-concurrency` Dim 3/5.
- **Suggested Fix**: Resolve the clip facts before taking the player guard: collect `(handle → is_clamp, duration)` for the request handles under one registry read, drop it, then take `query_mut::<AnimationPlayer>()`. `pick_idle_system` (:150-161) already does this.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
