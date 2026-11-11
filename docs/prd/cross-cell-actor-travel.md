# Cross-cell actor travel — Product Requirements

**Status**: PROPOSED (2026-11-11). Revision 1, the first draft, written at
the owner's request after VCG02 ("Back in the Saddle") was found to need an
actor to leave an interior. Statements about how an original engine, format
or tool behaves cite external sources; a claim that could not be verified is
marked **Phase 0: verify** instead of being stated as fact.

**Scope**: an actor whose AI package or script sends it to a reference in
another cell gets there. While its cell is loaded it walks to the load door
that leads toward the destination and comes out on the far side; while its
cell is not loaded it arrives offscreen; either way the engine then spawns it
in its new cell, not its authored one, across cell loads and saves, and a
scripted `MoveTo` to a reference in another cell takes the same path. For
every title with loadable cells and actors. Outside it: door and cover
semantics *inside* one navmesh, simulating what an unseen actor does on the
way (combat, sandboxing), and non-persistent actors unless decision Q3 takes
them in.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Package selection, procedures and their destinations | [AI Packages Completion — PRD](ai-packages-completion.md) and [PACKAL](../engine/packal.md#2-two-drivers-one-core) |
| Pathing across resident navmesh tiles; door data decode | [NAVM Pathfinding](../engine/navmesh-pathfinding.md#2-scope) |
| FormID-keyed actor state across a despawn | [Stream-boundary state continuity](../engine/stream-boundary-state-continuity.md#3-formid-keyed-state-not-entityid-keyed-state) |
| Saved state and its restore on load | [Save / load round trip](../engine/save-load-roundtrip.md#6-live-load-apply-m451) |
| Door routing, the relocation ledger, offscreen arrival | none yet: Phase 0 writes `docs/engine/cross-cell-travel.md` |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `XCT-` prefix.

---

## 1. Product thesis

The owner's goal for New Vegas is "make FNV playable from the beginning".
The new-game route now runs from Doc Mitchell's house to Goodsprings
(`docs/smoke-tests/fnv-new-game.sh`), but the first quest after it, VCG02,
stalls at stage 10: its result script says "Sunny goes outside" and runs
`SunnyREF.evp`, which selects `VCG02SunnyTravelOutside`, a Travel package to
`VCG02SunnyOutsideMarkerREF` (0x0010A201). `SunnyRef` (0x00104E85) is a
persistent actor placed in `GSProspectorSaloonInterior` (0x00106185), and the
marker is outside. The engine has no way to move an actor between cells, so
Sunny never leaves and the shooting tutorial is reachable only from the
console.

This is not one quest's problem. The original games move actors between
cells and keep them there: Oblivion's save format carries a *Moved* record
(the reference's new cell and position) and a *Cell Changed* record ("the
original position of an actor who has moved from one cell to another"),
seen on actor change records
([UESP: Save File Format/Location Data](https://en.uesp.net/wiki/Oblivion_Mod:Save_File_Format/Location_Data)).
Travel packages name a destination the actor paths to
([GECK: Travel Package](https://geck.uesp.net/wiki/Travel_Package)), and
`MoveTo` places a reference at a target reference wherever it is
([GECK: MoveTo](https://geck.uesp.net/wiki/MoveTo),
[CK: MoveTo](https://ck.uesp.net/wiki/MoveTo_-_ObjectReference)).

It is for players, who meet quest-givers where the quest says they will be,
and for quest and mod authors, whose packages and `MoveTo` calls name
references in other cells.

The corollary that shapes every requirement: **an actor's cell is state,
not a constant of its record.** Where an actor is lives in one FormID-keyed
ledger that the cell loader, the AI packages, scripts and saves all read;
the authored cell is only its starting value.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Home cell** | The cell the actor's `ACHR` record is placed in by the load order |
| **Current cell** | The cell the actor is in now: its home cell until it moves |
| **Cross-cell destination** | A package location or script target reference whose cell, by `EsmCellIndex::locate_refr`, is not the actor's current cell |
| **Load door** | A door reference carrying an `XTEL` teleport destination: the door it leads to and a pose on the far side |
| **Watched** | The actor's current cell is loaded (its entity exists with content) |
| **Unwatched** | The actor's current cell is not loaded |
| **Relocation ledger** | The FormID-keyed record of each moved actor's current cell and pose, saved, read by the cell loader |
| **Hop** | One pass through one load door |
| **Logical identity** | An entity that stands for a persistent reference whose cell is not loaded, as `UnloadedReferenceStub` does for worldspace-persistent actors today |

---

## 3. Core model

Each requirement below is phrased in four states an actor with a cross-cell
destination can be in:

| State | Meaning | Reported by |
|---|---|---|
| **Routing** | Watched; walking to the load door of its next hop | `actor.where` (XCT-RPT-001): `routing via door <form>` |
| **Through** | Has passed a door this frame; its pose and current cell are rewritten to the door's far side | the engine log line `actor <form> -> <cell>` |
| **Offscreen** | Unwatched; arrival time pending | `actor.where`: `offscreen, arrives at <game time>` |
| **Arrived** | Its current cell is the destination's cell | `actor.where`: `in <cell>`; the package then runs as within one cell |

The inventory below is the state at HEAD on 2026-11-11.

| # | Piece | Today | Proof |
|---|---|---|---|
| 1 | Travel to a reference that is not resident | Waits `ANCHOR_RETRY_SECS` (2 s), then walks to a wander target near home | `resolve_destination`, `byroredux/src/systems/travel.rs:150`; `defer_unresolved_reference`, `:114` |
| 2 | Scripted `MoveTo` of an actor | Moves a resident entity to a resident destination only; declines otherwise | `apply_move_to_effect`, `crates/scripting/src/fragment/effects/placement.rs` |
| 3 | Player `MoveTo` / door to another cell | Works: queued cell transition | `PlayerMoveRequest`, `byroredux/src/app_step.rs` |
| 4 | Actor state across a despawn of its own cell | Position, package, Travel and seat kept within the same cell, not saved | `ActorStreamSnapshot`, `byroredux/src/cell_loader/stream_snapshot.rs` |
| 5 | Persistent actor whose cell is not loaded | Worldspace-persistent actors get a logical identity; interior-persistent actors (such as `SunnyRef`) get none | `cell_loader/exterior/persistent.rs:183` |
| 6 | Navmesh door links (`NAVM` `NVDP`) | Parsed into `NavmRecord` door triangles; no reader outside the plugin crate | `NavmDoorTriangle`, `crates/plugin/src/esm/records/misc/world.rs`; [NAVM Pathfinding §2](../engine/navmesh-pathfinding.md#2-scope) |
| 7 | Navmesh connections (`NAVI` `NVCI` door links) | Not parsed: `NaviRecord` keeps `NVER` only | `parse_navi`, same file |
| 8 | Saved actor location | Nothing records an actor's cell; a reload places every actor in its home cell | `ReferenceState` rows in `byroredux/src/cell_loader/reference_state.rs` hold inventory, equipment, script state and a corpse's pose, but no cell and no living actor's position |

The catalog Phase 0 checks in (§9) replaces rows 1–8 as the dated source.

---

## 4. Goals and non-goals

### 4.1 Goals

- **XCT-G-001 — VCG02 without the console.** From Sunny's line in the
  Prospector Saloon, Sunny reaches `VCG02SunnyOutsideMarkerREF` and the
  bottle stage runs with no console step.
- **XCT-G-002 — Moves persist.** A moved persistent actor is in its new cell
  after the cell reloads and after a save and load.
- **XCT-G-003 — Scripts move actors across cells.** A scripted `MoveTo` (or
  Papyrus `MoveTo`) to a reference in another cell relocates the actor.
- **XCT-G-004 — Measured coverage.** Every game in scope publishes the share
  of cross-cell package destinations its actors reach (§8).

### 4.2 Non-goals

- **XCT-NG-001.** Door and cover semantics inside one navmesh (an actor
  opening a non-load door on its way, cover-to-cover movement): owned and
  excluded by [NAVM Pathfinding §2](../engine/navmesh-pathfinding.md#2-scope).
- **XCT-NG-002.** Simulating what an unwatched actor does en route (fights,
  sandboxing, intermediate positions). The ROADMAP keep list's "parse data,
  don't emulate engine", as the AI packages PRD applies it in
  [AIP-NG-003](ai-packages-completion.md#42-non-goals).
- **XCT-NG-003.** Fallout 76: its cells do not load with actors (§7.1).
- **XCT-NG-004.** The player's own cell transitions: they work today
  (inventory row 3) and are not changed here.

---

## 5. Requirements by area

### 5.1 Destinations (`XCT-DEST`)

- **XCT-DEST-001.** A package destination reference that is not resident
  MUST resolve to its cell and position through the load order
  (`EsmCellIndex::locate_refr`), not to a wander target. Today it falls back
  to a wander target after 2 s (inventory row 1).
- **XCT-DEST-002.** When that cell is not the actor's current cell, the
  actor MUST enter **Routing** (watched) or **Offscreen** (unwatched)
  instead of walking toward the destination's coordinates in its own cell.
- **XCT-DEST-003.** A destination whose cell the load order does not hold
  MUST leave the actor where it is and log the reference once.

### 5.2 Routing through load doors (`XCT-ROUTE`)

- **XCT-ROUTE-001.** A **Routing** actor MUST walk to a load door in its
  current cell whose `XTEL` leads into the destination's cell, choosing the
  nearest when several do. **Phase 0: verify** that the original routes a
  watched actor through a load door rather than placing it directly:
  settled by observing VCG02 stage 10 in the original game (does Sunny walk
  to the saloon's front door?) and recorded in the design.
- **XCT-ROUTE-002.** An actor that reaches that door MUST, the same frame,
  leave its cell and become **Through**: its current cell becomes the door's
  destination cell and its pose the `XTEL` pose.
- **XCT-ROUTE-003.** A destination two or more hops away SHOULD be routed
  hop by hop through `NAVI` `NVCI` door links, the field xEdit names
  "Door Links" per navmesh connection (`wbDefinitionsFNV.pas`, `NAVI`).
  **Phase 0: verify** that the original routes actors with these links;
  settled by a census of multi-hop destinations (§9, Phase 0) and, if any
  exist, observation in the original.
- **XCT-ROUTE-004.** A **Through** actor whose new cell is loaded MUST be
  spawned there on the next frame, with its package state, inventory,
  equipment and script variables; one whose new cell is not loaded MUST
  continue **Offscreen**.

### 5.3 Offscreen arrival (`XCT-OFF`)

- **XCT-OFF-001.** An **Offscreen** actor MUST become **Arrived** after the
  arrival time decision Q1 defines. **Phase 0: verify** how long the
  original takes; settled by timing an unwatched Travel in the original
  game, recorded in the design.
- **XCT-OFF-002.** When the player enters the destination cell after that
  time, the actor MUST spawn at its package location, as the CK's
  `MoveToPackageLocation` "places them" where "the running procedure(s) of
  the actor's current package" put them
  ([CK: MoveToPackageLocation](https://ck.uesp.net/wiki/MoveToPackageLocation_-_Actor)).
- **XCT-OFF-003.** When the player enters the destination cell *before*
  that time, the actor MUST NOT be spawned there yet.

### 5.4 The relocation ledger (`XCT-LOC`)

- **XCT-LOC-001.** Each moved actor's current cell and pose MUST be held in
  one FormID-keyed ledger, the model the stream-boundary design set for
  per-actor state
  ([§3](../engine/stream-boundary-state-continuity.md#3-formid-keyed-state-not-entityid-keyed-state)).
- **XCT-LOC-002.** The cell loader MUST spawn a ledgered actor in its
  current cell at its ledger pose and MUST NOT spawn it in its home cell.
  Today every actor spawns in its home cell (inventory row 8).
- **XCT-LOC-003.** The ledger MUST be saved and restored by a load, with a
  `FORMAT_MAJOR` bump and no `serde(default)` (the save shape guard). The
  original records the same facts (UESP, *Moved* and *Cell Changed*).
  **Phase 0: verify** that FO3 / FNV / Skyrim saves carry the equivalent;
  settled by the published save-format documentation of each.
- **XCT-LOC-004.** A scripted `MoveTo` of an actor to a reference in
  another cell MUST relocate it through the ledger, watched or not. Today
  it declines unless both are resident (inventory row 2).
- **XCT-LOC-005.** A persistent actor whose current cell is not loaded MUST
  have a logical identity scripts can address, interior-persistent actors
  included. Today interior-persistent actors have none (inventory row 5).

### 5.5 Reporting (`XCT-RPT`)

- **XCT-RPT-001.** `actor.where <entity|0xREFR>` MUST print the actor's
  current cell, pose and §3 state.
- **XCT-RPT-002.** `actor.where --census` SHOULD list every ledgered actor
  with its home cell and current cell, for the gates and the metrics.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | An unwatched actor is processed along its route (**Phase 0: verify** how) | No positions between departure and arrival (XCT-NG-002, Q1) | — | A script that checks an unwatched actor's position mid-route sees it at its origin. `actor.where` reports **Offscreen** (XCT-RPT-001), and the census (§8) measures how often a package destination is reached |
| 2 | Navmesh connections may drive multi-hop routes (**Phase 0: verify**) | One hop first; multi-hop through `NVCI` in Phase 3 | — | A two-hop destination is not reached before Phase 3. The Phase 0 census counts them (XCT-ROUTE-003) |
| 3 | Actors of any persistence may move (UESP saw *Cell Changed* on actor records without saying which) | Persistent actors first (Q3) | — | A non-persistent actor sent across cells stays home. XCT-DEST-003's log line names it |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow the engine tiers in
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail).
FNV leads, as the engine's reference title and the title that found the gap.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 3. Tier 3. Load doors carry `XTEL` like the later titles; routing beyond one hop has pathgrids, not navmeshes, so XCT-ROUTE-003 does not apply |
| Fallout 3 | yes | Wave 1. Shares FNV's records and package runtime |
| Fallout: New Vegas | yes | Wave 1. Reference title; VCG02 is acceptance scenario 1 |
| Skyrim (LE / SE / AE) | yes | Wave 2. Papyrus `MoveTo` and `MoveToPackageLocation` are the script half (XCT-LOC-004, XCT-OFF-002) |
| Fallout 4 | yes | Wave 2. One hop through `XTEL`; its packed navmesh door data is undecoded ([NAVM Pathfinding §2](../engine/navmesh-pathfinding.md#2-scope)), so multi-hop waits on that decode |
| Fallout 76 | no | Tier 3, parse only: no cell loads with actors (XCT-NG-003) |
| Starfield | yes | Wave 3. Tier 2 walkable interior; one hop first |

### 7.2 Acceptance

Game data comes from local installs only (`BYROREDUX_FNV_DATA` and the
other per-game variables the smoke scripts read); nothing is checked in.

A scenario passes when:

1. **VCG02 (FNV, headless).** A gate continues `fnv-new-game.sh` from
   Goodsprings: the player enters the Prospector Saloon and takes Sunny's
   line; within the time Q1 sets, `actor.where 0x00104E85` reports `in`
   the exterior cell holding `VCG02SunnyOutsideMarkerREF`, and the shooting
   leg passes without its console stand-in (XCT-G-001).
2. **Persistence (`cargo test`).** A ledgered actor spawns in its current
   cell and not its home cell after a reload, and the ledger survives a
   save round trip (XCT-LOC-002, XCT-LOC-003).
3. **Scripted move (FNV, headless).** `obscript.cmd 0x00104E85 109e
   r:0010A201` from Goodsprings, with the saloon unloaded, makes
   `actor.where` report the exterior cell, and entering it shows Sunny at
   the marker (XCT-LOC-004).
4. **Coverage (per game, headless).** The census sweep reaches the §8
   arrival rate published for its phase.

---

## 8. Metrics

The numbers live in ROADMAP's M42 row, beside the AI packages PRD's.

| Metric | Definition | Granularity |
|---|---|---|
| Cross-cell destinations | Packages whose destination reference lies outside the owning actor's home cell ÷ packages with a reference destination | per game |
| Multi-hop share | Cross-cell destinations two or more load doors away ÷ cross-cell destinations | per game |
| Arrival rate | Actors that reach a cross-cell destination in a headless sweep ÷ actors given one | per game, per phase |

---

## 9. Delivery sequence

**Phase 0 — Census, verify list, design (no engine code).** Count the §8
cross-cell and multi-hop destinations for every in-scope game and check the
table in; close the four **Phase 0: verify** items (XCT-ROUTE-001,
XCT-ROUTE-003, XCT-OFF-001, XCT-LOC-003) or file each as an issue; write
`docs/engine/cross-cell-travel.md` and add it to Related; decide Q1–Q3.
Exit: the census table is in ROADMAP for every in-scope game, the design is
merged, and every verify item is closed or filed.

**Phase 1 — The ledger and scripted moves.** XCT-DEST-001, XCT-DEST-003,
XCT-LOC-001 to XCT-LOC-005, XCT-RPT-001. Depends on M45 (saves). Exit:
acceptance scenarios 2 and 3 pass.

**Phase 2 — Watched travel through one door.** XCT-DEST-002,
XCT-ROUTE-001, XCT-ROUTE-002, XCT-ROUTE-004. Depends on M42 (packages) and
the NAVM pathfinder. Exit: acceptance scenario 1 passes on FNV and FO3.

**Phase 3 — Offscreen arrival, multi-hop, the census.** XCT-OFF-001 to
XCT-OFF-003, XCT-ROUTE-003, XCT-RPT-002, and followers if Q2 takes them in.
Exit: the arrival rate is published in ROADMAP for every in-scope game and
scenario 4 passes for each.

---

## 10. Decisions required

1. **Q1 — How an unwatched actor arrives.** Recommended: it arrives after
   the time a walk of its route would take (route length ÷ its walk speed),
   with no positions in between; the closest to the original that needs no
   offscreen simulation. Alternative: it arrives the moment its package
   starts; simplest, but actors appear where they could not yet be.
   Alternative: simulate the walk offscreen hop by hop; most faithful, at
   the cost XCT-NG-002 excludes.
2. **Q2 — Followers and escorts through load doors.** Recommended: in this
   PRD, Phase 3: a companion who follows the player through a load door is
   the most common cross-cell move a player sees, and it reuses the ledger
   and the door hop. Alternative: a separate PRD; this one ships sooner and
   companions stay behind at every door until then.
3. **Q3 — Which actors may change cells.** Recommended: persistent actors
   only: scripts can name only persistent references, and the ledger stays
   small. Alternative: every actor; a package-driven move of a generic NPC
   is kept too, at the cost of a ledger row and saved state per such actor.

---

## 11. Document map

| Need | Go to |
|---|---|
| Which package an actor runs and where it goes | [AI Packages Completion — PRD](ai-packages-completion.md), [PACKAL](../engine/packal.md#2-two-drivers-one-core) |
| Pathing inside the resident navmesh; door data | [NAVM Pathfinding](../engine/navmesh-pathfinding.md#2-scope) |
| Keeping per-actor state by FormID | [Stream-boundary state continuity](../engine/stream-boundary-state-continuity.md#3-formid-keyed-state-not-entityid-keyed-state) |
| How saved state is restored | [Save / load round trip](../engine/save-load-roundtrip.md#6-live-load-apply-m451) |
| Per-game tiers | [game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail) |
| The new-game route this continues | [fnv-new-game.sh](../smoke-tests/fnv-new-game.sh) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-11 | First draft, at the owner's request (VCG02 found to need an actor to leave an interior) | — |
