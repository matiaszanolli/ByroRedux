# #7093 door.state holds Door across TwoStateActivator: Door/TwoStateActivator lock-order cycle (lock-order-check lane red)

Labels: bug, medium, gameplay, concurrency · State: OPEN

## Problem
The `door.state` console command (`byroredux/src/commands/door.rs:51-52`) takes `world.get::<Door>(*entity)` and keeps that guard alive while it calls `world.get::<TwoStateActivator>(*entity)`. `door_motion_system` (`byroredux/src/systems/door.rs:143`) acquires the pair through `query_2_mut::<TwoStateActivator, Door>()`. The two orders form a cycle: `Door → TwoStateActivator → Door`.

The command came in with `180080de7` (#7059). `4288659fa` had just restored the door lock order for the prompt and motion paths, so this re-opens the same pair from a third caller.

## Evidence
`BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux -- door` gives 31 passed and 8 failed:

- `interaction::tests::door_prompts_and_motion_share_the_lock_order`
- 7 tests in `systems::door::tests`: `an_unlocked_unblocked_door_toggles`, `a_locked_door_admits_only_an_activator_that_carries_its_key`, `a_script_blocked_door_stays_shut_but_a_native_activation_toggles_it`, `a_door_plays_the_open_sound_opening_and_the_close_sound_closing`, `a_transition_plays_the_matching_clip_and_settles_on_its_last_frame`, `a_non_door_transition_is_left_alone`, `a_door_at_rest_in_a_state_its_pose_does_not_show_snaps`

```
ECS cross-thread deadlock risk (lock-order cycle): attempted acquisition of `byroredux::components::door::Door` while holding `byroredux_scripting::vm_state::TwoStateActivator` on this thread — that closes a cycle over the acquisition orders already observed across threads: `byroredux::components::door::Door` → `byroredux_scripting::vm_state::TwoStateActivator` → `byroredux::components::door::Door`.
```

All 39 pass without the detector. The cycle closes only against the `Door → TwoStateActivator` edge that `commands::door::tests` records, so the `lock-order-check` CI lane should be red on this.

## Suggested fix
Read the activator state before taking `Door`. `state` is a `&'static str`, and its guard drops at the end of the `map_or`, so swapping lines 51 and 52 removes the nested acquisition. Snapshotting the `Door` fields and dropping its guard first would also work. Keep `door_prompts_and_motion_share_the_lock_order` as the regression guard. If the command should be pinned by name, add a `door.state` call to that test.

Found by `/audit-sync` (concurrency lane measurement, 2026-11-20).

