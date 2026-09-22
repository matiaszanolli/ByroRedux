# GAME-D4-2026-09-21-05: (latent) The death take re-inserts an AnimationPlayer over the ragdoll, and its once-only latch replays on every respawn and reload

**Issue**: #4708
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: LOW — latent until GAME-D4-2026-09-21-02 (#4700) is fixed; must be fixed with it
**Dimension**: 4 — Death reconciled once
**Location**: `byroredux/src/systems/combat_anim.rs`; `byroredux/src/combat.rs`; `byroredux/src/npc_spawn/resumable.rs`; `docs/engine/p2-combat-anim-sound-fixture.md`

## Description
(a) Same-frame as death, `reconcile_dead_actor` removes AnimationPlayers and activates the ragdoll; `combat_feedback_system` then re-inserts an AnimationPlayer over it. (b) `death_played` lives on unsaved/unclassified `DraugrCombatAnim`, re-inserted as `default()` at every spawn — a reloaded corpse replays its death take/voice.

## Evidence
Death decision fires on `dead && !death_played`. `DraugrCombatAnim::default()` inserted at every spawn. Save rows record only `Dead`.

## Impact
Once #4700 lands: every reload/revisit of a Draugr crypt replays each corpse's death scream.

## Related
GAME-D4-2026-09-21-02 (#4700, must land together); #3022; #3708.

## Suggested Fix
Derive the latch from `Dead` at spawn/restore (insert with `death_played: true` when already dead). Sequence the clip before ragdoll activation, or skip it when a ragdoll exists.
