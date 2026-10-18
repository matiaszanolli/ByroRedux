# SCR-D5-2026-10-17-07: Scene-trigger router holds a `QuestStageState` read guard across `evaluate_condition_list` (recursive read)

Issue: #6008
Labels: low, scripting, bug, concurrency

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-07: Scene-trigger router holds a `QuestStageState` read guard across `evaluate_condition_list` (recursive read)

- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `byroredux/src/systems/cinematic.rs:979-1053` (`match (..., world.try_resource::<QuestStageState>(), ...)` keeps `stages` alive for the arm; `evaluate_condition_list` at :1048 re-acquires it for GetStage-family conditions)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The same recursive-read shape 35f1cf051 removed from the Story Manager ("a second read guard on one thread can deadlock behind a queued writer, #2386"). Here the system is `add_exclusive`, so no writer can queue (hence LOW), but the lock tracker's once-per-acquisition recursive-read warning fires and the pattern is one non-exclusive registration away from a real deadlock.
- **Suggested Fix**: copy the few `QuestStageState` facts the router needs (`is_running`, `get_stage`, `get_stage_done`) into a snapshot before the loop, or evaluate the advance conditions after dropping `stages`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and `BYRO_LOCK_ORDER_CHECK=1` lane stays green
- [ ] **TESTS**: A regression test pins this specific fix
