# #5874: GAME-D6-2026-10-15-01: Level-ups, XP, caps and non-silent `AddItem` give the player no feedback

- **Labels**: low,bug,gameplay,ui
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Location**: `byroredux/src/combat.rs:619-690` (`grant_experience` logs only); `byroredux/src/commands/gameplay.rs:352` (`levelup` is the only reader of banked `LevelUpPoints`); `crates/scripting/src/fragment/effects.rs:366-378`
- **Status**: NEW (from `38539eee8` and `722421899`)
- **Description**: The production `notifications::push` producers are interaction, inventory, the barter refusal and live-load. Progression events reach only the log, and banked level-up points can only be spent from the console.
- **Suggested Fix**: Push level-up, XP, caps and item-added notices through `PlayerNotifications`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
