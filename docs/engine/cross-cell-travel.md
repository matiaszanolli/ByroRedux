# Cross-cell actor travel

**Status**: ACTIVE (2026-11-13). The design the
[cross-cell actor travel PRD](../prd/cross-cell-actor-travel.md) names as the
owner of door routing, the relocation ledger and offscreen arrival
(PRD *Related*, last row). Tracking: #6850; phases #6851 (0, this document),
#6852 (1), #6853 (2), #6854 (3). The PRD states the promise and the `XCT-`
requirements. This document states how the engine keeps it.

## 1. One model: an actor's space is ledger state

A *space* is an interior cell or a whole worldspace (PRD §2). Where a
persistent actor is lives in one FormID-keyed, saved resource,
`ActorRelocations` (`crates/scripting/src/created_references.rs`), read by
every subsystem that places actors:

| Field | Meaning | Written by | Read by |
|---|---|---|---|
| `placements` | Actor → `CreatedPlacement` (cell, Z-up pose) once it has left its home cell | the relocation step (`App::step_actor_relocations`), from every move request | the cell loader (XCT-LOC-002), `actor.where`, logical identities |
| `journeys` | Actor → `Journey` (destination reference, arrival placement, far side of its last door, arrival game time) while it travels **Offscreen** | the relocation step and the unload path (§4) | the arrival step (§4), `actor.where`, the census sweep |

The authored cell is only the starting value. An actor that is neither
ledgered nor on a journey is where the load order puts it.

Every move goes through one request queue, `PendingRelocations`. A Travel hop
through a door, a scripted `MoveTo`, `MoveToPackageLocation`, a follower
passing a door with its leader, and an offscreen arrival all use it. The
relocation step drains it once per frame, outside an interior transition:

1. record the placement in the ledger and log `actor <form> -> <cell>`
   (PRD §3's hop log line);
2. take the actor's live body out of the cell it leaves (`evict_actor`:
   the same capture-and-release a cell unload runs, so its inventory,
   equipment and script variables park in `PersistentReferenceStates`,
   XCT-ROUTE-007);
3. once the cell it entered is loaded, spawn it there at its ledger pose
   (`relocated_in` → `spawn_relocated`). That is the next frame when the cell
   is already loaded (XCT-ROUTE-005), or whenever the player loads it.

## 2. The routing graph

`byroredux_plugin::esm::cell::SpaceGraph` is built once per load order from
every placed reference with an `XTEL`. Each one is a load door from its own
space into its partner's space, landing at the `XTEL` pose. An actor walks to
a door's *approach*, the partner's own `XTEL` pose, which is where the engine
places someone coming the other way, so it is the floor in front of the door.

**Door links (XCT-ROUTE-003).** Where the game's navmesh data says which
doors a navmesh reaches, only those doors are routes: `NAVI` `NVCI` *Door
Links* on FO3/FNV (`NaviRecord::door_links`) and `NAVM` door triangles on
Skyrim and Starfield (`NavmRecord::door_triangles`), merged by
`EsmIndex::navmesh_door_links`. Oblivion authors none, and Fallout 4's packed
navmeshes are undecoded. There every load door routes (PRD deviation 3).
Measured on the shipped masters, the links cover 1,056 of 1,064 FNV load
doors, 1,109 of 1,114 FO3 and 1,691 of 1,708 Skyrim, so the filter drops
only doors no navmesh reaches.

**Routes.** `SpaceGraph::route(from, start, to, goal)` takes the fewest doors
(a breadth-first hop count back from the destination space). At each hop it
takes the door nearest to where the actor stands among the doors that keep
the route that short. That is XCT-ROUTE-001's "the nearest when several do",
applied hop by hop. A route's length is the sum of its straight legs: start →
first approach, each landing → next approach, last landing → goal. The same
graph answers the census's hop counts (`inspect cross-space-census`), so the
census measures the routes the engine takes.

**Why `XTEL` and not `NVCI` as the graph.** `NVCI` door links say which doors
a navmesh can reach. They do not say where a door leads; only the door's
`XTEL` does. Each game's door-link data is therefore a filter on the `XTEL`
graph, not a second graph.

## 3. Watched travel (Routing)

`travel_system` asks `route_toward` for a Travel whose destination reference
is not resident:

| Outcome | When | Effect |
|---|---|---|
| `SameSpace` | the destination is resident, or the load order puts it in the actor's own space | walk to it. A non-resident one in the same space is walked to at its load-order position (XCT-DEST-001), not a wander target |
| `Door` | a route exists; its first door is a live entity of the actor's cell | stamp `CrossSpaceHop`, walk to the door's approach (**Routing**) |
| `Unroutable(reason)` | the destination is in no cell (XCT-DEST-003), the actor is not persistent (XCT-DEST-004, Q3), or no chain of doors leads there | stand still. The reason goes to `TravelReports` for `actor.where`, asked again after `ANCHOR_RETRY_SECS` |

Reaching the approach (`DOOR_HOP_REACH`), the actor fades out over the game's
`fActorTeleportFadeSeconds` (XCT-ROUTE-008; the `AnimatedAlpha` sink
`SetActorAlpha` already drives). Then it passes: a request to the far side's
`XTEL` pose in the partner's cell, carrying the onward destination. The
relocation step then either:

- **lands it in the destination's space**: **Arrived** (XCT-ROUTE-009). It
  spawns there when that cell is loaded and fades in, and its package walks it
  on to its location;
- **lands it in a loaded intermediate cell**: it spawns there and routes again
  from there (**Routing**, XCT-ROUTE-005);
- **lands it in an unloaded intermediate cell**: it continues as a journey
  for the rest of the route (**Offscreen**, XCT-ROUTE-006).

**Followers (XCT-FOL-001, Q2).** When a leader passes a door, every live
actor whose Follow or Escort package targets that leader passes it too, to
the same far-side pose. That covers an NPC's hop (`travel_system`) and the
player's own door use (`queue_door_transition`).

## 4. Offscreen travel (journeys)

An unwatched actor is never simulated en route (XCT-NG-002). A journey
records only its destination, where it will stand, and when:

- **Started** when a **Routing** actor's cell unloads under it (the unload
  path asks `route_toward` for every released actor with an unfinished
  cross-space Travel), when a hop lands in an unloaded intermediate cell, or
  by the census sweep (`xct.sweep`, §6).
- **Arrival time** (XCT-OFF-001, Q1): now + route length ÷ the actor's
  walk speed (`WalkSpeed`, engine default when unauthored), in **game** time:
  the walk's real seconds × `GameTimeRes::time_scale`. A wait or sleep that
  advances the clock advances the journey, as it advances everything else on
  that clock.
- **On the road** (XCT-OFF-003): the actor has no body anywhere, and the
  cell loader skips it at its home cell and at its ledger cell. Its logical
  identity (§5) stays at its origin pose, so a script that reads its position
  mid-route sees its origin (PRD deviation 1).
- **Arrival**: the arrival step (part of the relocation step, once per
  in-game minute) ends every journey whose time has come:
  - the player is in the destination's space with the far side of its last
    door loaded: the actor comes through that door, placed at the door's
    `XTEL` pose, and its package walks it on (XCT-OFF-004);
  - otherwise it is placed at its package location (XCT-OFF-002), the
    destination reference's pose, and spawns there when the player arrives.

A journey ends early when a script moves the actor (`MoveTo` wins).

## 5. Logical identities (XCT-LOC-005)

A persistent actor whose cell is not loaded must still be addressable by
scripts. The worldspace persistent loader already stubs worldspace-persistent
actors (`UnloadedReferenceStub`, #6076's promote/demote), and scenes stub the
actors they cast (`RemoteSceneActorStub`). The gap is every other persistent
actor: interior-persistent actors and ledgered actors whose current cell is
not loaded.

`cell_loader::relocated::reconcile_logical_identities` closes it with the
scene path's own stamp (`spawn_logical_quest_reference_with_scripts` plus
`RemoteSceneActorStub`, no third kind). It runs with the relocation step,
keyed on the loaded cells and the ledger digest:

- **Demote**: every persistent actor of an interior cell that is not loaded,
  and every ledgered or journeying actor whose current cell is not loaded,
  with no entity yet, gets a stub at its current pose (ledger, journey
  origin, or load order). The stubs have no `CellRoot`, so no unload releases
  them.
- **Promote**: before a cell's reference loader or the relocation spawn gives
  an actor its body, its stub's state is captured and the stub despawned
  (`promote_persistent_actor_stubs`, the #6076 helper), so the body takes over
  the identity instead of running beside it.

A stub runs no `GameMode` (#6220). Its event blocks still answer.

## 6. Scripts, reporting and the sweep

- **`MoveTo` (XCT-LOC-004).** ObScript `MoveTo` and Papyrus
  `ObjectReference.MoveTo` keep their direct path when the moved actor and the
  target both have live bodies. Otherwise the scripting crate queues
  `ActorMoveRequest::ToReference` in `PendingActorMoves`. The relocation step
  resolves the target's current placement (its body, its ledger entry, or the
  load order), applies the Z-up offset, and relocates through §1. The player
  keeps its own path (`PlayerMoveRequest`).
- **`MoveToPackageLocation` (XCT-LOC-006).** Lowered to
  `Effect::MoveToPackageLocation`, queued as
  `ActorMoveRequest::ToPackageLocation`. The step reads the actor's running
  package and its location reference (`PackRecord::location_reference`), and
  moves it there like a `MoveTo`.
- **`actor.where <entity|0xREFR>` (XCT-RPT-001)** prints the §3 state:
  `routing via door entity <id> toward <cell>`,
  `offscreen, arrives at day <d> <hh:mm> in <cell>`, `unroutable <reason>`, or
  `in <cell>` / `not loaded, in <cell>` (Arrived or never moved). It also
  prints the pose and whether the actor has left its home cell.
- **`actor.where --census` (XCT-RPT-002)** lists every ledgered,
  journeying or unroutable actor with its home cell, current cell and state.
- **`xct.sweep [limit]`** starts a journey for every census pair of a
  persistent actor (the §8 population the engine can route), or records it
  **Unroutable** with its reason. `docs/smoke-tests/xct-census.sh` runs it,
  advances the clock past every arrival, and reads `actor.where --census`
  (PRD scenario 4).

## 7. Saved state

`ActorRelocations` (placements and journeys) is a saved resource column.
Adding journeys changed its shape, so `FORMAT_MAJOR` moved with it and no
field has a `serde(default)` (the save shape guard). `PendingRelocations`,
`PendingActorMoves`, `TravelReports`, `CrossSpaceHop` and the door fade are
`NOT_SAVED_BY_DESIGN`. A load re-chooses routes from the packages, and the
stubs are rebuilt by §5's reconcile.

## 8. Phase 0 census

`cargo run --release -p byroredux-plugin --example inspect -- cross-space-census <game> <ESM>`
(the census is an `inspect` subcommand, not the separate example #6851 named:
#5856 folds one-off probes into `inspect`). Shipped masters, 2026-11-13:

| game | actors with packages | pairs (ref destination) | destination not placed | cross-space | persistent actor | 1 hop | 2 hops | 3+ hops | unreachable |
|---|---|---|---|---|---|---|---|---|---|
| fnv | 5987 | 3775 | 116 (116 player) | 1008 | 986 | 546 | 290 | 77 | 95 |
| fo3 | 5265 | 2813 | 17 (17 player) | 1199 | 1191 | 737 | 219 | 213 | 30 |
| oblivion | 3361 | 5247 | 9 (9 player) | 2880 | 2876 | 1530 | 835 | 473 | 42 |
| skyrim | 4717 | 1622 | 57 (57 player) | 849 | 509 | 553 | 200 | 94 | 2 |
| fo4 | 1420 | 426 | 9 (9 player) | 102 | 24 | 62 | 32 | 8 | 0 |
| starfield | 2979 | 427 | 0 (0 player) | 70 | 7 | 33 | 9 | 1 | 27 |

Multi-hop share (2+ hops ÷ cross-space): FNV 36%, FO3 36%, Oblivion 45%,
Skyrim 35%, FO4 39%, Starfield 14%. Non-persistent actors are **Unroutable**
by Q3: 22 FNV pairs, 8 FO3, 4 Oblivion, 340 Skyrim, 78 FO4, 63 Starfield.

## 9. Phase 0 verify items

| Item | Settled by | State |
|---|---|---|
| XCT-ROUTE-001: which door the original routes Sunny through | observing VCG02 stage 10 in the original | filed #6891 (needs the original game) |
| XCT-OFF-001: how long an unwatched Travel takes in the original | timing one in the original | filed #6892 |
| XCT-OFF-002: where an offscreen arrival stands | observation in the original | filed #6893 |
| XCT-LOC-003: the FO3 / FNV / FO4 / Starfield save equivalents of *Moved* / *Cell Changed* | published save-format documentation | filed #6894: none published. UESP documents only Oblivion and Skyrim. The FO4 lead is the ReSaver editor's source |

The engine's defaults until they close: the nearest door at each hop (§2),
the walk time at the walk speed (§4), and the package location (§4). The
ledger is the engine's own format, so XCT-LOC-003 does not block it.
