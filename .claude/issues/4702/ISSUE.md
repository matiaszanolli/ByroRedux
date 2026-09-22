# GAME-D4-2026-09-21-04: SDK actor-value batches can drop Health to <= 0 without the death transition

**Issue**: #4702
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: 4 — Combat & Death (single damage path)
**Location**: `byroredux/src/extensions/commands.rs` (`apply_pending_actor_value_writes`)

## Description
The extension host commits `Damage`/`SetBase`/`ModifyPermanent` directly into `ActorValues` via `*target = values;`, validating only finiteness. No `Dead` insertion or `queue_dead_actor_reconciliation` call anywhere in the file.

## Evidence
`grep -n "Dead\|queue_dead_actor_reconciliation" byroredux/src/extensions/commands.rs` returns nothing.

## Impact
An extension damaging an actor to zero leaves it alive: AI keeps running, not lootable, no ragdoll until an unrelated HitEvent. Same gap applies to the player.

## Related
GAME-D4-2026-09-21-03 (#4701); #3119; `/audit-tooling` owns the SDK surface generally.

## Suggested Fix
After commit, for each entity whose Health AV is now <= 0 and not `Dead`, insert `Dead` and call `queue_dead_actor_reconciliation` (the water-damage pattern).
