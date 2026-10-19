# PHYS-SD-02: A Starts-Dead actor's ragdoll is built (awake) before the cell's later-in-file collision exists, so a corpse resting on late-ordered statics falls past them

Labels: medium,physics,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6133

**Severity**: MEDIUM · **Dimension**: Ragdoll start at NPC spawn (Dim 3 contract with the streaming apply)

## Location
`byroredux/src/cell_loader/references/mod.rs` (REFRs applied in file order, actors inline; `apply_starts_dead`), `byroredux/src/cell_loader/reference_state.rs` (parked `dead` row restore), `byroredux/src/combat.rs` (`reconcile_pending_dead_actors_system` -> `activate_ragdoll`), `crates/physics/src/ragdoll.rs` (`RigidBodyBuilder::dynamic()`, `pw.wake()`)

## Trigger Conditions
An exterior cell applied in budgeted slices (no loading cover, so `dt != 0`) with a Starts-Dead ACHR whose supporting statics (platform, table, bed, rampart piece, rock outcrop) come LATER in the cell's REFR order than the ACHR and are not part of the terrain. The same applies to a corpse restored from a parked `dead` row.

## Description
`apply_starts_dead` queues the reconciliation as soon as the actor job completes; the Late sink runs on the next scheduler pass and `activate_ragdoll` builds the ragdoll from the bone `GlobalTransform`s. The bodies are dynamic and awake and the world is woken. Nothing gates the activation on the rest of the cell's collision being registered: the cell apply job is still pending (references remaining, `budget.should_yield()`), so statics later in the list spawn on later frames and register one frame after that. The corpse integrates gravity meanwhile (686.7 BU/s^2: about 10 BU of fall after 10 frames, about 340 BU after one second; a cell apply routinely spans several frames). Terrain is always present first (terrain and water are spawned before references), so the body ends on the terrain or on whatever registered earlier; the late static then appears under or through it. Interior transitions are not affected: `dt` is 0.0 while the loading cover is up and the interior apply completes before the cover lifts.

## Evidence
The awake/wake facts are in `ragdoll.rs`; the ordering fact is the single `while job.next_ref < refs.len()` loop in `references/mod.rs` that spawns actors and statics in one pass. Rapier's begin-touch wake and the sleep re-settle only help once something is touching; a body that already passed the late floor stays below it. Corpse ordering frequency was not censused (needs game data).

## Impact
Visible mis-placement of authored corpses (lying under or beside the structure they were placed on) in exterior ring cells, and for interpenetration cases a depenetration impulse on the ragdoll. Not a crash: the explosion ladder bounds it.

## Related
#5015 (XRGD decode), GAME-SD-03 (#6073, corpse pose not carried across eviction - different mechanism), #4814, #5267.

## Suggested Fix
Hold the queued reconciliation for an actor until its cell's apply job has completed (the owning `ExteriorCellApplyJob` / `ReferenceLoadJob` knows when), or build the ragdoll asleep and wake it on the first contact the way placed clutter spawns.

Source: docs/audits/AUDIT_PHYSICS_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
