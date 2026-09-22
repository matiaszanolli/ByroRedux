# AUD-2026-09-21-D5-01: Player swing sound queued before a DraugrCombatAnim early return — no P2 combat sound can fire on Skyrim, not even the swing

**Issue**: #4742
**Filed**: 2026-09-22 (audit-publish, AUDIT_AUDIO_2026-09-22.md)

**Severity**: MEDIUM
**Dimension**: 5 — Engine Consumers
**Location**: `byroredux/src/systems/combat_anim.rs:126-159` (push + early return), `:329-337` (drain); `byroredux/src/npc_spawn/resumable.rs:1079-1084`; `crates/core/src/ecs/world.rs:474-476`

## Description
`combat_feedback_system_inner` pushes the player's swing position into `scratch.swings`, then hits `let Some(anim_q) = world.query::<DraugrCombatAnim>() else { return; };` — this early return exits the whole function, skipping the swing drain further down. Since #4700 means `DraugrCombatAnim`'s storage is never registered on Skyrim, the query returns `None` and the swing is never dispatched. **This corrects #4700's own claim that "only the player swing sound … can fire": on Skyrim today, none of the P2 combat sound family plays, not even the swing.** After #4700 is fixed, the first Draugr spawn would instead dump a burst of stranded, stale-position swing one-shots (unbounded `scratch.swings` growth today).

## Evidence
`git grep -n DraugrCombatAnim` → definition, one insert site (`resumable.rs:1082`), no test reaches the swing drain. `fc825a6cd` (#4605, closed) touched this function but only rescoped a resource guard — the query/return/drain ordering is untouched.

## Impact
Skyrim player melee is silent on the shipped profile route today. Unbounded scratch growth. Latent audible burst once #4700 lands without this fix.

## Related
#4700 (GAME-D4-2026-09-21-02, open — root cause of missing marker; corrected here) — comment posted cross-linking this issue; #4605 (closed, unrelated fix in same function); #4743, #4744, #4745 (this report, same area).

## Suggested Fix
Drain `scratch.swings` before the marker query; let a missing `DraugrCombatAnim` storage skip only the per-actor loop. Add a test with a persistent scratch, player swing pushed, no marker storage registered, asserting the drain still empties `scratch.swings`.
