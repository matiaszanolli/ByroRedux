# #7337: GP-01: `PlayerControlState.fighting_enabled` / `activation_enabled` have no consumer, so a script's `DisablePlayerControls` does not stop the player attacking, blocking, firing, casting or activating

- **Severity**: MEDIUM
- **Dimension**: 4 (combat input) / 6 (player flow)
- **Location**: `byroredux/src/combat.rs:199-203` (`can_swing` = Character mode + `player_can_act`), `byroredux/src/combat.rs:241` (`state.blocking = block_held`), `byroredux/src/systems/character.rs:91-103` (`player_can_act`), `byroredux/src/bow.rs:255`, `byroredux/src/player_cast.rs:36`, `byroredux/src/interaction.rs:1755`; writer `crates/scripting/src/player_control.rs:105-125`; the only reader `crates/scripting/src/obscript_more.rs:445-446`
- **Status**: NEW (no open or closed issue; #2292 only saved the state; not in the 2026-11-15 / 11-22 reports)
- **Trigger**: any game, any cell where a script calls `DisablePlayerControls` (Skyrim `Game.DisablePlayerControls()` with its CK defaults, ObScript `DisablePlayerControls`, FO4/SF `InputEnableLayer.DisablePlayerControls`). Examples named in the repo: Skyrim `QF_MQ101_*` (`SetInChargen; DisablePlayerControls; ...`), FNV VCG chargen. Save state: any, the control state is saved.
- **Description**: `PlayerControlState::apply_selection` stores every control domain the script selects, and the module header says "lets the app input/controller layer consume the same state". Only `movement_enabled` (`player_accepts_movement_input`, `player_seat`), `looking_enabled` (`app_events.rs:1507`) and `sneaking_enabled` (M65 `systems/stance.rs`) have a consumer in the binary. `fighting_enabled` and `activation_enabled` (and `menu_enabled`) are read only by ObScript `GetPlayerControlsDisabled`. `DISABLE_PLAYER_CONTROLS_DEFAULT` sets `fighting: true, activation: true`, so a bare `Game.DisablePlayerControls()` is meant to lock attack, block, ready-weapon and activate.
- **Evidence**:
  - `rg 'fighting_enabled|activation_enabled' byroredux crates --glob '!*test*'` returns the field declaration, `apply_selection`, and `obscript_more.rs:445-446` (`GetPlayerControlsDisabled`) only.
  - `combat_input_system`: `let can_swing = world.try_resource::<PlayerMode>()...== Character && crate::systems::player_can_act(world);` and `player_can_act` is `Dead` + VATS only.
  - `state.blocking = block_held;` is written from the raw key every frame, and `target_block` reads `MeleeState.blocking`, so a locked-out player still resists damage and banks Block.
  - Same shape in `player_bow_system`, `player_cast_system`, `iron_sights_system` (all gate on `player_can_act`) and `interaction_system` (`interaction.rs:1755`).
  - `InputLayers` (`Control::Fighting`, `Control::Activate`) folds into the same fields (`input_layers.rs:147`), so the FO4/Starfield layer path is equally unread. `docs/engine/scripting.md:1352` lists only VATS/favorites/running/jumping/sprinting/ZKey/FastTravel as "nothing reads them yet", so this is not a declared gap.
- **Impact**: during every scripted sequence that locks the controls, the player can still swing, shoot, cast, block and press activate. The sequence can break (an NPC killed or turned hostile mid-scene, a quest object activated out of order), and `GetPlayerControlsDisabled` answers 1 while the player demonstrably can fight, which tutorial scripts poll.
- **Related**: M65 wired `sneaking_enabled` the same way (PRD sneak-and-detection row 2, "read only by GetPlayerControlsDisabled" then); #6469 / #6350 wired `Control::Vats`; routed to /audit-ui: `menu_enabled`.
- **Suggested Fix**: add one `player_may_fight(world)` helper (PlayerControlState.fighting_enabled and the `Control::Fighting` layer) beside `player_can_act` and apply it to `combat_input_system` (swing, ready-weapon, `blocking`), `player_bow_system`, `player_cast_system`, `iron_sights_system`; gate `interaction_system`'s activate edge on `activation_enabled`. Add one guard test per consumer.
- **Dimension agent ID**: GP-D4-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other action systems / movers / park paths / add paths named in the finding)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the system's `Access` row matches
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_GAMEPLAY_2026-11-23.md`
