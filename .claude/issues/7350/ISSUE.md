# #7350: SAVE-05: #7341 restarts every block the reload noted, including those of packages the re-seat then discards

- **Severity**: LOW
- **Dimension**: Live Load-Apply
- **Data-Loss Class**: none (ambient AI timing; nothing saved is lost)
- **Location**: `byroredux/src/npc_spawn/package_flags.rs:118-125` (`restart_day_blocks_at_loaded_clock`), `byroredux/src/save_io/live_load.rs:843`, `byroredux/src/npc_spawn/ai_package.rs:1232-1235` (`note_selected` at spawn)
- **Status**: NEW (refinement of the closed #7341 fix)
- **Description**: The reload's spawns note a block for the Once Per Day package they pick under the OUTGOING clock (`apply_ai_package_behavior`, `ai_package.rs:1213-1235`). `restart_day_blocks_at_loaded_clock` then sets EVERY held block to the loaded clock. `reseat_ambient_packages_after_restore` runs at the loaded clock afterwards; when the loaded hour is outside that package's window it installs another winner and the noted package stays blocked, now for a full 24 hours from the load (`once_per_day_blocked` exempts only `current == Some(package)`). Example: the reload spawns an actor at 09:00 under its Once Per Day breakfast package (block noted), the save is 20:00; the re-seat installs the evening package and the breakfast block now runs until 20:00 the next day, so the actor skips tomorrow's breakfast. The saved session had run breakfast that morning (blocked until about 08:05 next day) and a cold start from the same save would have the spawn-time block lapse as the clock jumps ahead. Every other check in the claim holds: at the restart moment all held entries are the reload's own (`reload_with_fresh_day_blocks` took the outgoing map, no other writer runs between the reload and line 843, and no path in the drain before it can leave the blocks under a different clock; an aborted or failed load before line 836 keeps the outgoing clock and the reload's blocks together).
- **Impact**: one skipped daily package per affected actor after a load, an ambient behavior difference only.
- **Related**: #7252, #7340, #7341.
- **Suggested Fix**: keep only the blocks of the package each actor is running after the re-seat (drop the rest), or restart a block only when its package is still the actor's `active_package_form_id` after the re-seat.
- **Dimension agent ID**: SAVE-D5-03

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other parked-state paths, load-drain steps, notification callers and `atomic_write` callers the finding names)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_SAVE_2026-11-23.md`
