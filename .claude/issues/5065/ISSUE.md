# #5065 — SCR-D4-2026-09-29-01: A scripted Enable()/Disable() on a resident reference only writes the ledger — since #4813, a quest stage's Enable() no longer materialises the reference until its cell reloads

**Labels**: medium, bug, scripting, quests

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-29.md` — finding `SCR-D4-2026-09-29-01`

**Severity**: MEDIUM

**Dimension**: Attach & Reference Identity

**Untrusted-Input**: No

**Location**:
- `crates/scripting/src/fragment/effects.rs:932-963`: the arm pushes to `deferred.reference_enable_changes`.
- `crates/scripting/src/fragment/effects.rs:355-360`: the deferred apply is `ReferenceEnableState::set_enabled` only.
- `byroredux/src/cell_loader/spawn.rs:712`: a disabled root gets `PlacementContentWithheld` and no mesh or collider.
- `byroredux/src/cell_loader/references/mod.rs:656-676`: a disabled actor becomes an identity stub, with no NPC job.
- `byroredux/src/components.rs:91-102`: the doc says "there is no live re-spawn".
- `byroredux/src/interaction.rs:1345-1357`: "its meshes only go on the next load".

**Status in report**: NEW. There is no open issue. #3278, #3489, #4698, #4813 and #4820 are all closed, and #4820's suggested fix
defers the work "until live re-spawn exists". Nothing tracks that re-spawn.

## Description

before #4813, the authored "Initially Disabled" flag was ignored. References that a quest later
`Enable()`s were therefore already (wrongly) present. #4813 now withholds them correctly, and #4820 blocked interaction
with them rather than re-spawning. No system consumes the enable edge: `ReferenceEnableState` has no reader besides the
spawn gate, interaction, and save. The lowering and the ledger are correct; the missing piece is the live consumer.

## Impact

the quest-gated references in the #4813 census stay absent after the stage fragment that enables them has
run, until the cell reloads. Examples: Skyrim `HelgenKeep01` actors, the MS10 pirates, `MQ106DragonParchment`, and the
FNV `Vault11c` turrets. An alias bound to such a stub drives scenes and packages with a bodiless actor that has only a
transform. The reverse also holds: a live `Disable()` leaves the reference rendered and solid until reload.

## Related

#4813, #4820, #3278, #3489.

## Suggested Fix

on a resident reference's enable edge, run that REFR's placement spawn (or restore the withheld
content). On a disable edge, strip the mesh, collider and volume from the root, keeping the identity (the #3278 posture).
At minimum, record the deferral on ROADMAP M47 so it is visible.

Validated at HEAD 9fcfdc3fc: the deferred apply in `crates/scripting/src/fragment/effects.rs` only calls `ReferenceEnableState::set_enabled`; `ReferenceEnableState` readers in the binary are the spawn gate (`cell_loader/spawn.rs`), `interaction.rs` and save; `byroredux/src/components.rs` still documents that there is no live re-spawn.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the `quest_advance` demo path that also writes `ReferenceEnableState`)
- [ ] **TESTS**: A regression test pins this specific fix (Enable() on a withheld resident reference restores its mesh/collider without a cell reload)
