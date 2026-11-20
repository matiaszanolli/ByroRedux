# Door Runtime

**Status**: DESIGN (Phase 0, 2026-11-19). The design of the promise in the [Door Runtime PRD](../prd/door-runtime.md) (tracking #7057; Phase 0 is #7058). It records the motion census, what the sources settle and what they do not, and the shape the later phases build. Nothing here is built yet.

**Scope**: an in-cell door, a `DOOR` placement with no teleport. Load doors and what an actor does at a door are out of it ([Cross-cell actor travel](cross-cell-travel.md); the AI packages PRD's AIP-DOOR).

## 1. Census: how in-cell doors move

`door_motion_census` (checked in at `byroredux/examples/door_motion_census.rs`, PRD DRT-REC-003) reads every `DOOR` base that is placed with no teleport in an interior cell, finds its model in the mesh archives and classes it by what the model authors. Measured 2026-11-19 over the vanilla masters:

| Game | In-cell door bases | `sequence` bases / placements | `graph` bases / placements | `none` bases / placements |
|---|---|---|---|---|
| Oblivion (with Shivering Isles meshes) | 204 | 135 / 1,745 | 0 / 0 | 69 / 411 |
| Fallout 3 | 146 | 134 / 2,014 | 0 / 0 | 12 / 36 |
| Fallout: New Vegas | 137 | 109 / 1,379 | 0 / 0 | 28 / 99 |
| Skyrim SE | 99 | 92 / 1,523 | 1 / 43 | 6 / 18 |
| Fallout 4 | 164 | 161 / 2,368 | 0 / 0 | 3 / 7 |
| Starfield | 235 | not read (see below) | not read | 231 / 6,293 (4 models not in the archives given) |

```bash
cargo run --release -p byroredux --example door_motion_census -- <Master.esm> <meshes archive>...
# Oblivion: Oblivion - Meshes.bsa and DLCShiveringIsles - Meshes.bsa; Skyrim: Skyrim - Meshes0.bsa and Meshes1.bsa;
# Fallout 4: Fallout4 - Meshes.ba2.  DOOR_CENSUS_LIST=1 lists every base; DOOR_CENSUS_CELL=<cell> lists one cell's doors.
```

What the table says:

- **`sequence` is the door.** In every game with a census, nine in ten in-cell door placements or more author a `NiControllerManager` whose sequences are named `Open` and `Close` (the clip names over the `sequence` bases: `open` and `close` on all but two or three bases per game). The motion is authored data; the engine does not invent it.
- **`graph` is not a door class today.** The only base is a Skyrim hinge trap (`Traps\HingeTrap\TrapHingeTrigger01.nif`, 43 placements). No in-cell door names a behaviour graph. The PRD's `Graph event` class stays in the vocabulary for a game that needs it and has no work in it now.
- **`none` is a minority and mostly not a swinging door.** The listed bases are ladders and ladder exits, trapdoors, hatches, manholes, elevator doors, static door frames, and a few hinged doors that author nothing (Oblivion's `ICDoorInt01`, `CastleDoor02`, `FarmHouseDoor01`; Skyrim's `FarmhouseDoor01`; FNV's `HotelDoor01`, `BlackDoor`). Those hinged ones are the only place the engine swing (DRT-MOVE-002) would be exercised, 411 of Oblivion's 2,156 placements and a few dozen elsewhere.
- **Starfield is not read.** Its door models parse but name no sequence and no behaviour graph in the NIF, and its animation text tables have no reader, so `none` here means "not in the NIF", not "does not move". It stays census-only (decision Q3).

## 2. What the sources settle

| Question | Source | Answer |
|---|---|---|
| What activating a door does | CS *Activate* | the default activation of a door "Opens" |
| That a door animates | CS and GECK *Door* | "Animating doors run their animation when activated. Usually this means swinging open" |
| The state numbers | CS and GECK *GetOpenState* | 0 none (not a door), 1 open, 2 opening, 3 closed, 4 closing; `SetOpenState` takes 0 closed and 1 open, "NOT the same values" |
| Script open of an unanimated door | CS *SetOpenState* | "Will animate open/closed if the door is loaded. Calling this to open on certain doors, possibly ones without animations, causes the game to lock up" |
| The default state | CS *GetDoorDefaultOpen*, CS and GECK *Reference* | the state a door reverts to when its cell is unloaded and re-entered; `Open by Default` sets it |
| `Minimal Use` | CS and GECK *Door* | NPCs do not use the door when path finding through the cell |
| Find a door | CS *Find Package* | "Open it. Go through for load doors." |
| Locks and actors | CK *Locking and Unlocking Doors* | for load doors: an actor must own the door, be in the owning faction or carry the key, "otherwise their AI package will not start" |

The CS *SetOpenState* line bears on DRT-MOVE-002. It suggests that the original does not open a door that authors no animation by script, and it says nothing of what a player's activation of one does, so the swing's existence is still unsourced and the labelled approximation (decision Q2) stays.

Not in any source read, and so still **Phase 0: verify** (filed as #7062): the swing's hinge, angle, duration and direction; what a door does when it closes onto a body; what an activation during a swing does; when the loop sound plays; whether the default-state return reaches exteriors or a cell restored from a save; whether an actor passes a locked in-cell door by ownership, faction or key; and what `Automatic Door` does to a door that leads nowhere.

## 3. Shape of the runtime

**State.** The door state is the existing `TwoStateActivator` (`is_open`, `is_animating`; `crates/scripting/src/vm_state.rs`), which the save registry already carries and `GetOpenState` already reads (DRT-G-005). `is_open` is the state the door is heading to, and `is_animating` says it has not arrived: open and animating is *opening*, closed and animating is *closing*. The spawn path adds the component to every in-cell door (DRT-STATE-001); the Papyrus recogniser keeps filling it for `default2StateActivator` scripts. The `doOnce` and `activatedOnce` fields stay as they are.

**Authored data.** A second, immutable component, `DoorData`, carries what the record and the placement author: the decoded flags (DRT-REC-001), the three sounds (DRT-REC-002), the default state (DRT-REC-004) and the motion class with its clip handles or swing parameters. It carries no open or closed state, so there is still one state machine.

**Motion.** The class is chosen once, where the model is imported (NIFAL): `Sequence` when the model has `Open` and `Close` clips (the existing `NamedSequences`), `Swing` when it has neither, `Graph event` reserved. A system plays the `Open` or `Close` clip through the object's animation player when `TwoStateActivator` changes, sets `is_animating` for the clip's length, clears it on the last frame, and never reads the game kind (DRT-MOVE-003). The swing, when it runs, rotates the door node about its hinge and is the only code with invented constants (decision Q2), held in one place.

**Collision.** Keyframed bodies already follow their entity's `GlobalTransform` (`crates/physics/src/sync.rs`, `push_kinematic`), and the door models author the collision object on the moving node (`bhkCollisionObject` beside the sequences). So the collider follows the clip with no new physics code, which DRT-MOVE-004's test proves. DRT-MOVE-005 (a closing door over a body) is the one physics addition: a capsule overlap test against the door's collider, run while `closing`.

**Activation.** The one entry is the existing `ActivateEvent`. A system consumes it for an in-cell door that is not locked to its activator, flips `is_open`, starts the motion and queues the sound. The player's prompt reads the state (`Open` or `Close`, DRT-ACT-002). An actor's package raises the same event (AIP-DOOR-002).

**Saves.** `TwoStateActivator` is saved; a restored `open` door must sit at its open pose with open collision and no replayed swing (DRT-SAVE-001), so the restore seeks the `Open` clip to its last frame.

## 4. Phase 0 status

| PRD Phase 0 item | State |
|---|---|
| Motion census (DRT-REC-003) | done: §1, the example is checked in |
| Verify items | the sourced ones recorded in §2; the rest filed as #7062 |
| The Moth door (PRD §7.2 scenario 2) | `TempleMothCatacombs02`, reference `00094D23`, base `0000C519`, a `sequence` door, **locked** (level 10, key `0009DAC6`, `TG08AncestorMothCatacombKey`); the priest who searches for a container (`TG08MothPriestBossMale01`) is not among the four actors that carry the key |
| This design | this document |

## 5. Phase 1 status (#7059)

| Requirement | State |
|---|---|
| DRT-REC-001, -002, -004 (the `DOOR` flags and sounds, `Open by Default`) | built: `DoorData` and the placement's `ONAM`, per game |
| DRT-STATE-001 (the state from spawn, `Open by Default` spawns open) | built: `attach_door` seeds `TwoStateActivator::is_open` from the default; `GetOpenState` reads it |
| DRT-MOVE-001, DRT-ACT-001 (the authored `Open` / `Close` clips; no activation mid-motion) | built: `door_motion_system` |
| DRT-SND-001 (open sound as it starts opening, close sound as it starts closing) | built 2026-11-20: `door_sound` picks `SNAM` / `ANAM`, `scripted_sound::play_sound_record` plays it at the door through M44; the loop sound (`BNAM`) is deliberately not played, when it plays being unsourced (#7062). Unit-tested for the pick; the audible path rides the script-sound helper and is unverified live until `door-runtime.sh` |
| DRT-STATE-003, DRT-SAVE-001, -002 (default on re-entry, saved state, open pose without a replayed swing) | partly: a stale pose snaps (`posed`), a respawned door takes its default; no test pins re-entry and no save leg is written |
| DRT-ACT-002, -003, -004 (prompt, lock and the activator's key, `OnActivate` first) | the prompt reads `Open` / `Close`; the activator's-inventory key and the script-first order are not built |
| DRT-MOVE-002 to -005 (the swing, collision, closing onto a body) | not built (the swing's hinge, angle and duration are unsourced, #7062) |
| `docs/smoke-tests/door-runtime.sh` | not written |

