# PRDR2-2026-11-03-07: VATS stays active across a save load, a cell change and player death: the world stays frozen and the queue targets the old cell

Labels: bug, low, gameplay, combat, game:fnv, game:fo3, game:fo4

**Severity**: LOW (recoverable by pressing the VATS key, but the world stays frozen in the new cell and a confirm acts on entities of the old one)
**Dimension**: Gameplay / VATS
**Source**: found by the vats PRD review round 2 (2026-11-03; the PRD now states the requirement as VATS-MODE-003); re-verified at HEAD `eba979dfb`
**Status**: NEW

## Location
- Exits that exist: the VATS key toggle (`byroredux/src/systems/vats.rs:39-46`), `vats.exit` (`vats_cancel`, `:746-749`) and a completed confirm (`:738-740`).
- No other code touches `VatsState`: nothing in `save_io/`, `cell_loader/`, `streaming/` or the death path resets it. `VatsState` is not saved (`save_io/registry_completeness_tests.rs:459`, "transient … dies with the session"), but an in-process load does not recreate the resource.

## Description
VATS stays active across:
- **a save load** (F9 quickload, pause-menu load, `load`): the new world opens in `TargetSelect` with `world_time_scale() == 0.0` (`components/vats.rs:82-88`), so the loaded world is frozen, and `targets` / `queue` hold `EntityId`s from the unloaded cell;
- **a cell change** (`coc`, a scripted `MoveTo player` or door teleport during playback): same state;
- **player death** while a mode is active (no `Dead` check after entry; `vats_entry_refusal` only gates entering).

A confirm after any of these charges AP for the stale queue and inserts `HitEvent`s on despawned entity ids (ids are never reused, so no live entity is hit, but the AP is spent and the event rows are orphaned).

## Impact
After a quickload from VATS the game looks hung (world frozen) until the player presses the VATS key again; AP can be spent on nothing.

## Suggested Fix
Call `exit_all` (dropping the reservation the same way the key toggle does) on player death, on every cell transition and on every in-process save load. The vats PRD (`docs/prd/vats.md`, VATS-MODE-003) states the rule; cite it in the commit with `Satisfies VATS-MODE-003` once that PRD is approved, and do not widen the change beyond the three exits.

## Completeness Checks
- [ ] **LOAD**: an in-process save load leaves `VatsState` at `VatsMode::None` with empty targets/queue
- [ ] **CELL**: a cell transition does the same
- [ ] **DEATH**: player death while in any VATS mode exits it
- [ ] **AP**: no AP is charged for a queue that did not execute
- [ ] **TESTS**: one test per exit in `systems/vats.rs`'s test module

