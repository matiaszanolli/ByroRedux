# #6266: SCR-2026-10-21-09: `ChallengeEvent::ItemAcquired` has no producer, so *Acquire Item* challenges never progress

**Labels**: low,scripting,gameplay,bug,game:fnv

**Source**: docs/audits/AUDIT_SCRIPTING_2026-10-21.md · **Severity**: LOW · **Dimension**: Legacy ObScript · **Location**: `crates/scripting/src/challenges.rs:134`, `:233-234`; producers exist only for `ItemUsed` (`byroredux/src/inventory.rs:1589`) and `Kill` (`obscript_state.rs:514`)

**Audit status**: NEW

## Description

The module doc lists the challenge types with no source (damage, hit, skill, craft, weapon-keyed kills) but not the acquire types. Those have a matched event variant and no producer, so they silently sit at 0.

## Related

ESM-2026-10-21-04 (CHAL decoded with the FNV schema everywhere; the decode side of the same table).

## Suggested Fix

Raise `ItemAcquired` from the player's inventory-add path, or list the acquire types under "no source" in the module doc and delete the variant.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other ChallengeEvent variants and challenge kinds with no producer)
- [ ] **TESTS**: A regression test pins this specific fix
