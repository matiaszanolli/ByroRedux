# #7341: GP-05: Once Per Day blocks the reload's spawns note carry the OUTGOING session's clock, so loading an earlier save suppresses the package for days (#7252 half-applied)

- **Severity**: LOW
- **Dimension**: 7 (gameplay-state coverage, load drain)
- **Location**: `byroredux/src/save_io/live_load.rs:628-641` (`reload_with_fresh_day_blocks`), `:836` (post-reload `restore_resources`), `:43-54` (`PRE_RELOAD_RESOURCES` has no `GameTimeRes`); `byroredux/src/npc_spawn/package_flags.rs:139-143` (`now_hours`), `:190-205` (`note_selected`)
- **Status**: NEW (follow-up to closed #7252 9f31ab66f)
- **Domain**: ai, save-load · **Game**: FNV, FO3, Oblivion (and Skyrim+, since the flag is a PKDT bit on every game)
- **Trigger**: An in-process load (quickload or the load menu) of a save whose clock is EARLIER than the live session's, in a cell holding actors that spawn on a Once Per Day package, with that package's window open at the outgoing hour.
- **Description**: #7252 moved the block clear to before the reload so the reload's own spawns keep their blocks. The reload runs `apply_ai_package_behavior` under the OUTGOING `GameTimeRes`: `ensure_game_time` inserts only when absent, and `GameTimeRes` is restored by the later wholesale `restore_resources`, not pre-reload. So `note_selected` stamps `first = T_out` (outgoing day*24+hour). After the restore the clock is `T_saved`. `reseat_ambient_packages_after_restore` re-notes only winners that changed, and `note_selected` uses `or_insert`, which does not restart a block whose `now - first` is negative. With `T_saved < T_out` the check `now - first < 24` stays true for `(T_out - T_saved) + 24` game hours. Once the package's window closes it cannot win again until the clock reaches `T_out + 24`. The same `first` is wrong in the other direction after loading a later save, but harmlessly (the block just lapses early). The cold-start path has the same wrong-clock stamp, but its boot clock is near zero so the block is already expired; only the in-process path produces a future-dated block.
- **Evidence**:
  ```rust
  // live_load.rs:774-791  without_parked_state(.. reload_with_fresh_day_blocks(.., reload_*_session))   // clock = outgoing
  // live_load.rs:836      byroredux_save::restore_resources(..)                                         // GameTimeRes -> saved clock, after
  // package_flags.rs:204  let first = blocks.0.entry((owner, package)).or_insert(now);  if now - *first >= 24.0 { *first = now }
  ```
- **Impact**: A player who plays 10 days and quickloads a day-4 save finds every Once Per Day actor selected at spawn suppressed for about 8 in-game days after its first window ends. The visible effect is an NPC skipping a daily visit.
- **Related**: #7252, #7317, #7230, AIP-FLG-001.
- **Suggested Fix**: Rebase after the restore: when `GameTimeRes` is restored, set every surviving block's `first` to the restored `now_hours` (a block restarts at load, as the NOT_SAVED reason states). Or re-note the unchanged winners in `reseat_ambient_packages_after_restore`. Add a test that reloads with the live clock ahead of the saved one.

---
- **Dimension agent ID**: GP-D7-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other action systems / movers / park paths / add paths named in the finding)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the system's `Access` row matches
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_GAMEPLAY_2026-11-23.md`
