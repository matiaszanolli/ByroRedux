# #7351: SAVE-06: nothing pins `restart_day_blocks_at_loaded_clock` in the drain

- **Severity**: LOW
- **Dimension**: Live Load-Apply
- **Data-Loss Class**: none
- **Location**: `byroredux/src/save_io/live_reload_tests.rs` (`the_day_blocks_restart_before_the_reload_and_survive_an_aborted_one`, `post_reload_drain_steps_keep_their_order`), `byroredux/src/save_io/live_load.rs:843`
- **Status**: NEW (test-gap)
- **Description**: `restart_day_blocks_at_loaded_clock` has a unit test on the function (`package_flags.rs:466-480`) but no test that the drain calls it, after the wholesale `restore_resources` (it reads `GameTimeRes`, restored there) and before `reseat_ambient_packages_after_restore`. Deleting line 843, or moving it above line 836, leaves the whole save_io suite green (205 passed, 4 ignored real-master). `post_reload_drain_steps_keep_their_order` does not list it.
- **Impact**: the #7341 regression (blocks dated in the future after loading an earlier save) can return silently.
- **Suggested Fix**: add `"package_flags::restart_day_blocks_at_loaded_clock(world)"` to the `steps` array of `post_reload_drain_steps_keep_their_order`, between `restore_resources` and `restore_resident`.

---
- **Dimension agent ID**: SAVE-D5-04

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other parked-state paths, load-drain steps, notification callers and `atomic_write` callers the finding names)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_SAVE_2026-11-23.md`
