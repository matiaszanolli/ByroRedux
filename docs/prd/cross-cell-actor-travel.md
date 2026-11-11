# Cross-cell actor travel — Product Requirements

**Status**: ACTIVE (2026-11-11). Revision 4 files the tracking issues. Revision 3 records the owner's approval
with every §10 decision taken as recommended and folded into the
requirements (XCT-OFF-001, XCT-DEST-004, XCT-FOL-001). Statements about how
an original engine, format or tool behaves cite external sources; a claim
that could not be verified is marked **Phase 0: verify** instead of being
stated as fact.

**Tracking**: #6850 (parent). Phases: #6851 (0), #6852 (1), #6853 (2), #6854 (3).

**Scope**: an actor whose AI package or script sends it to a reference in
another space (another interior cell, or another worldspace) gets there.
While its cell is loaded it walks to the load door that leads toward the
destination and comes out on the far side; while its cell is not loaded it
arrives offscreen; either way the engine then spawns it in its new space,
not its authored one, across cell loads and saves, and a scripted `MoveTo`
to a reference in another space takes the same path. For every title with
loadable cells and actors. Outside it: travel within one worldspace (it
already paths across exterior tiles), door and cover semantics inside one
navmesh, simulating what an unseen actor does on the way (combat,
sandboxing), and non-persistent actors unless decision Q3 takes them in.

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
persistent actor placed in the interior `GSProspectorSaloonInterior`
(0x00106185), and the marker is in the WastelandNV worldspace. The engine
cannot move an actor from one space to another, so Sunny never leaves and
the shooting tutorial is reachable only from the console.

This is not one quest's problem. In the original, an actor whose
destination is in another space "will assume that he has to go through one
or more load doors" ([CS: SubSpace](https://cs.uesp.net/wiki/SubSpace));
actors fade "when going through a load door"
([CS: fActorTeleportFadeSeconds](https://cs.uesp.net/wiki/FActorTeleportFadeSeconds));
actors outside the loaded area normally keep updating and "go through load
doors" ([GECK: SetSceneIsComplex](https://geck.uesp.net/wiki/SetSceneIsComplex)).
Where they went is saved: Oblivion's save carries *Moved* (the new cell and
position) and *Cell Changed* ("the original position of an actor who has
moved from one cell to another")
([UESP: Save File Format/Location Data](https://en.uesp.net/wiki/Oblivion_Mod:Save_File_Format/Location_Data)),
and Skyrim's actor change form carries `CHANGE_REFR_MOVE` and
`CHANGE_REFR_CELL_CHANGED`
([UESP: ACHR Changeform](https://en.uesp.net/wiki/Skyrim_Mod:Save_File_Format/ACHR_Changeform)).
Scripts move actors anywhere with `MoveTo`
([GECK: MoveTo](https://geck.uesp.net/wiki/MoveTo),
[CK: MoveTo](https://ck.uesp.net/wiki/MoveTo_-_ObjectReference)).

It is for players, who meet quest-givers where the quest says they will be,
and for quest and mod authors, whose packages and `MoveTo` calls name
references in other spaces.

The corollary that shapes every requirement: **an actor's space is state,
not a constant of its record.** Where an actor is lives in one FormID-keyed
store that the cell loader, the AI packages, scripts and saves all read;
the authored cell is only its starting value.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Space** | An interior cell, or a whole worldspace with its exterior cells; the CS calls these world spaces ([CS: SubSpace](https://cs.uesp.net/wiki/SubSpace)) |
| **Home cell** | The cell the actor's `ACHR` record is placed in by the load order |
| **Current space** | The space the actor is in now, with its pose: its home cell's space until it moves |
| **Cross-space destination** | A package location or script target reference whose space, by `EsmCellIndex::locate_refr`, is not the actor's current space. A destination in another exterior cell of the same worldspace is not one |
| **Load door** | A door reference carrying an `XTEL` teleport destination: the door it leads to and a pose on the far side (xEdit `wbDefinitionsFNV.pas`, `XTEL`) |
| **Watched** | The actor's current cell is loaded (its entity exists with content) |
| **Unwatched** | The actor's current cell is not loaded |
| **Hop** | One pass through one load door |
| **Relocation ledger** | The FormID-keyed, saved record of each moved actor's current space and pose, read by the cell loader |
| **Logical identity** | An entity that stands for a persistent reference whose cell is not loaded: `UnloadedReferenceStub` (worldspace-persistent actors) or `RemoteSceneActorStub` (interior actors a scene casts) today |

---

## 3. Core model

An actor with a cross-space destination is in exactly one of four states:

| State | Meaning | Reported by |
|---|---|---|
| **Routing** | Watched, not in the destination's space; walking to the load door of its next hop | `actor.where` (XCT-RPT-001): `routing via door <form>` |
| **Offscreen** | Unwatched, not in the destination's space; its arrival time is pending | `actor.where`: `offscreen, arrives at <game time>` |
| **Arrived** | In the destination's space; the package runs there as within one space | `actor.where`: `in <space>` |
| **Unroutable** | No hop leads toward the destination (no such load door, its space unknown, or a multi-hop route before Phase 3) | `actor.where`: `unroutable <reason>`, and one log line per actor and destination |

Passing a door is an event between states, not a state: the log line
`actor <form> -> <space>` records each hop.

The inventory below is the state at HEAD on 2026-11-11.

| # | Piece | Today | Proof |
|---|---|---|---|
| 1 | Travel to a reference that is not resident | Waits `ANCHOR_RETRY_SECS` (2 s), then walks to a wander target near home | `travel.rs:77`, `:114`, `:150` (`byroredux/src/systems/`) |
| 2 | Scripted `MoveTo` of an actor | Moves a resident entity to a resident destination only; declines otherwise | `apply_move_to_effect`, `crates/scripting/src/fragment/effects/placement.rs` |
| 3 | Player `MoveTo` to another space | Fallout: a bare `MoveTo` queues a cell transition; ObScript declines it on Oblivion and whenever an offset is given | `crates/scripting/src/obscript_actions.rs:270-272`; `PlayerMoveRequest`, `byroredux/src/app_step.rs` |
| 4 | Actor state across a despawn of its own cell | Position, package, Travel and seat kept within one worldspace session, not saved; the store is cleared on every exterior → interior drain | `ActorStreamSnapshot`, `byroredux/src/cell_loader/stream_snapshot.rs`; `streaming_helpers.rs:765-768` |
| 5 | Persistent actor whose cell is not loaded | Worldspace-persistent actors get `UnloadedReferenceStub`; interior actors get `RemoteSceneActorStub` only when a scene casts them; otherwise none | `cell_loader/exterior/persistent.rs:183`; `materialize_scene_actor_alias_stubs`, `byroredux/src/scene/scene_runtime.rs:539` |
| 6 | Navmesh door links (`NAVM` `NVDP`) | Parsed into `NavmRecord` door triangles; no reader outside the plugin crate | `NavmDoorTriangle`, `crates/plugin/src/esm/records/misc/world.rs`; [NAVM Pathfinding §2](../engine/navmesh-pathfinding.md#2-scope) |
| 7 | Navmesh connections (`NAVI` `NVCI` door links) | Not parsed: `NaviRecord` keeps `NVER` only | `parse_navi`, same file |
| 8 | Saved actor location | A save restores a resident actor's `Transform` by FormID and parks inventory, equipment and script state by FormID in `PersistentReferenceStates`; neither records a space, so a reload puts every actor back in its home cell | `MUTABLE_DELTA_COLUMNS`, `byroredux/src/save_io/registry.rs:54`; `reference_state.rs:170` |

The catalog Phase 0 checks in (§9) replaces rows 1–8 as the dated source.

---

## 4. Goals and non-goals

### 4.1 Goals

- **XCT-G-001 — VCG02 without the console.** From Sunny's line in the
  Prospector Saloon, Sunny reaches `VCG02SunnyOutsideMarkerREF` and the
  bottle stage runs with no console step.
- **XCT-G-002 — Moves persist.** A moved persistent actor is in its new
  space after the cell reloads and after a save and load.
- **XCT-G-003 — Scripts move actors across spaces.** A scripted `MoveTo`
  (ObScript or Papyrus) to a reference in another space relocates the actor.
- **XCT-G-004 — Measured coverage.** Every game in scope publishes the share
  of cross-space package destinations its actors reach (§8).

### 4.2 Non-goals

- **XCT-NG-001.** Door and cover semantics inside one navmesh (an actor
  opening a non-load door on its way, cover-to-cover movement): owned and
  excluded by [NAVM Pathfinding §2](../engine/navmesh-pathfinding.md#2-scope).
- **XCT-NG-002.** Simulating what an unwatched actor does en route (fights,
  sandboxing, intermediate positions). The ROADMAP keep list's "parse data,
  don't emulate engine", as the AI packages PRD applies it in
  [AIP-NG-003](ai-packages-completion.md#42-non-goals).
- **XCT-NG-003.** Fallout 76: its cells do not load with actors (§7.1).
- **XCT-NG-004.** The player's own transitions (inventory row 3) and travel
  within one worldspace: unchanged here. The original, too, does not use
  load doors toward a destination in the same world space (CS *SubSpace*).

---

## 5. Requirements by area

### 5.1 Destinations (`XCT-DEST`)

- **XCT-DEST-001.** A package destination reference that is not resident
  MUST resolve to its space and position through the load order
  (`EsmCellIndex::locate_refr`), not to a wander target. Today it falls back
  to a wander target after 2 s (inventory row 1).
- **XCT-DEST-002.** A cross-space destination MUST put the actor in
  **Routing** (watched), **Offscreen** (unwatched) or **Unroutable**,
  instead of walking toward the destination's coordinates in its own space.
- **XCT-DEST-003.** A destination whose space the load order does not hold
  MUST put the actor in **Unroutable** where it stands.
- **XCT-DEST-004.** A cross-space destination of a non-persistent actor
  MUST make it **Unroutable** with reason `not persistent` (Q3).

### 5.2 Routing through load doors (`XCT-ROUTE`)

- **XCT-ROUTE-001.** A **Routing** actor MUST walk to a load door in its
  current space whose `XTEL` leads into the destination's space, the nearest
  when several do, as an actor bound for another world space "has to go
  through one or more load doors" (CS *SubSpace*). **Phase 0: verify** the
  original's choice among several doors; settled by observing VCG02 stage
  10 in the original game (which saloon door Sunny uses).
- **XCT-ROUTE-002.** An actor reaching that door MUST, the same frame, take
  the door's `XTEL` pose in the destination space and log the hop (§3).
- **XCT-ROUTE-003.** A destination two or more hops away SHOULD be routed
  hop by hop through each game's door links, read in the translate step:
  `NAVI` `NVCI` "Door Links" on FO3 / FNV (`wbDefinitionsFNV.pas`, `NAVI`),
  `NAVM` "Door Links" on Skyrim and Starfield (`wbDefinitionsTES5.pas:5534`,
  `wbDefinitionsSF1.pas:4052`), door `XTEL`s alone on Oblivion (pathgrids,
  `wbDefinitionsTES4.pas` `PGRD`). Until then such an actor is
  **Unroutable** with reason `multi-hop`.
- **XCT-ROUTE-004.** (withdrawn, rev 2) Split into XCT-ROUTE-005 to
  XCT-ROUTE-007 and XCT-ROUTE-009.
- **XCT-ROUTE-005.** An actor whose hop lands in a loaded cell MUST be
  spawned there on the next frame.
- **XCT-ROUTE-006.** An actor whose hop lands in a cell that is not loaded
  MUST continue **Offscreen**.
- **XCT-ROUTE-007.** A moved actor MUST keep its inventory, equipment and
  script variables, as `PersistentReferenceStates` already parks them across
  a despawn (inventory row 8).
- **XCT-ROUTE-008.** An actor passing a load door SHOULD fade out and in,
  as the original does for `fActorTeleportFadeSeconds`.
- **XCT-ROUTE-009.** A hop that lands in the destination's space MUST make
  the actor **Arrived**; one that does not MUST leave it **Routing** for its
  next hop.

### 5.3 Offscreen arrival (`XCT-OFF`)

- **XCT-OFF-001.** An **Offscreen** actor MUST become **Arrived** after the
  time a walk of its route takes (route length ÷ its walk speed), with no
  positions in between (Q1). Low-process actors are processed about
  every 120 seconds in the original
  ([GECK: GetActorProcessingLevel](https://geck.uesp.net/wiki/GetActorProcessingLevel));
  **Phase 0: verify** how that bounds an unwatched Travel's duration;
  settled by timing one in the original game.
- **XCT-OFF-002.** When the player enters the destination space after that
  time, the actor MUST spawn at its package location. **Phase 0: verify**
  where the original places it; settled by observation in the original.
- **XCT-OFF-003.** When the player enters the destination space *before*
  that time, the actor MUST NOT be spawned there yet.
- **XCT-OFF-004.** When the player is already in the destination space at
  the arrival time, the actor MUST appear at the far-side door of its last
  hop and walk on from there.

### 5.4 The relocation ledger (`XCT-LOC`)

- **XCT-LOC-001.** Each moved actor's current space and pose MUST be held
  in one FormID-keyed store, the model the stream-boundary design set for
  per-actor state
  ([§3](../engine/stream-boundary-state-continuity.md#3-formid-keyed-state-not-entityid-keyed-state));
  the design decides whether it extends `PersistentReferenceStates`.
- **XCT-LOC-002.** The cell loader MUST spawn a ledgered actor in its
  current space at its ledger pose and MUST NOT spawn it in its home cell.
  Today every actor spawns in its home cell (inventory row 8).
- **XCT-LOC-003.** The ledger MUST be saved and restored by a load, with a
  `FORMAT_MAJOR` bump and no `serde(default)` (the save shape guard), the
  facts Oblivion's and Skyrim's saves record (§1). **Phase 0: verify** the
  FO3 / FNV / Fallout 4 / Starfield equivalent; settled by their published
  save-format documentation.
- **XCT-LOC-004.** A scripted `MoveTo` of an actor to a reference in
  another space MUST relocate it through the ledger, watched or not. Today
  it declines unless both are resident (inventory row 2).
- **XCT-LOC-005.** Every persistent actor whose cell is not loaded MUST
  have a logical identity scripts can address, by extending the
  `RemoteSceneActorStub` path beyond scene casting rather than adding a third
  kind. Today an interior actor has one only while a scene casts it
  (inventory row 5).
- **XCT-LOC-006.** Papyrus `MoveToPackageLocation` MUST place the actor
  where its current package's procedure puts it
  ([CK: MoveToPackageLocation](https://ck.uesp.net/wiki/MoveToPackageLocation_-_Actor)),
  across spaces through the ledger.

### 5.5 Followers (`XCT-FOL`)

- **XCT-FOL-001.** An actor following or escorting a leader (Follow,
  Escort, Accompany) MUST pass a load door its leader passes and arrive on
  the far side with it (Q2).

### 5.6 Reporting (`XCT-RPT`)

- **XCT-RPT-001.** `actor.where <entity|0xREFR>` MUST print the actor's
  current space, pose and §3 state.
- **XCT-RPT-002.** `actor.where --census` MUST list every ledgered actor
  with its home cell, current space and state, the input of the §8 sweep.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Unwatched actors are processed at a low level, about every 120 s (GECK *GetActorProcessingLevel*), and go through load doors | No positions between departure and arrival (XCT-NG-002, Q1) | — | A script reading an unwatched actor's position mid-route sees it at its origin. `actor.where` reports **Offscreen** (XCT-RPT-001); the §8 arrival rate measures the effect |
| 2 | Multi-hop routes through load doors (CS *SubSpace*) | One hop first; multi-hop in Phase 3 | — | A two-hop destination is not reached before Phase 3; the actor is **Unroutable** `multi-hop` (XCT-ROUTE-003) and the Phase 0 census counts how many |
| 3 | Oblivion routes multi-hop through pathgrids | Door `XTEL`s only | — | An Oblivion destination needing pathgrid inter-cell links stays **Unroutable** (XCT-ROUTE-003) |
| 4 | `MoveTo` "does not function" when the actor is already in the cell and the player is in, entering or leaving it ([CK: MoveTo, Bugs](https://ck.uesp.net/wiki/MoveTo_-_ObjectReference)) | `MoveTo` moves the actor (XCT-LOC-004) | Script workarounds for the bug | Content that relied on the no-op sees the actor move. Low: the CK lists it as a bug |
| 5 | A move into an unloaded cell keeps the reference's old rotation (same CK section) | The ledger stores the target pose (XCT-LOC-001) | — | None known: the CK records it as a defect found by disassembly |
| 6 | Any actor may change cells (UESP saw *Cell Changed* on actor change records without saying which) | Persistent actors first (Q3) | — | A non-persistent actor sent across spaces stays home and is reported **Unroutable** `not persistent` |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow the engine tiers in
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail).
FNV leads, as the engine's reference title and the title that found the gap.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 3. Tier 3. One hop through door `XTEL`s; multi-hop through pathgrids is deviation 3 |
| Fallout 3 | yes | Wave 1. Shares FNV's records, door links (`NAVI` `NVCI`) and package runtime |
| Fallout: New Vegas | yes | Wave 1. Reference title; VCG02 is acceptance scenario 1 |
| Skyrim (LE / SE / AE) | yes | Wave 2. Door links in `NAVM`; Papyrus `MoveTo` and `MoveToPackageLocation` are the script half (XCT-LOC-004, XCT-LOC-006) |
| Fallout 4 | yes | Wave 2. One hop through `XTEL`; its packed navmesh door data is undecoded ([NAVM Pathfinding §2](../engine/navmesh-pathfinding.md#2-scope)), so multi-hop waits on that decode |
| Fallout 76 | no | Tier 3, parse only: no cell loads with actors (XCT-NG-003) |
| Starfield | yes | Wave 3. Tier 2 walkable interior; door links in `NAVM`; one hop first |

### 7.2 Acceptance

Game data comes from local installs only (`BYROREDUX_FNV_DATA` and the
other per-game variables the smoke scripts read); nothing is checked in.

A scenario passes when:

1. **VCG02 (FNV, headless).** A gate continues `fnv-new-game.sh` from
   Goodsprings: the player enters the Prospector Saloon and takes Sunny's
   line; within 120 s of wall time `actor.where 0x00104E85` reports `in
   WastelandNV` at `VCG02SunnyOutsideMarkerREF`, and the shooting leg passes
   without its console stand-in (XCT-G-001).
2. **Persistence (`cargo test`).** A ledgered actor spawns in its current
   space and not its home cell after a reload, and the ledger survives a
   save round trip (XCT-LOC-002, XCT-LOC-003).
3. **Scripted move (FNV, headless).** With XCT-LOC-005 in place,
   `obscript.cmd 0x00104E85 109e r:0010A201` run from Goodsprings with the
   saloon unloaded makes `actor.where` report `in WastelandNV`, and Sunny is
   at the marker (XCT-LOC-004). Today the caller does not resolve
   ("no resident reference has FormID").
4. **Coverage (per game, headless).** A sweep script
   (`docs/smoke-tests/xct-census.sh`, Phase 3) starts every cross-space
   package of the census and reads `actor.where --census`; it passes when
   every destination is **Arrived** or listed with its **Unroutable**
   reason.

---

## 8. Metrics

The numbers live in ROADMAP's M42 row, beside the AI packages PRD's.

| Metric | Definition | Granularity |
|---|---|---|
| Cross-space destinations | Packages whose destination reference lies outside the owning actor's home space ÷ packages with a reference destination | per game |
| Multi-hop share | Cross-space destinations two or more load doors away ÷ cross-space destinations | per game |
| Arrival rate | Destinations **Arrived** in the scenario 4 sweep ÷ destinations swept | per game, per phase |
| Unroutable by reason | Count of **Unroutable** destinations per reason in the sweep | per game, per phase |

---

## 9. Delivery sequence

**Phase 0 — Census, verify list, design (no engine code).** Count the §8
cross-space and multi-hop destinations for every in-scope game and check the
table in; close the four **Phase 0: verify** items (XCT-ROUTE-001,
XCT-OFF-001, XCT-OFF-002, XCT-LOC-003) or file each as an issue; write
`docs/engine/cross-cell-travel.md` and add it to Related; decide Q1–Q3.
Exit: the census table is in ROADMAP for every in-scope game, the design is
merged, and every verify item is closed or filed.

**Phase 1 — The ledger and scripted moves.** XCT-DEST-001, XCT-DEST-003,
XCT-DEST-004, XCT-LOC-001 to XCT-LOC-006, XCT-ROUTE-007, XCT-RPT-001. Depends on M45
(saves). Exit: acceptance scenarios 2 and 3 pass.

**Phase 2 — Watched travel through one door.** XCT-DEST-002,
XCT-ROUTE-001, XCT-ROUTE-002, XCT-ROUTE-005, XCT-ROUTE-006,
XCT-ROUTE-008, XCT-ROUTE-009. Depends on M42 (packages) and the NAVM pathfinder. Exit:
acceptance scenario 1 passes.

**Phase 3 — Offscreen arrival, multi-hop, the census.** XCT-OFF-001 to
XCT-OFF-004, XCT-ROUTE-003, XCT-RPT-002, XCT-FOL-001.
Exit: scenario 4 passes and the arrival rate is published in ROADMAP for
every in-scope game.

---

## 10. Decisions required

1. **Q1 — How an unwatched actor arrives.** Recommended: it arrives after
   the time a walk of its route would take (route length ÷ its walk speed),
   with no positions in between; the closest to the original that needs no
   offscreen simulation. Alternative: it arrives the moment its package
   starts; simplest, but actors appear where they could not yet be.
   Alternative: simulate the walk offscreen hop by hop at the original's low
   process rate; most faithful, at the cost XCT-NG-002 excludes.
   **Decided** (2026-11-11): as recommended: an unwatched actor arrives after its route's walk time, with no positions in between (XCT-OFF-001).
2. **Q2 — Followers and escorts through load doors.** Recommended: in this
   PRD, Phase 3: a companion who follows the player through a load door is
   the most common cross-space move a player sees, and it reuses the ledger
   and the hop. Alternative: a separate PRD; this one ships sooner and
   companions stay behind at every door until then.
   **Decided** (2026-11-11): as recommended: followers and escorts pass load doors with their leader, in Phase 3 (XCT-FOL-001).
3. **Q3 — Which actors may change spaces.** Recommended: persistent actors
   only: a reference "must be checked" persistent "to be used in a script"
   ([GECK: Reference](https://geck.uesp.net/wiki/Reference)), so every
   scripted move names one, and the ledger stays small. Alternative: every
   actor; a package-driven move of a generic NPC is kept too, at the cost of
   a ledger row and saved state per such actor.
   **Decided** (2026-11-11): as recommended: only persistent actors change spaces; any other is Unroutable `not persistent` (XCT-DEST-004).

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
| 2 | 2026-11-11 | Review round 1 folded in: logical-identity and save/snapshot HEAD claims, space-not-cell destinations, citations (MoveToPackageLocation, persistence, load-door pathing, process levels, Skyrim change forms, XTEL/PGRD), state model (XCT-ROUTE-004 withdrawn and split), arrival while watched, per-game door links, scenarios and exits, deviations | claude: revise |
| 3 | 2026-11-11 | Owner approval: Q1–Q3 decided as recommended and folded in (XCT-OFF-001 reworded; XCT-DEST-004, XCT-FOL-001 added) | owner: approved |
| 4 | 2026-11-11 | APPROVED → ACTIVE: tracking issues filed (#6850 (parent). Phases: #6851 (0), #6852 (1), #6853 (2), #6854 (3)) | — |
