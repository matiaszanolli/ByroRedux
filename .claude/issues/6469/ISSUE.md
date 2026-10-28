# GAME-D4-2026-10-27-02: `vats.enter` claims "same gates as the key" but skips the fly-cam and `Control::Vats` gates; `vats.scan`/`queue`/`confirm` have no mode gate

Labels: low, gameplay, combat, bug
Source: docs/audits/AUDIT_GAMEPLAY_2026-10-27.md

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-27.md` finding `GAME-D4-2026-10-27-02` (LOW)
**Location (re-mapped to current tree)**: `byroredux/src/commands/vats.rs` (`VatsEnterCommand`, ~:13-52) vs `byroredux/src/systems/vats.rs` (`vats_mode_system` entry gates ~:48-66)

**Severity**: LOW (debug/console surface only). Dimension: Combat & Death (Dim 4, VATS plumbing). From `818eba7f4`, `7eb4de261`.

## Description
The command doc says "same gates as the key: model, living player, enabled control domain". The command checks the model and `player_can_act`, but not `PlayerMode != FlyCam` and not `control_vats_enabled`. `player_can_act` returns true when there is no player at all (`is_none_or`). So in fly-cam `vats.enter` sets `TargetSelect`: `dt` pins to 0, the world freezes, and the scan is empty. A script's `DisablePlayerControls`-style VATS disable is also ignored on the smoke gate-of-record surface. `vats.scan`, `vats.queue` and `vats.confirm` work with `mode == None`, so a dead player can land VATS hits from the console.

## Evidence
`vats_mode_system` gates: `VatsModel` present, `player_can_act`, `PlayerMode` not FlyCam, `control_vats_enabled`. `VatsEnterCommand::execute` checks only `VatsModel`, "already active" and `player_can_act` (verified at HEAD).

## Impact
Debug/console surface only. The gate list is spelled twice and has already diverged.

## Related
#6350 (VATS tracker), SAVE-2026-10-27-01 (VATS state across live load), #6429.

## Suggested Fix
Extract the entry gates into one `vats_entry_refusal(world) -> Option<&str>` shared by `vats_mode_system` and `vats.enter`; have `vats_queue_part` and `vats_confirm` refuse when `mode == None`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling systems
- [ ] **TESTS**: A regression test pins this specific fix

