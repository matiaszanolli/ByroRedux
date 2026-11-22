# #7227: GAME-2026-11-22-04: `place_character_body` clears the controller's momentum but not `FallTracker`, so a quick-load or teleport mid-fall applies fall damage on landing

**Labels**: medium,gameplay,combat,save-load,bug,game:oblivion

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-04**), audited at HEAD 2b40f76b6.

- **Severity**: MEDIUM
- **Dimension**: 7 (state re-derived after a load)
- **Domain**: combat · **Game**: Oblivion (the only profile with a `FallModel`)
- **Trigger**: An in-process load (F9 / `save.load`), or any `place_character_body` caller (door arrival, `combat.approach`, `cam.pos` with the body pinned), while the player is airborne below their takeoff height, with the new pose within `TELEPORT_DISTANCE` (400 BU) of the previous frame's. Typical case: quicksave at a ledge, jump off, F9 mid-fall.
- **Location**: `byroredux/src/systems/character.rs:611-613` (`FallTracker::step`), `:712-714` (`fall_landing`), `:991-1022` (`place_character_body`); `crates/core/src/character/fall.rs:74,97-125`; `byroredux/src/save_io/live_load.rs:480` (drain); `registry_completeness_tests.rs:463`
- **Status**: NEW (#7056, closed, introduced the tracker)
- **Description**: `FallTracker` is a process-lifetime resource holding the takeoff height and the seconds spent below it. It forgets a fall only on a one-frame move over 400 BU, a swim or a landing. `place_character_body` is the documented single teleport seam and calls `clear_momentum()`. But it never resets `FallTracker`, and neither does the load drain or the pose restore (`save_io/player.rs:246`). The allowlist reason ("a load starts with the body unplaced and no fall in flight") holds only for a cold start.
- **Evidence**: The first grounded frame after the placement returns `Some(self.seconds)`, which `fall_landing` turns into `FallModel::damage` and applies through `apply_player_drowning_damage`. At Oblivion's defaults (`time_min` 0.2 s, 80/s, skill factor 1.25), 0.5 s of pre-load fall costs about 50 Health. `grep FallTracker` finds no reset anywhere.
- **Impact**: Unearned damage, plus an Acrobatics skill use, right after a quick-load of a save taken on solid ground. It can kill a low-Health character.
- **Related**: #7056, #5591
- **Suggested Fix**: Reset the tracker inside `place_character_body`, next to `clear_momentum()`, so every teleport path shares it. Reword the allowlist reason. Add a test beside `place_character_body_moves_both_transforms_and_clears_momentum`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
