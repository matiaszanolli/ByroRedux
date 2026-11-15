# #6928 — INC-2026-11-14-16: Battle intro/outro one-shots bypass suspension; the NV outro plays after a death sting or during a movie

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-16). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: LOW
- **Location**: `byroredux/src/music/controller.rs` (`step` calls `step_battle` before suspension; `step_battle`, `one_shot`, `sting`)
- **Changed in**: `controller.rs` (commits `45b3b0a64`, `ad06d54b0`)

## Description
`step_battle` runs before suspension is computed and plays the intro/outro on the Battle one-shot voice directly. The death sting stops Battle's voices but keeps its directive; `in_combat` goes false, so `battle_hold` counts down and the outro starts. Suspension only pauses voices when a layer's suspended state *changes*, and Battle is already suspended by the sting, so the new outro voice is never paused. Hold (a movie) and Script-layer suspension are bypassed the same way.

## Impact
A stray battle outro a few seconds after the player's death, or during a movie, where the original stops all music.

## Related
#6885 (NV battle sets, closed), #6901

## Suggested Fix
Skip the one-shot when `hold || silenced || script_holds`, or clear the Battle directive in `sting()`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
