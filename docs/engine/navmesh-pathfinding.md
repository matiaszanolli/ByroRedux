# NAVM pathfinding

**Status**: PROPOSED (2026-08-23). No code lands from this document by
itself — it's the design authority for EX-16 item 3 (#2372), flagged in
`docs/engine/exterior-readiness-plan.md` as "the single largest item in the
whole epic" and deliberately deferred pending its own scoping pass. This is
that pass.

## 1. What already exists (the foundation this builds on)

Unusually for a greenfield item, the data layer is *done*, not a blocker.
`parse_navm` (`crates/plugin/src/esm/records/misc/world.rs`) decodes real
triangle geometry and cross-mesh connectivity for every game that ships
`NAVM` except Fallout 4:

| Plugin | tiles | geometry decoded |
|---|---:|---:|
| `Skyrim.esm` + `Dragonborn.esm` + `Dawnguard.esm` | 19,611 | 19,550 |
| `Fallout3.esm` | 7,198 | 7,198 |
| `FalloutNV.esm` | 4,771 | 4,771 |
| `Fallout4.esm` | 7,894 | 0 (blob retained, undecoded) |
| `Oblivion.esm` | 0 | — (no NAVM at all) |

(Full provenance and the corpus-derived evidence for every decode decision
is in `parse_navm`'s doc comment — not repeated here.)

`NavmRecord` (same file) already exposes exactly the graph a pathfinder
needs:

- `vertices: Vec<[f32; 3]>` — world-space positions.
- `triangles: Vec<NavmTriangle>` — each with `vertices: [u16; 3]` and
  `edge_neighbours: [Option<u16>; 3]` (triangle index across each edge, or
  `None` at a mesh border). This *is* the within-tile adjacency graph;
  nothing needs to be derived, it's parsed straight off the wire.
- `external_connections: Vec<NavmExternalConnection>` — `{ mesh_form,
  triangle }` pairs. This is the cross-tile adjacency graph: a link from a
  border edge in this mesh to a specific triangle in another `NAVM`.
- `indices_are_in_range()` — already verified against all 11,969 decoded
  FO3+FNV meshes; a pathfinder can trust triangle indices without its own
  bounds-checking pass.

`NavmeshTile` (`byroredux/src/components.rs`) already streams this data
alongside every cell: one CPU-only ECS component per resident `NavmRecord`,
spawned by `spawn_navmesh_tiles` in the same window every other cell-owned
entity is spawned in, reclaimed automatically by the existing
`stamp_cell_root_range` → `unload_cell` teardown chain (no bespoke residency
tracking needed — EX-16 item 2 already established this). It currently
carries `#[allow(dead_code)]` because nothing reads it yet. This document is
about giving it that first reader.

## 2. Scope

**In scope**: point-to-point pathfinding across currently-**resident**
navmesh tiles, returned as a waypoint polyline, consumed by the six AI
locomotion systems that already call `step_toward`
(wander/travel/follow/escort/guard/patrol).

**Explicitly out of scope**, and why:

- **Fallout 4.** `NavmRecord::packed_geometry` retains the blob but nobody
  has decoded FO4's body layout (0/7,894 reconcile under Skyrim's). A
  pathfinder over FO4 content degrades to today's straight-line locomotion
  until that decode lands — a separate, already-tracked gap, not this
  document's problem to solve.
- **Doors and cover.** The Creation-Engine `NVNM` decoder walks door and
  cover triangle lists but deliberately does not retain them (`parse_navm`'s
  doc: "belong to actor traversal and combat cover... this crate has
  repeatedly paid for fields parsed ahead of a consumer"). The Gamebryo
  typed form's equivalents (`NVDP`/`NVCA`/`NVGD`) aren't even walked — the
  `parse_navm` match falls through to `_ => {}` for all three. So on
  *every* game, a navmesh triangle's door/cover semantics are currently
  unavailable at any layer. A path can cross a triangle flagged as a door
  boundary without knowing an actor should visibly interact with a door
  first. Not a regression (today's locomotion has no door awareness
  either), but worth being honest that pathfinding doesn't fix it — a door
  decode is its own prerequisite, sequenced in §9.
- **Triangle flags as path cost.** `NavmTriangle::flags: u32` is documented
  as "preferred pathing, water, door, …" but no per-bit meaning has been
  established against real data (same "flag not guess" posture the project
  applies everywhere else — see `exterior-readiness-plan.md`'s item 6/EX-10
  notes for the precedent). v0 treats every non-door triangle as uniform
  cost. Revisit once someone corpus-verifies the bit layout.
- **Dynamic obstacle avoidance.** The navmesh describes static, authored
  walkable surface. Actors, physics props, and anything that moves are not
  factored into the graph. Out of scope for the same reason it's out of
  scope for every shipped Bethesda title's navmesh, too.

## 3. Algorithm

Standard navmesh pathfinding, in two passes — this isn't a place to
innovate, the textbook shape fits the data exactly as parsed:

1. **A\* over the triangle-adjacency graph.** Nodes are `(mesh_form,
   triangle_index)` pairs. Edges are `edge_neighbours` (within-tile, no
   FormID change) and `external_connections` (cross-tile, FormID changes).
   Edge cost = distance between the two triangles' centroids (or, for a
   closer-to-optimal result at negligible extra cost, the distance between
   the midpoints of the shared edge — cheap to compute, avoids the
   centroid method's tendency to zigzag through long thin triangles).
   Heuristic = straight-line distance from a triangle's centroid to the
   goal (admissible, since it can only under-estimate the true walking
   distance).
2. **String-pulling (funnel algorithm) over the A\* triangle corridor.**
   A\* alone returns "which triangles to cross," not "where to actually
   walk" — naively following triangle centroids produces a visibly jagged
   path. The funnel algorithm (Simple Stupid Funnel, the standard
   navmesh-to-polyline technique — same approach Recast/Detour and most
   shipped engines use) walks the shared-edge "portals" between
   consecutive corridor triangles and produces the shortest polyline that
   stays inside the corridor. This is the step that turns "list of
   triangles" into "list of waypoints," which is the actual shape
   `step_toward` (§5) needs.

Both passes are pure graph/geometry algorithms over data already resident
in memory — no new I/O, no new resource, no interaction with streaming
beyond "only consider currently-loaded tiles" (§4).

## 4. Cross-tile pathing and the residency boundary

`external_connections` makes cross-mesh pathing possible in principle, but
the pathfinder can only ever see tiles that are **currently resident** —
same boundary every other exterior system in this engine already respects
(`ExteriorWorldContext` keeps parsed data resident, but `NavmeshTile`
entities are spawned/despawned with the streaming grid, not with the
parsed-data lifetime). Two consequences:

- **The graph query is a live ECS scan, not a persisted structure.**
  Building a `NavmeshGraph` resource that's rebuilt/invalidated every time
  the streaming grid changes would be duplicating state `NavmeshTile`'s own
  entity set already *is* — the same reasoning EX-16 item 2 used to decide
  no bespoke reclaim path was needed for `NavmeshTile` itself. A path
  request should query `world.query::<NavmeshTile>()` (or an
  index — see §8) at request time, not maintain a second copy of the same
  graph.
- **A path that would need an unresident tile must degrade, not fail
  silently or panic.** If A\* exhausts every resident triangle without
  reaching a triangle containing the goal, the correct behavior is falling
  back to today's straight-line `step_toward` for the unreachable remainder
  — exactly the same posture the engine already has for every actor today,
  just now scoped to "beyond the known corridor" instead of "always." This
  also naturally handles the common case of a goal that's simply off any
  navmesh (open wilderness FNV/FO3 content, or a point that's just wrong).

## 5. Point localization

Both the start (actor's current position) and the goal (target destination)
are arbitrary world points, not triangle indices — the pathfinder needs a
`find_containing_triangle(point, tiles) -> Option<(mesh_form, triangle_idx)>`
step before A\* can run at all. This doesn't exist anywhere in the codebase
today (confirmed: no point-in-triangle/barycentric helper anywhere under
`crates/core/src/math`), so it's real new code, not a reuse.

Approach: for a given query point, walk the vertical column of resident
tiles whose 2D (XZ) footprint could contain it (a tile's vertex bounding box
is enough for this — no spatial index needed at today's per-cell tile
counts, see §8 on when that stops being true), test each candidate
triangle's XZ projection with a standard barycentric/sign test, and prefer
the vertically-closest match when a point could plausibly belong to more
than one triangle at different heights (multi-story interiors, a bridge
over a navmesh below). A point that matches no triangle at all is the
"target isn't on a navmesh" case from §4 — not an error, a `None`.

## 6. Consumer integration

All six locomotion-driving AI systems (wander/travel/follow/escort/guard/
patrol) already funnel through `step_toward`
(`byroredux/src/systems/locomotion.rs`), which currently takes one
`target_xz: Vec3` per call. Pathfinding doesn't replace `step_toward` — it
sits *above* it, turning "one target point" into "a sequence of waypoints
fed to `step_toward` one at a time":

- A new shared component, tentatively `NavPath { waypoints: VecDeque<Vec3>
  }`, computed once when a system resolves its destination (mirroring where
  `TravelState.destination` and `WanderState`'s picked point are set today
  — no new resolution timing to invent).
  - When §5 finds no containing triangle for the goal, or §3's A\* can't
  reach it (§4), `NavPath` is either omitted or given a single waypoint
  (the goal itself) — the caller falls back to exactly today's
  straight-line behavior, so this is additive, not a required upgrade path
  every consumer must handle specially.
- `step_toward`'s call sites pop the front waypoint once arrived (same
  `LOCOMOTION_ARRIVAL_EPSILON` check that already exists) rather than
  reporting "arrived" after the first `step_toward` call.
- **Follow is the one system that needs special handling.** It re-resolves
  a *live*, moving target's position every tick (per its own module doc);
  repathing with full A\* every tick against a moving goal would be
  needlessly expensive for the common case of a target that's barely
  moved. Reasonable v0 policy: only repath when the live target has moved
  more than some threshold distance from the endpoint of the currently-held
  path (a "close enough, keep walking the existing path" tolerance) —
  flagged as a tuning constant to land empirically, not derived from
  content, same posture `LOCOMOTION_WALK_SPEED` already has.

Wander/Travel are the natural first consumers — they already resolve a
single frozen destination once, so plugging in `NavPath` is close to a pure
addition with no new state-machine shape. Follow/Escort/Guard/Patrol follow
once the shared piece is proven.

## 7. Multi-tile scale and query cost

Worst case, a path could span many resident tiles (a long corridor, or a
worldspace with many small NAVM tiles per cell). A\* over a few dozen
resident tiles' triangles (a typical resident radius, per
`exterior-grid-streaming.md`'s existing streaming radius) is well within a
single frame's budget for the request rate this actually needs (one path
per AI-locomotion target resolution — not a per-tick cost, since paths are
computed once and consumed incrementally, same as `TravelState.destination`
today). No case has been identified where this needs a hard cap or an
async/multi-frame budget the way, say, NIF streaming does — but if profiling
against real content later shows otherwise, the fix is bounding A\*'s
open-set expansion (a max node count, falling back to §4's "reachable
prefix, straight-line remainder" degrade), not a redesign.

## 8. Query indexing — deferred decision

§4 and §5 both describe scanning `world.query::<NavmeshTile>()` directly.
At today's per-cell tile counts (FO3/FNV: 1 tile per interior cell,
Skyrim-era: variable but small per exterior grid cell) a linear scan over
resident tiles is almost certainly fine and building a spatial index (grid
hash, BVH) ahead of any measured cost would be exactly the kind of
speculative optimization this project's own conventions warn against
(`feedback_speculative_vulkan_fixes` — same "don't guess a failure mode
that isn't visible to a test/measurement" principle applies past the
renderer specifically named there). Land the linear-scan version first; if
a soak or a specific slow-path measurement later shows tile count matters,
add indexing then, against real evidence.

## 9. Rollout

1. **Phase 1 — single-tile pathing.** A\* + funnel over one `NavmRecord`'s
   triangles, no cross-tile connectivity yet. Fully testable with synthetic
   small triangle meshes (a handful of hand-built triangles with known
   adjacency) — doesn't need real game data for algorithm correctness,
   though the existing corpus (`indices_are_in_range`-verified real meshes)
   is available for a perf/soak pass once the algorithm is proven.
2. **Phase 2 — cross-tile via `external_connections`.** Extends the A\*
   graph across `NavmeshTile` entity boundaries per §4. Needs at least one
   multi-tile fixture — either a small hand-built two-mesh pair, or (once
   Phase 1 is solid) a real adjacent-cell pair from the corpus.
3. **Phase 3 — wire into Wander/Travel** (§6's easy consumers first).
4. **Phase 4 — wire into Follow/Escort/Guard/Patrol**, including §6's
   repath-threshold tuning for Follow specifically.
5. **Not this document's job, sequenced after**: FO4 `NVNM` body decode
   (unblocks Phase 1+ for FO4 content) and a real door-triangle decode
   (unblocks door-aware pathing) are both separate, already-identified
   prerequisites for full coverage — neither blocks landing Phases 1–4 for
   the five games that already have decoded geometry.

## 10. What this document does NOT decide

- **The exact `NavPath`/waypoint data structure** (a component vs. some
  other storage shape) — implementation detail for whoever picks this up.
- **The A\* open-set/closed-set concrete implementation** (a `BinaryHeap`-
  based priority queue is the obvious default; not pinned here).
- **Follow's repath-threshold constant's actual value** — flagged in §6 as
  something to land empirically, not derive.
- **Whether `find_containing_triangle` (§5) ever needs a spatial index** —
  deferred per §8, pending real evidence.
- **Door/cover triangle decode** — a real prerequisite for full coverage,
  named in §2 and §9, but its own separate scoping effort (a fresh
  corpus investigation into `NVDP`/`NVCA`/`NVGD` and the `NVNM` door/cover
  blocks), not attempted here.
