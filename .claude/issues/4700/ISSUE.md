# GAME-D4-2026-09-21-02: The P2 combat tail (Draugr attack/hit/death takes, impact sound, death voice) never fires — its marker is only inserted on the Oblivion/FO3/FNV spawn path

**Issue**: #4700
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: 4 — Combat & Death
**Location**: `byroredux/src/npc_spawn/resumable.rs` (runtime path marker insert; prebaked finalize has no insert); `byroredux/src/cell_loader/load.rs` (#4551 fix/pin)

## Description
`DraugrCombatAnim` is inserted only in `RuntimePhase::Finalize`, serving only `has_runtime_facegen_recipe()` games (Oblivion, Fallout3NV). Draugr are Skyrim-only and spawn via `PrebakedPhase::Finalize`, which never inserts the marker. `combat_feedback_system` gates every take/sound on it.

## Evidence
`rg 'DraugrCombatAnim::default'` has one production hit, inside `RuntimePhase::Finalize`. `PrebakedPhase::Finalize` read in full — no insertion.

## Impact
The P2 combat-feel goal is dead in production on its only target content (Skyrim BleakFallsBarrow01 Draugr).

## Related
#4551 (closed; installed clip resource but "regardless of path" premise false for Skyrim); GAME-D4-2026-09-21-05 (#4700-adjacent, latent defects this fix would expose); GAME-D7-2026-09-21-02 (#4705, marker also unclassified for saves).

## Suggested Fix
Compute `combat_anim_draugr` from resolved race in `prepare_prebaked_state`; insert in `PrebakedPhase::Finalize`.
