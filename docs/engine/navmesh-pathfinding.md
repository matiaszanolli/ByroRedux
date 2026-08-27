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

   **Correction, Phase 1 (2026-08-23)**: not landed as planned — see §9's
   Phase 1 entry. A full funnel pass needs each portal's two vertices in
   a consistent left/right orientation along the corridor, which needs a
   winding convention this codebase doesn't confirm anywhere. Phase 1
   landed shared-edge-midpoint waypoints instead (correct, in-corridor,
   just not shortest-path-optimal); a true funnel pass is a deferred
   follow-up, not abandoned.

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

1. **Phase 1 — single-tile pathing. Landed (2026-08-23),
   `byroredux/src/systems/navmesh_path.rs`, with a real correction to
   §3's plan**: full funnel string-pulling turned out to need each
   corridor portal's two vertices in a *consistent left/right
   orientation* along the path, which in turn needs a corpus-confirmed
   consistently-wound `NavmTriangle::vertices` convention — not
   established anywhere in this codebase (`parse_navm`'s own doc is
   silent on winding), and not safely guessable without either that
   confirmation or interactive visual verification (unavailable in this
   environment). Landed instead: A\* over the triangle-adjacency graph
   (as planned, centroid-cost edges — edge-midpoint costing was also
   deferred for the same unconfirmed-convention reason), plus waypoint
   extraction as the **midpoint of each shared edge** between corridor
   triangles, where the shared edge is derived from actual shared vertex
   *indices* (`shared_edge`, orientation-free — provably correct for a
   watertight mesh, no convention assumed). This is a real, valid,
   in-corridor path — a genuine improvement over today's
   straight-line-through-walls locomotion — just not shortest-path
   optimal the way a true funnel pass would be. Upgrading to full
   string-pulling is a well-scoped, isolated follow-up once winding is
   confirmed against real data, not blocked on anything else landing
   first. 9 unit tests (point localization on both sides of a split
   quad, trivial same-triangle path, multi-triangle corridor traversal,
   off-navmesh `None`, `shared_edge` derivation + non-adjacency). Also
   surfaced and fixed one thing this document didn't call out: NAVM
   vertices are raw Z-up floats (`parse_navm` applies no coordinate
   conversion), while every other locomotion-facing API is engine Y-up —
   the module converts at read time via `zup_to_yup_pos`, documented in
   its own header since missing it would silently produce a
   rotated/mirrored path.
2. **Phase 2 — cross-tile via `external_connections`. Blocked, corpus-
   verified (2026-08-23), not merely assumed.** `NavmExternalConnection`
   names the *destination* mesh + triangle but has no confirmed field for
   the **source** triangle within the current mesh — `unknown: u32` was
   already flagged as "not established by the corpus" (§ this doc
   predates), and a direct test of the obvious hypothesis (does `unknown`
   name the source triangle index?) refutes it: swept all 94,543 external
   connections across `FalloutNV.esm`'s 4,771 meshes, and while
   `unknown < triangle_count` holds 100% of the time (weak evidence —
   `unknown` values are usually small regardless), the triangle it would
   name has an actual border edge (`edge_neighbours` containing `None` —
   the only kind of triangle that could plausibly originate a link to
   another mesh) only **32.2%** of the time. A real per-triangle source
   join would need to be ~100%, not one-third. Without a confirmed source
   triangle, a cross-tile A\* graph can't be built precisely — only "this
   whole tile connects somehow to that tile," not "crossing from
   triangle X lands you in triangle Y." Needs its own corpus investigation
   (candidate: `unknown` indexes something *other* than a raw triangle
   index — e.g. the Nth border edge in scan order, or an edge-slot
   position — not tested yet) before Phase 2 can land precisely; the
   alternative (treat a tile-to-tile link as connecting via an arbitrary
   border triangle) is a real approximation this doc isn't willing to
   ship silently. Deferred, not guessed past.

   **Resolved (2026-08-27, #3300) — `unknown` cannot carry the source
   triangle, and the 32.2% figure above was measuring chance.** A value
   census over the same 94,543 `FalloutNV.esm` connections shows `unknown`
   takes exactly **three distinct values across the entire corpus**:
   `0` (94,303 rows), `1` (120) and `2` (120). It is a 3-valued enum or
   edge ordinal, not an index into anything — which is also why
   `unknown < triangle_count` held "100% of the time" and why the
   border-edge test landed near one-third: with `unknown` almost always
   `0`, that test was really asking how often *triangle 0* happens to own
   a border edge. Both prior results were artifacts. The candidate
   follow-up this doc floated ("the Nth border edge in scan order") is
   refuted by the same census — three distinct values cannot enumerate a
   per-mesh edge list.

   **Positional correspondence is refuted too.** If the source triangle
   were implied by row order, `NVEX` row count would equal the mesh's
   border-edge or border-triangle count. It equals the border-**edge**
   count on 123/4,105 FNV meshes (3.0%) and 108/5,521 FO3 (2.0%); the
   border-**triangle** count on 204/4,105 (5.0%) and 206/5,521 (3.7%).
   The residual is signless (rows exceed border triangles about as often
   as they fall short), so it is not an off-by-one either.

   **What would resolve it: geometry, with a measured bound.** The source
   join is not in the sub-record at all, so it has to be recovered from
   the meshes themselves — matching this mesh's border-edge vertices
   against the neighbour's in world space. Viability measured, not
   assumed: over 3,000 distinct adjacent mesh pairs on `FalloutNV.esm`,
   **67.1% share at least one exactly-equal vertex** (rounded to 1e-2
   world units), and where they share any, the median count is **2** —
   precisely the two endpoints of one shared border edge. So exact vertex
   identity recovers the join for roughly two thirds of adjacent pairs
   outright; the remaining third needs a tolerance sweep to bound (the
   two meshes are authored separately and need not share exact
   coordinates). That is the accuracy bound Phase 2 would have to accept
   and state, and it is a real number rather than a hope — but it is a
   different mechanism from "decode a field", so Phase 2 stays deferred
   pending a decision to build it.
3. **Phase 3 — wire into Wander/Travel** (§6's easy consumers first).
   **Travel half landed (2026-08-23)**, `byroredux/src/systems/travel.rs`
   + `crate::components::NavPath` (the cached-waypoint-queue component
   §6 anticipated, byroredux-crate-local, `NOT_SAVED_BY_DESIGN` —
   rederived on demand same as `NavmeshTile`). Routes through a resident
   tile's triangle corridor via `navmesh_path::path_from_resident_tiles`
   when the actor's current position localizes onto one; caches per
   `(entity, goal)` and only recomputes on a goal change, including
   caching the negative "no path found" result so an off-navmesh actor
   doesn't retry every tick — the design doc's §7 "computed once" cost
   posture, honored for real. 3 new tests, including one that pins an
   exact post-tick position to 1e-3 precision proving the actor actually
   routes through the shared-edge waypoint rather than a straight line
   that happens to look similar. **Wander half deliberately deferred**:
   `step_oscillating_wander` is shared verbatim with `patrol_system`
   (out of Phase 3's scope), and threading a waypoint override through
   that shared primitive without silently changing Patrol's behavior
   needs its own pass — see `wander.rs`'s module doc for the full
   reasoning. Travel's integration needed no such change (it already
   calls `step_toward` directly, one-shot, not shared with anything).
4. **Phase 4 — wire into Follow/Escort/Guard/Patrol. Landed (2026-08-23),
   completing Phase 3's deferred Wander half too.** All six locomotion
   systems now consume single-tile pathing:
   - `guard_system` — frozen-goal (leash anchor), identical shape to
     Travel; only resolves a path while beyond the leash.
   - `follow_system` — introduced the repath-threshold family
     (`FOLLOW_REPATH_THRESHOLD = 64.0`, half of
     `FOLLOW_DEFAULT_DISTANCE`'s scale, an engine default per this
     section's own flag below). Landing this surfaced and fixed a real
     bug in `resolve_cached_waypoints`'s original contract: every caller
     was writing its own tick-local goal back into the cache regardless
     of whether the reused waypoints came from a hit or a miss, which is
     invisible for a frozen goal (Travel/Guard) but silently defeats a
     repath threshold entirely for a moving one — the cached goal crept
     toward the live target every tick, so the "has it moved far enough"
     check could never fire. Fixed by having the function return the
     *effective* goal (the cached one on a hit, the new one only on a
     miss) instead of leaving that judgment to callers. 3 direct unit
     tests now pin this contract, one of them a named regression guard.
   - `escort_system` — both phases in one file: the lead phase reuses
     Travel's frozen-goal shape, the collect phase reuses Follow's
     repath-threshold shape (own constant,
     `ESCORT_COLLECT_REPATH_THRESHOLD`, cross-referenced not imported —
     matches this module's existing `ESCORT_COLLECT_DISTANCE` convention
     for `FOLLOW_DEFAULT_DISTANCE`). One `NavPath` serves both phases in
     sequence; the collect→lead transition naturally invalidates it via
     the ordinary goal-mismatch path, no special-casing needed.
   - `wander_system`/`patrol_system` — the deferred piece. Both share
     `step_oscillating_wander` verbatim, which gained one new parameter
     (`waypoint_override: Option<Vec3>`) rather than any NAVM awareness
     of its own: it still only decides *when* to pause/re-pick, using
     `state.target` for that check unconditionally; the caller resolves
     and consumes the resident-tile waypoint queue and passes in which
     point to actually step toward this tick. `None` reproduces the
     exact pre-Phase-4 straight-line behavior, so Patrol's existing
     tests needed no changes despite the shared primitive's signature
     changing under it. The path cache is skipped entirely while
     `Paused` (nothing to walk toward that tick) and dropped the instant
     the final wander target is reached, mirroring every other system's
     "arrived → clear the cache" posture.

   Every one of the five new integrations shipped with its own
   distinguishing test asserting an exact post-tick position, proving
   the actor actually routed through the shared-edge waypoint rather
   than a straight line that happens to look similar for a naively
   chosen start/goal pair.
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
  something to land empirically, not derive. Landed at `64.0` (§9 Phase
  4); still just a starting engine default, not tuned against real
  gameplay observation.
- **Whether `find_containing_triangle` (§5) ever needs a spatial index** —
  deferred per §8, pending real evidence.
- **Door/cover triangle decode** — a real prerequisite for full coverage,
  named in §2 and §9, but its own separate scoping effort (a fresh
  corpus investigation into `NVDP`/`NVCA`/`NVGD` and the `NVNM` door/cover
  blocks), not attempted here.
